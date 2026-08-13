use super::*;

pub(super) async fn get_single_tool_version_impl(
    tool: &str,
    wsl_shell: Option<&str>,
    wsl_shell_flag: Option<&str>,
) -> ToolVersion {
    debug_assert!(
        VALID_TOOLS.contains(&tool),
        "unexpected tool name in get_single_tool_version_impl: {tool}"
    );

    // 判断该工具的运行环境 & WSL distro（如有）
    let (env_type, wsl_distro) = tool_env_type_and_wsl_distro(tool);

    // 使用全局 HTTP 客户端（已包含代理配置）
    let client = crate::proxy::http_client::get();

    // 1. 获取本地版本
    let probe = if let Some(distro) = wsl_distro.as_deref() {
        try_get_version_wsl(tool, distro, wsl_shell, wsl_shell_flag)
    } else {
        #[cfg(target_os = "windows")]
        {
            // Windows 上只执行已经定位到的真实可执行文件，避免 `cmd /C tool`
            // 误触发 App Execution Alias 或协议处理器。
            scan_cli_version(tool)
        }

        #[cfg(not(target_os = "windows"))]
        {
            // PATH 第一个命令优先；只有它确实没装(NotFound)才去常见目录兜底扫描。
            match try_get_version(tool) {
                ShellProbe::NotFound(_) => scan_cli_version(tool),
                found => found,
            }
        }
    };
    let (local_version, local_error, installed_but_broken) = match probe {
        ShellProbe::Found(v) => (Some(v), None, false),
        ShellProbe::FoundButFailed(e) => (None, Some(e), true),
        ShellProbe::NotFound(e) => (None, Some(e), false),
    };

    // 2. 获取远程最新版本。Homebrew Cask 是独立发布渠道；若仍拿 npm 的版本比较，
    // Cask 已是渠道最新版时会被错误标成“可升级”。
    let local = local_version.as_deref();
    let upstream_latest_version = fetch_upstream_latest_version(&client, tool, local).await;
    let cask_token = if wsl_distro.is_none() {
        homebrew_cask_token_for_path_default(tool)
    } else {
        None
    };
    let (latest_version, latest_version_source, upstream_latest_version) =
        if let Some(cask_token) = cask_token {
            resolve_homebrew_cask_latest_version(
                fetch_homebrew_cask_latest_version(&client, &cask_token).await,
                upstream_latest_version,
            )
        } else {
            (upstream_latest_version, None, None)
        };

    ToolVersion {
        name: tool.to_string(),
        version: local_version,
        latest_version,
        latest_version_source,
        upstream_latest_version,
        error: local_error,
        installed_but_broken,
        env_type,
        wsl_distro,
    }
}

/// 查询工具默认发布源的最新版本。npm 工具在本地领先 latest 时会按预发布通道补查，
/// 见 `fetch_npm_latest_for_tool` / `npm_prerelease_tags`。
pub(super) async fn fetch_upstream_latest_version(
    client: &reqwest::Client,
    tool: &str,
    local_version: Option<&str>,
) -> Option<String> {
    match tool {
        "claude" => {
            fetch_npm_latest_for_tool(client, "@anthropic-ai/claude-code", tool, local_version)
                .await
        }
        "codex" => fetch_npm_latest_for_tool(client, "@openai/codex", tool, local_version).await,
        "gemini" => {
            fetch_npm_latest_for_tool(client, "@google/gemini-cli", tool, local_version).await
        }
        "opencode" => {
            if let Some(version) =
                fetch_npm_latest_for_tool(client, "opencode-ai", tool, local_version).await
            {
                Some(version)
            } else {
                fetch_github_latest_version(client, "anomalyco/opencode").await
            }
        }
        "openclaw" => fetch_npm_latest_for_tool(client, "openclaw", tool, local_version).await,
        "hermes" => fetch_pypi_latest_version(client, "hermes-agent").await,
        // xAI's changelog is protected by Cloudflare for non-browser clients. Its official
        // npm distribution provides the same stable release through a public `latest` tag.
        "grok" => {
            fetch_npm_latest_for_tool(client, "@xai-official/grok", tool, local_version).await
        }
        _ => None,
    }
}

/// 从 Homebrew Formulae API 取得 Cask 的当前发布版本。
pub(super) async fn fetch_homebrew_cask_latest_version(
    client: &reqwest::Client,
    cask_token: &str,
) -> Option<String> {
    let url = format!("https://formulae.brew.sh/api/cask/{cask_token}.json");
    let response = client.get(url).send().await.ok()?;
    if !response.status().is_success() {
        return None;
    }

    response
        .json::<serde_json::Value>()
        .await
        .ok()?
        .get("version")?
        .as_str()
        .map(str::trim)
        .filter(|version| !version.is_empty())
        .map(str::to_string)
}

pub(super) const LATEST_VERSION_SOURCE_HOMEBREW_CASK: &str = "homebrew_cask";

/// Cask 元数据可用时，版本比较必须以 Cask 为准；官方版本仅作为“渠道滞后”的说明。
/// 元数据请求失败时保守回退既有上游版本检查，避免网络瞬断把已安装工具显示成未知。
pub(super) fn resolve_homebrew_cask_latest_version(
    cask_latest_version: Option<String>,
    upstream_latest_version: Option<String>,
) -> (Option<String>, Option<String>, Option<String>) {
    match cask_latest_version {
        Some(cask_latest_version) => {
            let upstream_latest_version = upstream_latest_version
                .filter(|upstream_version| upstream_version != &cask_latest_version);
            (
                Some(cask_latest_version),
                Some(LATEST_VERSION_SOURCE_HOMEBREW_CASK.to_string()),
                upstream_latest_version,
            )
        }
        None => (upstream_latest_version, None, None),
    }
}

/// 该工具在 npm 上的预发布通道 tag(靠前者优先)。仅当本地版本已**严格领先**
/// `latest` 时才会被补查 —— 让主动在抢先通道的用户(如走 Claude Code 的 `next`)
/// 看到与所在通道对齐的"最新版本",同时绝不把稳定通道用户暴露给预发布版。
/// 返回空切片表示该工具只看 `latest`、不补查。
///
/// 为何不通用覆盖所有工具:各家预发布 tag 命名互不统一(codex=alpha/beta/native、
/// gemini=nightly/preview、openclaw=alpha/beta),且 codex 的 beta/native 是
/// `0.1.x` 时间戳式版本、gemini 有误发的 `false` tag —— 这些脏值虽会被
/// `pick_latest_version` 的版本比较挡掉,但维护成本与误报风险不值当,故暂只为
/// Claude Code 启用。
pub(super) fn npm_prerelease_tags(tool: &str) -> &'static [&'static str] {
    match tool {
        "claude" => &["next"],
        _ => &[],
    }
}

/// 解析 "2.1.156" / "2.1.156-beta.1" → (主版本三段, 预发布段)。无法解析返回 None。
/// 与前端 `src/lib/version.ts` 的 parseVersion 语义对称(跨语言各实现一份)。
/// patch 用 u64 以容纳 codex 的 `0.1.2505172116` 时间戳式版本而不溢出。
pub(super) fn parse_semver(v: &str) -> Option<([u64; 3], Vec<String>)> {
    // 忽略 `+build` 元数据,再以首个 `-` 切出预发布段。
    let core_and_pre = v.trim().split('+').next().unwrap_or("");
    let (core, pre) = match core_and_pre.split_once('-') {
        Some((c, p)) => (c, Some(p)),
        None => (core_and_pre, None),
    };
    let mut parts = core.split('.');
    let major = parts.next()?.parse::<u64>().ok()?;
    let minor = parts.next()?.parse::<u64>().ok()?;
    let patch = parts.next()?.parse::<u64>().ok()?;
    if parts.next().is_some() {
        return None; // 多于三段,非法
    }
    let pre_segments = pre
        .map(|p| p.split('.').map(|s| s.to_string()).collect())
        .unwrap_or_default();
    Some(([major, minor, patch], pre_segments))
}

/// 比较两个版本号(遵循 semver:主版本三段优先;core 相等时有预发布 < 无预发布;
/// 预发布段逐段比 —— 数字段按数值、数字段 < 非数字段、非数字段按 ASCII、前缀相同
/// 则段更多者更大)。任一无法解析返回 None,调用方据此保守处理。
pub(super) fn compare_semver(a: &str, b: &str) -> Option<std::cmp::Ordering> {
    use std::cmp::Ordering;
    let (ac, ap) = parse_semver(a)?;
    let (bc, bp) = parse_semver(b)?;
    for i in 0..3 {
        match ac[i].cmp(&bc[i]) {
            Ordering::Equal => continue,
            other => return Some(other),
        }
    }
    match (ap.is_empty(), bp.is_empty()) {
        (true, true) => return Some(Ordering::Equal),
        (true, false) => return Some(Ordering::Greater),
        (false, true) => return Some(Ordering::Less),
        (false, false) => {}
    }
    for (x, y) in ap.iter().zip(bp.iter()) {
        let ord = match (x.parse::<u64>(), y.parse::<u64>()) {
            (Ok(xv), Ok(yv)) => xv.cmp(&yv),
            (Ok(_), Err(_)) => Ordering::Less, // 数字段 < 非数字段
            (Err(_), Ok(_)) => Ordering::Greater,
            (Err(_), Err(_)) => x.as_str().cmp(y.as_str()),
        };
        if ord != Ordering::Equal {
            return Some(ord);
        }
    }
    Some(ap.len().cmp(&bp.len()))
}

/// 从一次 registry 请求得到的完整 dist-tags 出发,挑选要展示的"最新版本"。
///
/// 规则:默认就是 `latest`;仅当本地版本已**严格领先** `latest`(说明用户主动在
/// 抢先通道)时,才把 `prerelease_tags` 指向的版本纳入比较,取其中能被解析、且
/// 高于 `latest` 的最高者。无法解析或不高于 latest 的脏 tag 一律落选。
pub(super) fn pick_latest_version(
    dist_tags: &serde_json::Map<String, serde_json::Value>,
    prerelease_tags: &[&str],
    local_version: Option<&str>,
) -> Option<String> {
    use std::cmp::Ordering;
    let latest = dist_tags.get("latest").and_then(|v| v.as_str())?;

    // 本地是否严格领先 latest;任一无法解析则按"未领先"保守处理(只看 latest)。
    let local_ahead = local_version
        .and_then(|local| compare_semver(local, latest))
        .map(|ord| ord == Ordering::Greater)
        .unwrap_or(false);
    if prerelease_tags.is_empty() || !local_ahead {
        return Some(latest.to_string());
    }

    let mut best = latest.to_string();
    for tag in prerelease_tags {
        if let Some(candidate) = dist_tags.get(*tag).and_then(|v| v.as_str()) {
            if compare_semver(candidate, &best) == Some(Ordering::Greater) {
                best = candidate.to_string();
            }
        }
    }
    Some(best)
}

/// 拉取 npm 包的完整 dist-tags(单次请求即含 latest/next/beta/...)。
pub(super) async fn fetch_npm_dist_tags(
    client: &reqwest::Client,
    package: &str,
) -> Option<serde_json::Map<String, serde_json::Value>> {
    let url = format!("https://registry.npmjs.org/{package}");
    let resp = client.get(&url).send().await.ok()?;
    let json = resp.json::<serde_json::Value>().await.ok()?;
    json.get("dist-tags")?.as_object().cloned()
}

/// 查询某 npm 工具要展示的"最新版本":取 `latest`,并在本地版本领先时按工具的
/// 预发布通道(见 `npm_prerelease_tags`)补查 —— 复用同一次 registry 响应,无额外请求。
pub(super) async fn fetch_npm_latest_for_tool(
    client: &reqwest::Client,
    package: &str,
    tool: &str,
    local_version: Option<&str>,
) -> Option<String> {
    let dist_tags = fetch_npm_dist_tags(client, package).await?;
    pick_latest_version(&dist_tags, npm_prerelease_tags(tool), local_version)
}

/// Helper function to fetch latest version from GitHub releases
pub(super) async fn fetch_github_latest_version(
    client: &reqwest::Client,
    repo: &str,
) -> Option<String> {
    let url = format!("https://api.github.com/repos/{repo}/releases/latest");
    match client
        .get(&url)
        .header("User-Agent", "agentswitch")
        .header("Accept", "application/vnd.github+json")
        .send()
        .await
    {
        Ok(resp) => {
            if let Ok(json) = resp.json::<serde_json::Value>().await {
                json.get("tag_name")
                    .and_then(|v| v.as_str())
                    .map(|s| s.strip_prefix('v').unwrap_or(s).to_string())
            } else {
                None
            }
        }
        Err(_) => None,
    }
}

/// Helper function to fetch latest version from PyPI
pub(super) async fn fetch_pypi_latest_version(
    client: &reqwest::Client,
    package: &str,
) -> Option<String> {
    let url = format!("https://pypi.org/pypi/{package}/json");
    match client.get(&url).send().await {
        Ok(resp) => {
            if let Ok(json) = resp.json::<serde_json::Value>().await {
                json.get("info")
                    .and_then(|info| info.get("version"))
                    .and_then(|v| v.as_str())
                    .map(|s| s.to_string())
            } else {
                None
            }
        }
        Err(_) => None,
    }
}

/// 预编译的版本号正则表达式
pub(super) static VERSION_RE: Lazy<Regex> =
    Lazy::new(|| Regex::new(r"\d+\.\d+\.\d+(-[\w.]+)?").expect("Invalid version regex"));

/// 从版本输出中提取纯版本号
pub(super) fn extract_version(raw: &str) -> String {
    VERSION_RE
        .find(raw)
        .map(|m| m.as_str().to_string())
        .unwrap_or_else(|| raw.to_string())
}
