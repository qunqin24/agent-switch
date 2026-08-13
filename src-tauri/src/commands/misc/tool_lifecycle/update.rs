use super::*;

/// 工具对应的 npm 包名（hermes 走自己的 CLI/installer，不在此表）。锚定升级据此拼 `npm i -g`。
/// 全平台共用一张表——Windows 锚定层(`anchored_command_from_paths` 的 windows 版)也读这里。
pub(super) fn npm_package_for(tool: &str) -> Option<&'static str> {
    match tool {
        "claude" => Some("@anthropic-ai/claude-code"),
        "codex" => Some("@openai/codex"),
        "gemini" => Some("@google/gemini-cli"),
        "opencode" => Some("opencode-ai"),
        "openclaw" => Some("openclaw"),
        _ => None,
    }
}

/// 取路径的父目录(纯字符串截断,不碰 fs):`/a/b/npm` → `/a/b`、`C:\a\b\npm.cmd`
/// → `C:\a\b`、混合分隔符 `C:\a/b\npm` → `C:\a/b`。无父目录返回空串。
///
/// 平台无关:`\` 和 `/` 都识别,取两者最右出现位置。`Option<usize>` 的 Ord 让
/// `None < Some(_)`,所以 `rfind('\\').max(rfind('/'))` 自动取存在的那个、两者都
/// 存在时取靠右的——比 `or_else` 优先取一种正确(混合分隔符不会拿错父目录)。
/// 跨平台 fs separator 在两侧均接受,使 macOS/Linux 上的 cargo test 也能跑 Windows
/// 路径用例(`parent_dir_cases::mixed_separators_takes_rightmost`)。空串语义由上游
/// `sibling_bin` 的 `is_empty()` 检查转成 None → 锚定整体退化到静态兜底。
pub(super) fn parent_dir(p: &str) -> String {
    match p.rfind('\\').max(p.rfind('/')) {
        Some(i) if i > 0 => p[..i].to_string(),
        _ => String::new(),
    }
}

/// 从 canonicalize 后的真身路径提取 Homebrew formula 名：
/// `/opt/homebrew/Cellar/gemini-cli/0.13.0/...` → `Some("gemini-cli")`。
/// 非 Cellar 路径（= 不是 formula，可能是 Cask 或 Homebrew 的 node 装的 npm 全局包）返回 None。
/// 关键区分：formula 即便内部用 node，真身也落在 `Cellar/<formula>/` 下；而 Homebrew
/// npm 全局包落在 `/opt/homebrew/lib/node_modules`（不含 Cellar）。三者升级命令不同。
#[cfg(not(target_os = "windows"))]
pub(super) fn brew_formula_from_path(real: &str) -> Option<String> {
    let mut segs = real.split('/');
    while let Some(seg) = segs.next() {
        if seg.eq_ignore_ascii_case("Cellar") {
            return segs.next().filter(|s| !s.is_empty()).map(|s| s.to_string());
        }
    }
    None
}

/// 从 canonicalize 后的真身路径提取 Homebrew Cask token：
/// `/opt/homebrew/Caskroom/claude-code/2.1.193/claude` → `Some("claude-code")`。
/// Cask 安装由 Homebrew 管理，不能交给 CLI 的自更新或 npm；两者都会绕开 Caskroom
/// 当前命中的版本，导致“命令成功但版本没变”或依赖 GUI PATH 中 node 的失败。
#[cfg(not(target_os = "windows"))]
pub(super) fn brew_cask_from_path(real: &str) -> Option<String> {
    let mut segs = real.split('/');
    while let Some(seg) = segs.next() {
        if seg.eq_ignore_ascii_case("Caskroom") {
            return segs.next().filter(|s| !s.is_empty()).map(|s| s.to_string());
        }
    }
    None
}

/// PATH 默认入口若落在 Homebrew Caskroom，返回其 Cask token。
#[cfg(not(target_os = "windows"))]
pub(super) fn homebrew_cask_token_for_path_default(tool: &str) -> Option<String> {
    let real = resolve_path_default(tool)?;
    brew_cask_from_path(&real.to_string_lossy())
}

#[cfg(target_os = "windows")]
pub(super) fn homebrew_cask_token_for_path_default(_tool: &str) -> Option<String> {
    None
}

/// 含空格才用 POSIX 单引号包一层,否则保持裸路径——命令展示更干净。
/// claude / brew / volta / bun / npm 五个锚定分支共用,避免"含空格"判定漂移。
///
/// **仅按空格判定,不防其他 shell 元字符**(`$` / `` ` `` / `'` / `"` / `;` 等)。
/// 调用方传入的是探测得到的可执行路径(`enumerate_tool_installations` 里来源于
/// `Path::display()`),实际 macOS/Linux 上 home dir 名几乎不允许这类字符、
/// npm/brew/volta/bun 也不会装到含这类字符的路径,与 diff 前内联在 npm 分支里的
/// `if npm.contains(' ')` 实现等价。若未来要扩广,改成 `shell_single_quote` 无条件
/// 包裹即可,但会失去"无空格时的清洁展示"。
#[cfg(not(target_os = "windows"))]
pub(super) fn quote_path_if_spaced(p: &str) -> String {
    if p.contains(' ') {
        shell_single_quote(p)
    } else {
        p.to_string()
    }
}

/// 锚定路径走 `.bat` 文件且**被 `call` 调用**,需要为 batch 特殊字符做两层防御:
///
/// **(1) `%` 经历两轮 percent expansion → 用 4 个 `%` 转义**。.bat 中字面 `%` 的
/// 标准转义是 `%%`,但 `call` 命令(Microsoft `call /?`:"percent (%) expansion is
/// performed on each parameter")**在 batch parser 处理完 `%%` → `%` 后自己再做一轮**。
/// 所以源 .bat 里写 `%%FOO%%`,batch 一轮变 `%FOO%`,call 二轮当成 variable reference
/// 又展开一次——要让最终 call 看到字面 `%FOO%` 必须写 `%%%%FOO%%%%`(一轮 → `%%FOO%%`,
/// 二轮 → `%FOO%` 字面)。这是 cmd 唯一**引号无法保护**的字符:引号内的 `%` 仍参与
/// 两轮 expansion。
///
/// **(2) token 边界 / escape 字符触发外层双引号**:`' '` `'&'` `'('` `')'` `'^'`
/// `';'` `'<'` `'>'` `'|'` `','` 任一出现即包引号。NTFS 允许这些字符出现在路径中,
/// 不包会让 cmd 把路径切成多 token、`^` 又会触发 escape;引号内它们是字面意义,
/// 而且 call 二次解析对引号内的它们也不会做特殊处理(`^` 在引号内失去 escape 作用,
/// token 边界字符在引号内是字面)。
///
/// `!`(delayed expansion)只在 `setlocal enabledelayedexpansion` 下生效——我们
/// .bat 头只有 `@echo off`、没开,所以不需要处理。`'` 在 cmd 中无特殊意义。
///
/// 镜像 POSIX `quote_path_if_spaced` 的"轻量条件包装"语义:不含任何特殊字符就保持
/// 裸路径(命令展示更干净),否则用 `win_double_quote` 包并做必要转义。
#[cfg(target_os = "windows")]
pub(super) fn win_quote_path_for_batch(p: &str) -> String {
    // `%` 经历两轮 expansion:.bat parser 一轮 + `call` 二轮(Microsoft `call /?`:
    // "percent (%) expansion is performed on each parameter")。要让 call 最终看到
    // 字面 `%` 需要 4 个 → `%%%%`(batch 一轮 → `%%`,call 二轮 → `%` 字面)。
    // 引号内仍参与两轮 expansion,所以这一步独立于外层引号、必须无条件做。
    let escaped = if p.contains('%') {
        p.replace('%', "%%%%")
    } else {
        p.to_string()
    };
    // 注:`needs_quote` 基于**原路径** `p` 判断,不能用 `escaped`——后者引入的 `%`
    // 字符不算"特殊触发字符",否则含 `%` 的路径会被错误地额外加引号。
    let needs_quote = p
        .chars()
        .any(|c| matches!(c, ' ' | '&' | '(' | ')' | '^' | ';' | '<' | '>' | '|' | ','));
    if needs_quote {
        win_double_quote(&escaped)
    } else {
        escaped
    }
}

/// Windows 版 sibling 推导:在 `<bin_path 父目录>` 下按 `ext_candidates` 顺序找
/// 第一个存在的 `<exe_basename>.<ext>` 文件,返回该绝对路径。
///
/// **与 POSIX `sibling_bin` 的关键区别:这里碰 fs**——Windows 上 npm/pnpm 的入口
/// 实际扩展名可能是 `.cmd` 也可能是 `.exe`(Node.js installer 装的是 `npm.cmd`、
/// 部分 pnpm 是 `pnpm.exe`),纯字符串拼接无法知道哪个真的存在,猜错会拼出
/// "GUI 执行时 file not found" 的命令。fs 检查放进 helper、单测用 tempdir 覆盖,
/// 让上层 `anchored_command_from_paths` 仍保持"接收已锚定路径"的接口形态。
///
/// **TOCTOU 是 by design**:预检 `is_file` 是为了让确认对话框展示真实命令字符串;
/// 检查到执行之间被外部进程(卸载器 / nvm switch / 杀软隔离)移走文件 → cmd /C
/// 报 ENOENT,toast 显示错误。不要在执行前再做二次预检——双重 syscall 也解决不了 race。
///
/// 候选扩展名顺序按工具 idiom:npm/pnpm 优先 `.cmd`(node 装的),volta 优先 `.exe`
/// (Volta 是 Rust 写的 native binary)。
///
/// **不用 `which::which_in` 的理由**:per-tool 扩展名优先级(volta 偏 `.exe`、npm/pnpm
/// 偏 `.cmd`)与 PATHEXT 的固定顺序不一致,而且只为这一处加 `which` 依赖收益不抵 audit
/// surface。`PathBuf::join` 让 separator 选择交给 std,避免 `format!("{dir}\\...")`
/// 硬编码 `\\` 在混合分隔符 bin_path 下产出丑陋路径。
///
/// 空 dir 或所有候选都不存在 → None,上游退化到静态命令,与 POSIX 路径同款语义。
#[cfg(target_os = "windows")]
pub(super) fn sibling_bin_with_ext(
    bin_path: &str,
    exe_basename: &str,
    ext_candidates: &[&str],
) -> Option<String> {
    let dir = parent_dir(bin_path);
    if dir.is_empty() {
        return None;
    }
    let dir = std::path::PathBuf::from(dir);
    for ext in ext_candidates {
        let candidate = dir.join(format!("{exe_basename}.{ext}"));
        if candidate.is_file() {
            return Some(candidate.to_string_lossy().into_owned());
        }
    }
    None
}

/// 返回 `<bin_path 同目录>/<exe>` 的绝对路径。bin_path 是命令行命中的入口
/// (如 `/opt/homebrew/bin/gemini`、`~/.volta/bin/codex`),`exe` 是与之共处一个
/// bin 目录的另一个可执行(`brew` / `volta` / `bun` / `npm`)——这些包管理器
/// 都把自己的 cli 跟它们安装的命令并列放在同一个 bin 目录,所以"同目录推导"
/// 是可靠的绝对路径来源。
///
/// **dir 为空(bin_path 不含 `/`) → 返回 None**:此时无法推导出绝对路径,让上游
/// `anchored_command_from_paths` 整体退化为 None,调用方落到静态命令兜底——而非
/// 悄悄拼出 `npm i -g <pkg>` 这种依赖 PATH 的指令,违背"必须绝对路径"不变量。
/// 实际从 `enumerate_tool_installations` 走的 bin_path 都是 `Path::display()` 出
/// 来的绝对路径,这条防线不期望被触发,但闭合了 helper 与函数文档的语义一致。
#[cfg(not(target_os = "windows"))]
pub(super) fn sibling_bin(bin_path: &str, exe: &str) -> Option<String> {
    let dir = parent_dir(bin_path);
    if dir.is_empty() {
        None
    } else {
        Some(format!("{dir}/{exe}"))
    }
}

#[cfg(not(target_os = "windows"))]
pub(super) fn anchored_official_update_command(tool: &str, bin_path: &str) -> Option<String> {
    official_update_args(tool).map(|args| format!("{} {args}", quote_path_if_spaced(bin_path)))
}

#[cfg(target_os = "windows")]
pub(super) fn anchored_official_update_command(tool: &str, bin_path: &str) -> Option<String> {
    official_update_args(tool).map(|args| format!("{} {args}", win_quote_path_for_batch(bin_path)))
}

/// 哪些工具的"官方 self-update"优先于包管理器升级（生成 `<tool> update || <pkg-mgr>`）。
///
/// Codex 的健康安装也优先走 `codex update`；真正损坏（`runnable=false`）时由
/// `installs_anchored_command` 的门控先改用 `codex_repair_command` 的 uninstall+install
/// 自愈，避免把已经跑不起来的 CLI 交给 self-update。
pub(super) fn prefers_official_update(tool: &str, shell: LifecycleCommandShell) -> bool {
    match shell {
        LifecycleCommandShell::Posix => {
            matches!(tool, "claude" | "codex" | "opencode" | "openclaw" | "grok")
        }
        LifecycleCommandShell::WindowsBatch => {
            matches!(
                tool,
                // OpenCode 的 Windows `upgrade` 在 anomalyco/opencode#17295 修复前可能因
                // 安装方式探测失败弹交互 prompt（spawn npm.cmd 没传 shell:true）；静默
                // lifecycle 没有 stdin 会挂死，Windows 先锚到包管理器路径，等上游修了
                // 再把 opencode 加回这里。
                "claude" | "codex" | "openclaw" | "grok"
            )
        }
    }
}

/// Codex 平台分发包损坏的自愈命令。Codex 的 npm 包是「主包 `@openai/codex`（纯 JS
/// launcher）+ 平台二进制 optional 依赖 `@openai/codex-<triple>`」的分发模式（同 esbuild/swc）。
/// 当平台二进制缺失时 codex 跑不起来——`enumerate_tool_installations` 跑 `--version` 会拿到
/// “Missing optional dependency” 的非 0 退出，标记 `runnable=false`。此状态下普通
/// `npm i -g @pkg@latest` 是 **no-op**：npm 视 optional 依赖缺失为非致命，reify 又认为主包已是
/// 最新（外加半损坏留下的空 nested `node_modules` 残骸强化「tree 已满足」判断），不会补回平台
/// 二进制。唯一实测可靠的修复是先 `uninstall` 清掉残骸、再 `install` 装回完整的主包 + 平台二进制
/// （实测输出 `added 2 packages`）。
///
/// 锚定到与 codex 入口同目录的 npm（与升级路径一致，不依赖 GUI 非登录进程的 PATH）。`|| true`
/// 让 uninstall 失败（如 nvm 上对半损坏包静默返回非 0）不触发外层 `set -e` 中止，但随后的
/// install 若失败仍会被 `set -e` 捕获并上报给前端 toast。
///
/// **仅对会锚定到 sibling npm 的 node 管理器来源（nvm/fnm/mise/homebrew npm）生效**：
/// `runnable=false` 是宽信号（权限 / node 版本 / 任意 `--version` 失败皆可触发），非 npm
/// 全局安装各有自己的二进制分发与修复方式，无脑套 npm uninstall+install 会出错——Homebrew
/// formula（real 在 `Cellar/`）本应 `brew upgrade codex`，npm 够不到它反而旁路装第二份 npm
/// 全局 codex；Volta/Bun 本应 `volta install`/`bun add`，且 `~/.bun/bin` 下没有 npm、
/// `sibling_bin` 会拼出不存在的路径；system/未知来源无可靠 sibling npm。这些来源一律返回
/// None，让上游继续走 source-specific 的 `anchored_command_from_paths`。白名单与
/// `package_manager_anchored_command_from_paths` 的 sibling-npm 分支对齐。
/// 刻意**不**额外用 `inst.error` 文本确认「确系缺二进制」：enumerate 只保留 stderr 末尾 4 行，
/// 而 codex.js 抛错的 "Missing optional dependency" 行会被尾部 node stack `at ...` 行挤出窗口
/// （实测用户原始错误即如此），强加该条件反而漏修真实缺包；对 npm 全局安装，uninstall+install
/// 对各类损坏都是合理且不会更糟的修复。
#[cfg(not(target_os = "windows"))]
pub(super) fn codex_repair_command(bin_path: &str, real: &str) -> Option<String> {
    // brew formula（real 在 Cellar）→ 不归 npm 管，交回 anchored 走 brew upgrade。
    if brew_formula_from_path(real).is_some() {
        return None;
    }
    // 只认会落到 sibling npm 的 node 管理器来源；volta/bun/system/未知交回 anchored。
    if !matches!(
        infer_install_source(Path::new(bin_path)),
        "nvm" | "fnm" | "mise" | "homebrew"
    ) {
        return None;
    }
    let npm = sibling_bin(bin_path, "npm")?;
    let npm = quote_path_if_spaced(&npm);
    let pkg = "@openai/codex";
    Some(format!(
        "{npm} uninstall -g {pkg} || true; {npm} i -g {pkg}@latest"
    ))
}

/// Windows 暂不做平台分发自愈：Windows 上 codex 的破坏模式不同（EPERM 文件锁 / 版本 bump
/// 残留，见 openai/codex#21872、#19824），且 `.bat` 链的错误处理与 POSIX `set -e` 语义不同，
/// 需要单独设计；先在本问题实际发生的 POSIX 平台落地。返回 None → 上游走正常锚定命令。
#[cfg(target_os = "windows")]
pub(super) fn codex_repair_command(_bin_path: &str, _real: &str) -> Option<String> {
    None
}

/// `codex_broken_package_command` 是 `installs_anchored_command` 在 codex
/// `runnable=false` 分支里、`codex_repair_command` 之后接续的第二级兜底。它复用
/// `package_manager_anchored_command_from_paths`，对**会锚定到各自包管理器的来源**
/// 走 source-specific 的锚定命令：Homebrew formula → `brew upgrade codex`、Volta →
/// `volta install @openai/codex`、Bun → `bun add -g`、nvm/fnm/mise/homebrew-npm →
/// 锚到 sibling npm 跑 `npm i -g`（与 runnable=true 的健康升级同一套锚定，只是此处
/// codex 跑不起来、不能走 `codex update`）。
///
/// **为什么 broken 分支走 pkg-mgr 锚定而非 self-update**：`runnable=false` 意味着
/// `codex --version` 已非 0 退出，把 `codex update` 交给一个跑不起来的 CLI 等于
/// 调用必然失败；而包管理器侧（brew/volta/bun/sibling npm）的升级不依赖 codex 自身
/// 可运行——它们直接重写主包 + 平台二进制，是 broken 状态下仍能修复的写回路径。
/// system / 未知来源没有可靠的 sibling 包管理器可锚定，本函数返回 None，交回上游
/// `anchored_command_from_paths` 走官方 CLI 自升级（见 `installs_anchored_command` 兜底）。
#[cfg(not(target_os = "windows"))]
pub(super) fn codex_broken_package_command(bin_path: &str, real: &str) -> Option<String> {
    package_manager_anchored_command_from_paths("codex", bin_path, real)
}

#[cfg(target_os = "windows")]
pub(super) fn codex_broken_package_command(bin_path: &str, _real: &str) -> Option<String> {
    package_manager_anchored_command_from_paths("codex", bin_path)
}

#[cfg(not(target_os = "windows"))]
pub(super) fn package_manager_anchored_command_from_paths(
    tool: &str,
    bin_path: &str,
    real_target: &str,
) -> Option<String> {
    if let Some(formula) = brew_formula_from_path(real_target) {
        let brew = sibling_bin(bin_path, "brew")?;
        return Some(format!("{} upgrade {formula}", quote_path_if_spaced(&brew)));
    }
    if let Some(cask) = brew_cask_from_path(real_target) {
        let brew = sibling_bin(bin_path, "brew")?;
        return Some(format!(
            "{} upgrade --cask {cask}",
            quote_path_if_spaced(&brew)
        ));
    }
    let pkg = npm_package_for(tool)?;
    match infer_install_source(Path::new(bin_path)) {
        "volta" => {
            let volta = sibling_bin(bin_path, "volta")?;
            return Some(format!("{} install {pkg}", quote_path_if_spaced(&volta)));
        }
        "bun" => {
            let bun = sibling_bin(bin_path, "bun")?;
            return Some(format!(
                "{} add -g {pkg}@latest",
                quote_path_if_spaced(&bun)
            ));
        }
        // 自带同级 npm 的 node 管理器：落到下面锚定到那处的 npm。
        "nvm" | "fnm" | "mise" | "homebrew" => {}
        // system / 未知来源通常没有同级 npm，不能拼 `<dir>/npm`。若工具有官方
        // self-update，上层会直接锚到 CLI 自身；否则返回 None 走静态兜底。
        _ => return None,
    }
    let npm = sibling_bin(bin_path, "npm")?;
    Some(format!("{} i -g {pkg}@latest", quote_path_if_spaced(&npm)))
}

/// 给定工具、原始 bin 路径（命令行命中的入口）、canonicalize 后的真身路径，
/// 推断"写回同一处"的锚定升级命令。**POSIX 版是纯函数（不碰 FS）**——真实 canonicalize
/// 由调用方做（`installs_anchored_command` 复用 enumerate 时算出的 `inst.real`),
/// 便于单测覆盖各包管理器分支。Windows 版同名函数因 sibling 扩展名歧义必须读 fs,
/// 是刻意保留的平台差异(详见 Windows 版本 doc)。
///
/// **关键不变量：返回的命令必须用绝对路径调用执行体，不依赖 PATH**。
/// 这条命令最终在 `run_tool_lifecycle_silently` 的非登录 `bash -c` 里执行——
/// GUI App 启动的进程 PATH 由 launchd / Windows Service / systemd 给,通常**不含**
/// `~/.local/bin` / `/opt/homebrew/bin` / `~/.volta/bin` 等用户级 bin 目录;而探测
/// 阶段 `try_get_version` 用的是 `$SHELL -lic`(登录+交互式,会读 .zshrc/.zprofile),
/// 两者 PATH 不对称。裸 `claude update` / `brew upgrade ...` 在 GUI 进程里大概率
/// `command not found`(exit 127)→ `set -e` 中止 → 用户看到失败 toast,锚定决策却
/// 已展示给用户"将写回原生那处"——欺骗性故障。
///
/// 判定顺序（命中即返回）：
/// ① Hermes → `<bin_path 绝对> update`;Hermes CLI 自己知道安装环境,避免 agentswitch
///    猜系统 `python3`/`python` 时撞上 Python 版本或 pyenv shim 问题。
/// ② Claude 原生安装器（`~/.local/share/claude/versions/`）→ `<bin_path 绝对> update`；
///    bin_path 指向 launcher,launcher 内部 dispatch update 子命令。它不归 npm 管,
///    且在 PATH 里比 nvm/homebrew 更靠前,用 npm 升级会装到别处且被原生那份遮蔽。
/// ③ Homebrew formula / Cask（真身在 `Cellar/<formula>/` / `Caskroom/<cask>/`）→
///    `<bin_path 同目录>/brew upgrade [--cask] <package>`；Homebrew 管理的安装不可交给
///    CLI 自更新，否则可能因 GUI PATH 缺 node 失败，或返回成功却仍命中旧 Cask 版本。
/// ④ 其余支持官方自升级的工具 → `<bin_path 绝对> update/upgrade || <原锚定包管理器命令>`；
///    Codex 的 self-update 只在部分 release 可用,所以保留 npm/brew/bun/volta fallback。
/// ⑤ 不支持官方自升级的 npm 全局包(例如 Gemini CLI) → 锚定到"那处 bin 目录的 npm"。
#[cfg(not(target_os = "windows"))]
pub(super) fn anchored_command_from_paths(
    tool: &str,
    bin_path: &str,
    real_target: &str,
) -> Option<String> {
    let real_lower = real_target.to_ascii_lowercase();

    if tool == "hermes" {
        return anchored_official_update_command(tool, bin_path);
    }
    if tool == "claude"
        && (real_lower.contains("/.local/share/claude/")
            || real_lower.contains("/claude/versions/"))
    {
        return anchored_official_update_command(tool, bin_path);
    }
    let package_command = package_manager_anchored_command_from_paths(tool, bin_path, real_target);
    if brew_formula_from_path(real_target).is_some() || brew_cask_from_path(real_target).is_some() {
        return package_command;
    }
    if prefers_official_update(tool, LifecycleCommandShell::Posix) {
        let update = anchored_official_update_command(tool, bin_path)?;
        return Some(match package_command {
            Some(fallback) => chain_update_commands(update, fallback, LifecycleCommandShell::Posix),
            None => update,
        });
    }
    package_command
}

#[cfg(target_os = "windows")]
pub(super) fn package_manager_anchored_command_from_paths(
    tool: &str,
    bin_path: &str,
) -> Option<String> {
    let pkg = npm_package_for(tool)?;

    match infer_install_source(Path::new(bin_path)) {
        "volta" => {
            let volta = sibling_bin_with_ext(bin_path, "volta", &["exe", "cmd"])?;
            Some(format!(
                "{} install {pkg}",
                win_quote_path_for_batch(&volta)
            ))
        }
        "pnpm" => {
            let pnpm = sibling_bin_with_ext(bin_path, "pnpm", &["cmd", "exe"])?;
            Some(format!(
                "{} add -g {pkg}@latest",
                win_quote_path_for_batch(&pnpm)
            ))
        }
        // 兜底 = npm 类:Scoop / Chocolatey / winget / nvm-windows / MS Store nodejs /
        // system / 任何识别不到专属来源的 → sibling npm.cmd。
        _ => {
            let npm = sibling_bin_with_ext(bin_path, "npm", &["cmd", "exe"])?;
            Some(format!(
                "{} i -g {pkg}@latest",
                win_quote_path_for_batch(&npm)
            ))
        }
    }
}

/// Windows 版锚定命令生成。对平台确认可静默运行的工具优先使用官方 CLI 自升级；
/// 对 npm/Volta/pnpm 这类可确认写回位置的安装，再接一个包管理器 fallback。不存在 brew/bun/claude-native
/// (Windows 没 Homebrew、Bun for Windows 仍 preview、claude.ai/install.sh 是 bash 脚本)。
/// Scoop/Chocolatey/winget/nvm-windows/MS Store node 都归 npm 类——它们都只是"如何装
/// node"的不同入口,全局包真正的 idiom 仍是 sibling `npm.cmd`。
///
/// **与 POSIX 版的语义差异**:POSIX 版是纯函数(不碰 fs),Windows 版通过
/// `sibling_bin_with_ext` 读 fs 来探明扩展名(`.cmd` vs `.exe`)——Node installer
/// 装 `.cmd`、Volta 装 `.exe`,纯字符串拼接无法消歧。这一平台差异**被刻意保留**:
/// 测试用 tempdir 隔离 fs,生产侧 TOCTOU 是 by design(见 `sibling_bin_with_ext` doc)。
///
/// `_real_target` 占位维持与 POSIX 版的签名对称——Windows 上未观测到需要真身路径
/// 区分的等价类(无 Cellar、无 claude-native installer)。若未来加 Scoop persist 锚定
/// (scoop 装的工具真身在 `<scoop_root>/persist/<app>/...`),从这里启用 `_real_target`。
///
/// **关键不变量同 POSIX 版:返回的命令必须用绝对路径,不依赖 PATH**。Windows GUI
/// 进程 PATH 由 Service Control Manager / explorer.exe 给,通常不含用户 `%LOCALAPPDATA%`
/// 下的 Volta/pnpm 路径;`$SHELL -lic` 的探测时 PATH 与执行时 PATH 不对称。
///
/// 判定顺序(命中即返回):
/// ① hermes → `<bin_path> update`;Hermes CLI 自己处理安装环境。
/// ② 支持官方自升级且 Windows 可安全静默执行的工具 → `<bin_path> update/upgrade || call <包管理器 fallback>`。
/// ③ 其余 npm 工具 → sibling `npm.cmd`/`.exe` i -g <pkg>@latest。
///
/// 包管理器 fallback 的 sibling 探测都通过 `sibling_bin_with_ext`(碰 fs):该处无候选
/// 扩展名存在时,支持官方自升级的工具仍返回 `<bin_path> update/upgrade`,其余工具
/// 才返 None 让上游兜回静态命令、`anchored=false`。
#[cfg(target_os = "windows")]
pub(super) fn anchored_command_from_paths(
    tool: &str,
    bin_path: &str,
    _real_target: &str,
) -> Option<String> {
    if tool == "hermes" {
        return anchored_official_update_command(tool, bin_path);
    }
    let package_command = package_manager_anchored_command_from_paths(tool, bin_path);
    if prefers_official_update(tool, LifecycleCommandShell::WindowsBatch) {
        let update = anchored_official_update_command(tool, bin_path)?;
        return Some(match package_command {
            Some(fallback) => {
                chain_update_commands(update, fallback, LifecycleCommandShell::WindowsBatch)
            }
            None => update,
        });
    }
    package_command
}

/// 从枚举结果里取"命令行实际命中的那处"：优先 `is_path_default`；否则（解析不到
/// PATH 默认、但只有一处）取唯一那处；多处且无默认标记 → None（无从锚定）。
///
/// 全平台共用——POSIX 和 Windows 版的 `anchored_command_from_paths` 都通过
/// `installs_anchored_command` 调它,取默认那处再 canonicalize 拿真身。
pub(super) fn default_install(installs: &[ToolInstallation]) -> Option<&ToolInstallation> {
    installs.iter().find(|i| i.is_path_default).or_else(|| {
        if installs.len() == 1 {
            installs.first()
        } else {
            None
        }
    })
}

/// 基于已枚举的安装列表生成锚定升级命令（复用 enumerate 结果，避免二次探测）。
/// 读取 enumerate 时已 canonicalize 写入的 `inst.real`,**不再二次 canonicalize**——
/// 既消除冗余 syscall,也闭合"enumerate 与 anchor 看到同一真身"的一致性边界
/// (两次 canonicalize 之间 symlink 被换会让锚定指向不同真身)。
///
/// 全平台共用——`anchored_command_from_paths` 自身是 cfg 二选一(POSIX 五分支 /
/// Windows 三分支),这里只负责取默认那处 + 转发。
pub(super) fn installs_anchored_command(
    tool: &str,
    installs: &[ToolInstallation],
) -> Option<String> {
    let inst = default_install(installs)?;
    let real = inst.real.to_string_lossy();
    // Codex 平台分发包损坏自愈：主包在但平台二进制缺失时 codex 跑不起来
    // （runnable=false），此时正常锚定的 `npm i -g @latest` 是 no-op 修不好——改用
    // uninstall+install 重装补回平台二进制。**但仅限会锚定到 sibling npm 的 node 管理器
    // 来源**（codex_repair_command 内按 source/real 收窄，brew/volta/bun/system 交给
    // codex_broken_package_command 做 source-specific 锚定，避免误用 npm 重装，也避免对
    // 已经跑不起来的 CLI 执行 `codex update`）。runnable=true 的正常升级走下方普通锚定
    // 路径，并优先使用 `codex update`。
    if tool == "codex" && !inst.runnable {
        if let Some(cmd) = codex_repair_command(&inst.path, &real) {
            return Some(cmd);
        }
        if let Some(cmd) = codex_broken_package_command(&inst.path, &real) {
            return Some(cmd);
        }
        // system / 未知来源两级 anchor 都返回 None：没有可靠的 sibling npm 可锚定，
        // 不能像 runnable=false 的兜底旧实现那样无来源约束地裸跑 `npm i -g`（违背
        // "必须绝对路径、不依赖 PATH"不变量，也和 system 安装的真实写回处脱节）。
        // 继续走下方 `anchored_command_from_paths`：它会锚到 codex CLI 自身跑官方
        // `codex update`——与该来源健康升级同一条路径（系统级 codex 的官方自升级）。
    }
    anchored_command_from_paths(tool, &inst.path, &real)
}

/// 静态命令（= 平台可安全静默执行的官方 CLI 自升级 || `npm i -g <pkg>@latest` /
/// 官方 installer）。锚定探不到默认安装时回退到它；npm fallback 仍等同于
/// "装到 PATH 第一个 npm"的旧行为。
pub(super) fn static_fallback_command_for(tool: &str, action: ToolLifecycleAction) -> String {
    tool_action_shell_command(tool, action).unwrap_or_default()
}

pub(super) fn static_fallback_command(tool: &str) -> String {
    static_fallback_command_for(tool, ToolLifecycleAction::Update)
}

/// 新装(install)的命令:对有官方 installer 的工具走「上游推荐 || npm 兜底」短路链,
/// 其余工具透传到 install 静态命令。update fallback 会在平台可安全静默执行时
/// 优先跑官方 CLI 自升级,但 install 端不能先跑 `tool update`,
/// 否则“未安装时安装”的路径会多一次无效失败。
///
/// 设计理由:
/// - install 没有锚点可言(从无到有),但**有"上游推荐方式"这一事实** ——
///   Anthropic 和 SST(OpenCode)都已将自家 native installer 列为首推、把 npm 列为传统方式。
///   把这层认知补进来,让 install 表与 update 端的锚定决策树共用同一份"上游事实"。
/// - Hermes 使用官方 installer,避免用系统 Python/pip 安装时踩 Python >=3.11 与 pyenv
///   `python` shim 问题;更新路径若能锚定已安装 CLI,则走 `<hermes> update`。
///   **Hermes 没有 npm 包,install 端不享受 `||` 降级**——上游 installer 不可达就只能等。
/// - 对**有 npm 包**的工具(claude/opencode),短路链(POSIX `||`)保证官方脚本不可达/
///   防火墙拦截时仍能装上,降级到裸 `npm i -g`。官方脚本本身不用 pipe,
///   所以这条路径在 WSL 的 `sh -c` 子 shell 中也不依赖外层 `pipefail`。
/// - Windows 原生不启用:claude.ai/install.sh、opencode.ai/install 都是 bash 脚本,
///   Windows 原生继续走 `tool_action_shell_command` 的 npm/PowerShell 命令;WSL 作为
///   Linux 环境复用这套 POSIX 安装优先级。
pub(super) fn installer_with_npm_fallback(installer: &str, tool: &str) -> String {
    match npm_install_command_for(tool) {
        Some(npm) => chain_update_commands(
            installer.to_string(),
            npm.to_string(),
            LifecycleCommandShell::Posix,
        ),
        None => installer.to_string(),
    }
}

pub(super) fn posix_install_command_for(tool: &str) -> String {
    match tool {
        "claude" => installer_with_npm_fallback(CLAUDE_INSTALL_UNIX, tool),
        "opencode" => installer_with_npm_fallback(OPENCODE_INSTALL_UNIX, tool),
        "hermes" => HERMES_INSTALL_UNIX.to_string(),
        "grok" => GROK_INSTALL_UNIX.to_string(),
        _ => static_fallback_command_for(tool, ToolLifecycleAction::Install),
    }
}

#[cfg(not(target_os = "windows"))]
pub(super) fn install_command_for(tool: &str) -> String {
    posix_install_command_for(tool)
}

/// 计算某工具的升级命令与"是否需确认"。全平台共用一份:
/// - **Windows + WSL 工具**(override 是 `\\wsl$\<distro>\...` UNC 路径)的升级规划
///   始终走 POSIX 静态命令、不锚定:锚定命令是 Windows 主机绝对路径,跨 `wsl.exe`
///   边界进入 distro 文件系统后完全无效;且 `enumerate_tool_installations` 不参与
///   WSL 文件系统、锚定无锚点。这一类显式短路到 `(unix_static, false, false)`,
///   前端不会弹确认。
///   **必须用 `wsl_tool_action_shell_command`(unix 版)而非 `static_fallback_command`**
///   ——后者读 `tool_action_shell_command`,Windows target 给 hermes 返回 PowerShell
///   installer,跨 wsl.exe 后不适用;`build_tool_action_line` 的 WSL 分支也用同一 wrapper,
///   保证 plan 展示给前端的命令与实际执行落 .bat 的命令一致。
/// - 其他平台与 Windows 原生工具走 `installs_anchored_command`:命中 → 锚定;
///   None(无默认 / sibling 不存在等)→ 静态兜底、`anchored=false`,
///   前端据此给"默认入口无法确定"诚实文案。
pub(super) fn plan_command_for(tool: &str, installs: &[ToolInstallation]) -> (String, bool, bool) {
    #[cfg(target_os = "windows")]
    {
        if wsl_distro_for_tool(tool).is_some() {
            let cmd = wsl_tool_action_shell_command(tool, ToolLifecycleAction::Update)
                .unwrap_or_default();
            return (cmd, false, false);
        }
    }
    match installs_anchored_command(tool, installs) {
        Some(command) => (command, installs.len() >= 2, true),
        None => (static_fallback_command(tool), installs.len() >= 2, false),
    }
}

/// 多处安装是否构成"真冲突"：≥2 处，且(版本分歧 或 有的能跑有的跑不起来)。
/// 同版本装两份且都能跑不算冲突（不打扰用户）。诊断展示据此判定。
pub(super) fn is_conflicting(installs: &[ToolInstallation]) -> bool {
    if installs.len() < 2 {
        return false;
    }
    let distinct_versions: std::collections::HashSet<&Option<String>> =
        installs.iter().map(|i| &i.version).collect();
    let runnable_mixed =
        installs.iter().any(|i| i.runnable) && installs.iter().any(|i| !i.runnable);
    distinct_versions.len() > 1 || runnable_mixed
}
