//! Skills 服务层
//!
//! 当前管理架构：
//! - 各 CLI 的原生 Skills 目录是事实来源
//! - 安装、卸载和恢复只作用于用户选中的 CLI
//! - skills.sh 全局 Skills 以 ~/.agents/skills 为规范源目录
//! - 原生扫描该目录的 CLI 直接可用，其他 CLI 按需通过符号链接启用
//! - 数据库仅补充仓库、哈希和备份元数据
//!
//! 文件后半仍保留旧版 SSOT 方法，供历史数据与兼容命令使用；新 UI 不再调用。

use anyhow::{anyhow, Context, Result};
use chrono::{DateTime, Utc};
use regex::Regex;
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::{Component, Path, PathBuf};
use std::process::Stdio;
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};
use tokio::process::Command;
use tokio::time::timeout;

use crate::app_config::{AppType, InstalledSkill, SkillApps, UnmanagedSkill};
use crate::config::get_app_config_dir;
use crate::database::Database;
use crate::error::format_skill_error;

// ========== 数据结构 ==========

/// Skill 同步方式
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum SyncMethod {
    /// 自动选择：优先 symlink，失败时回退到 copy
    #[default]
    Auto,
    /// 符号链接（推荐，节省磁盘空间）
    Symlink,
    /// 文件复制（兼容模式）
    Copy,
}

/// Skill 存储位置（SSOT 目录选择）
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum SkillStorageLocation {
    /// Agent Switch 管理目录 (~/.agentswitch/skills/)
    #[default]
    CcSwitch,
    /// Agent Skills 统一标准目录 (~/.agents/skills/)
    Unified,
}

/// 可发现的技能（来自仓库）
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DiscoverableSkill {
    /// 唯一标识: "owner/name:directory"
    pub key: String,
    /// 显示名称 (从 SKILL.md 解析)
    pub name: String,
    /// 技能描述
    pub description: String,
    /// 目录名称 (安装路径的最后一段)
    pub directory: String,
    /// GitHub README URL
    #[serde(rename = "readmeUrl")]
    pub readme_url: Option<String>,
    /// 仓库所有者
    #[serde(rename = "repoOwner")]
    pub repo_owner: String,
    /// 仓库名称
    #[serde(rename = "repoName")]
    pub repo_name: String,
    /// 分支名称
    #[serde(rename = "repoBranch")]
    pub repo_branch: String,
}

/// 技能对象（兼容旧 API，内部使用 DiscoverableSkill）
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Skill {
    /// 唯一标识: "owner/name:directory" 或 "local:directory"
    pub key: String,
    /// 显示名称 (从 SKILL.md 解析)
    pub name: String,
    /// 技能描述
    pub description: String,
    /// 目录名称 (安装路径的最后一段)
    pub directory: String,
    /// GitHub README URL
    #[serde(rename = "readmeUrl")]
    pub readme_url: Option<String>,
    /// 是否已安装
    pub installed: bool,
    /// 仓库所有者
    #[serde(rename = "repoOwner")]
    pub repo_owner: Option<String>,
    /// 仓库名称
    #[serde(rename = "repoName")]
    pub repo_name: Option<String>,
    /// 分支名称
    #[serde(rename = "repoBranch")]
    pub repo_branch: Option<String>,
}

/// 仓库配置
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SkillRepo {
    /// GitHub 用户/组织名
    pub owner: String,
    /// 仓库名称
    pub name: String,
    /// 分支 (默认 "main")
    pub branch: String,
    /// 是否启用
    pub enabled: bool,
}

/// 技能安装状态（旧版兼容）
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SkillState {
    /// 是否已安装
    pub installed: bool,
    /// 安装时间
    #[serde(rename = "installedAt")]
    pub installed_at: DateTime<Utc>,
}

/// 持久化存储结构（仓库配置）
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SkillStore {
    /// directory -> 安装状态（旧版兼容，新版不使用）
    pub skills: HashMap<String, SkillState>,
    /// 仓库列表
    pub repos: Vec<SkillRepo>,
}

impl Default for SkillStore {
    fn default() -> Self {
        SkillStore {
            skills: HashMap::new(),
            repos: vec![
                SkillRepo {
                    owner: "anthropics".to_string(),
                    name: "skills".to_string(),
                    branch: "main".to_string(),
                    enabled: true,
                },
                SkillRepo {
                    owner: "ComposioHQ".to_string(),
                    name: "awesome-claude-skills".to_string(),
                    branch: "master".to_string(),
                    enabled: true,
                },
                SkillRepo {
                    owner: "cexll".to_string(),
                    name: "myclaude".to_string(),
                    branch: "master".to_string(),
                    enabled: true,
                },
                SkillRepo {
                    owner: "JimLiu".to_string(),
                    name: "baoyu-skills".to_string(),
                    branch: "main".to_string(),
                    enabled: true,
                },
            ],
        }
    }
}

/// Skill 卸载结果
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SkillUninstallResult {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub backup_path: Option<String>,
}

/// 单个 CLI 原生 Skills 目录中的技能。
///
/// 文件系统是事实来源；仓库字段仅作为可选的辅助元数据。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AppSkill {
    pub id: String,
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    pub directory: String,
    pub path: String,
    pub is_symlink: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub link_target: Option<String>,
    pub managed_globally: bool,
    pub global_source: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub repo_owner: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub repo_name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub repo_branch: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub readme_url: Option<String>,
    pub installed_at: i64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub content_hash: Option<String>,
    pub updated_at: i64,
}

/// 指定 CLI 的原生 Skills 目录快照。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AppSkillsResponse {
    pub app: String,
    pub skills_dir: String,
    pub skills: Vec<AppSkill>,
}

/// skills.sh 全局 Skills 目录中的技能。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GlobalSkill {
    pub id: String,
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    pub directory: String,
    pub path: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub repo_owner: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub repo_name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub repo_branch: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub readme_url: Option<String>,
    pub apps: SkillApps,
    pub installed_at: i64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub content_hash: Option<String>,
    pub updated_at: i64,
}

/// 全局 Skills 库快照。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GlobalSkillsResponse {
    pub skills_dir: String,
    pub direct_apps: SkillApps,
    pub skills: Vec<GlobalSkill>,
}

/// Skill 更新检测结果
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SkillUpdateInfo {
    /// Skill ID
    pub id: String,
    /// Skill 名称
    pub name: String,
    /// 当前本地哈希
    pub current_hash: Option<String>,
    /// 远程最新哈希
    pub remote_hash: String,
}

/// Skill 存储位置迁移结果
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MigrationResult {
    pub migrated_count: usize,
    pub skipped_count: usize,
    pub errors: Vec<String>,
}

// ========== skills.sh API 类型 ==========

/// skills.sh API 原始响应
///
/// 注意：API 命名不一致（searchType 是 camelCase，duration_ms 是 snake_case），
/// 因此不能用 rename_all，需要逐字段指定。
#[derive(Debug, Clone, Deserialize)]
struct SkillsShApiResponse {
    pub query: String,
    #[serde(rename = "searchType")]
    #[allow(dead_code)]
    pub search_type: String,
    pub skills: Vec<SkillsShApiSkill>,
    #[allow(dead_code)]
    pub duration_ms: u64,
}

/// skills.sh API 原始技能条目
#[derive(Debug, Clone, Deserialize)]
struct SkillsShApiSkill {
    pub id: String,
    #[serde(rename = "skillId")]
    pub skill_id: String,
    pub name: String,
    pub installs: u64,
    pub source: String,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
struct SkillsShLeaderboardProps {
    pub initial_skills: Vec<SkillsShLeaderboardApiSkill>,
    pub total_skills: usize,
    pub all_time_total: u64,
    pub view: String,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
struct SkillsShLeaderboardApiSkill {
    pub source: String,
    pub skill_id: String,
    pub name: String,
    pub installs: u64,
    #[serde(default)]
    pub weekly_installs: Vec<u64>,
    #[serde(default)]
    pub is_official: bool,
}

/// skills.sh 搜索结果（返回给前端）
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SkillsShSearchResult {
    pub skills: Vec<SkillsShDiscoverableSkill>,
    pub result_count: usize,
    pub query: String,
}

/// skills.sh 榜单结果（返回给前端）
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SkillsShLeaderboardResult {
    pub skills: Vec<SkillsShDiscoverableSkill>,
    pub result_count: usize,
    pub total_skills: usize,
    pub all_time_total: u64,
    pub view: String,
}

/// skills.sh 可安装技能（返回给前端）
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SkillsShDiscoverableSkill {
    pub key: String,
    pub name: String,
    pub directory: String,
    pub repo_owner: String,
    pub repo_name: String,
    pub repo_branch: String,
    pub installs: u64,
    #[serde(default)]
    pub weekly_installs: Vec<u64>,
    #[serde(default)]
    pub is_official: bool,
    pub readme_url: Option<String>,
    pub detail_url: String,
}

/// skills.sh 详情页中的安全审计摘要。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SkillsShSecurityAudit {
    pub provider: String,
    pub status: String,
}

/// skills.sh 公开详情页（返回给前端）。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SkillsShSkillDetail {
    pub topic: Option<String>,
    pub summary_html: String,
    pub content_html: String,
    pub github_stars: Option<String>,
    pub first_seen: Option<String>,
    pub security_audits: Vec<SkillsShSecurityAudit>,
}

/// skills.sh 发布者页中的单个仓库摘要。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SkillsShSourceSummary {
    pub name: String,
    pub skill_summary: String,
    pub installs: String,
}

/// skills.sh 发布者详情页（返回给前端）。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SkillsShPublisherDetail {
    pub owner: String,
    pub source_count: usize,
    pub skill_count: usize,
    pub total_installs: String,
    pub sources: Vec<SkillsShSourceSummary>,
}

/// skills.sh 仓库页中的单个 Skill 摘要。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SkillsShRepositorySkill {
    pub skill_id: String,
    pub name: String,
    pub installs: u64,
    pub installs_label: String,
}

/// skills.sh 仓库详情页（返回给前端）。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SkillsShRepositoryDetail {
    pub owner: String,
    pub repository: String,
    pub skill_count: usize,
    pub total_installs: String,
    pub skills: Vec<SkillsShRepositorySkill>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SkillBackupEntry {
    pub backup_id: String,
    pub backup_path: String,
    pub created_at: i64,
    pub skill: InstalledSkill,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source_app: Option<AppType>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct SkillBackupMetadata {
    skill: InstalledSkill,
    backup_created_at: i64,
    source_path: String,
    #[serde(default)]
    source_app: Option<AppType>,
}

const SKILL_BACKUP_RETAIN_COUNT: usize = 20;
const SKILLS_CLI_TIMEOUT_SECONDS: u64 = 120;
const SKILLS_CLI_CANONICAL_AGENT: &str = "codex";

/// 技能元数据 (从 SKILL.md 解析)
#[derive(Debug, Clone, Deserialize)]
pub struct SkillMetadata {
    pub name: Option<String>,
    pub description: Option<String>,
}

/// 导入已有 Skill 时，前端显式提交的启用应用选择
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ImportSkillSelection {
    pub directory: String,
    #[serde(default)]
    pub apps: SkillApps,
}

#[derive(Debug, Clone, Deserialize)]
struct LegacySkillMigrationRow {
    directory: String,
    app_type: String,
}

// ========== ~/.agents/ lock 文件解析 ==========

/// `~/.agents/.skill-lock.json` 文件结构
#[derive(Deserialize)]
struct AgentsLockFile {
    skills: HashMap<String, AgentsLockSkill>,
}

/// lock 文件中单个 skill 的信息
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct AgentsLockSkill {
    source: Option<String>,
    source_type: Option<String>,
    source_url: Option<String>,
    skill_path: Option<String>,
    branch: Option<String>,
    source_branch: Option<String>,
}

#[derive(Debug, Clone)]
struct LockRepoInfo {
    owner: String,
    repo: String,
    skill_path: Option<String>,
    branch: Option<String>,
}

fn normalize_optional_branch(branch: Option<String>) -> Option<String> {
    branch.and_then(|b| {
        let trimmed = b.trim();
        if trimmed.is_empty() {
            None
        } else {
            Some(trimmed.to_string())
        }
    })
}

fn parse_branch_from_source_url(source_url: Option<&str>) -> Option<String> {
    let source_url = source_url?;
    let source_url = source_url.trim();
    if source_url.is_empty() {
        return None;
    }

    // 支持 https://github.com/owner/repo/tree/<branch>/...
    if let Some((_, after_tree)) = source_url.split_once("/tree/") {
        let branch = after_tree
            .split('/')
            .next()
            .map(str::trim)
            .filter(|s| !s.is_empty())?;
        return Some(branch.to_string());
    }

    // 支持 URL fragment: ...git#branch
    if let Some((_, fragment)) = source_url.split_once('#') {
        let branch = fragment
            .split('&')
            .next()
            .map(str::trim)
            .filter(|s| !s.is_empty())?;
        return Some(branch.to_string());
    }

    // 支持 query: ...?branch=xxx / ?ref=xxx
    if let Some((_, query)) = source_url.split_once('?') {
        for pair in query.split('&') {
            let Some((key, value)) = pair.split_once('=') else {
                continue;
            };
            if matches!(key, "branch" | "ref") {
                let branch = value.trim();
                if !branch.is_empty() {
                    return Some(branch.to_string());
                }
            }
        }
    }

    None
}

/// 获取 `~/.agents/skills/` 目录（存在时返回）
fn get_agents_skills_dir() -> Option<PathBuf> {
    dirs::home_dir()
        .map(|h| h.join(".agents").join("skills"))
        .filter(|p| p.exists())
}

fn parse_agents_lock_file(path: &Path) -> HashMap<String, LockRepoInfo> {
    let content = match fs::read_to_string(path) {
        Ok(c) => c,
        Err(e) => {
            if e.kind() == std::io::ErrorKind::NotFound {
                log::debug!("未找到 agents lock 文件: {}", path.display());
            } else {
                log::warn!("读取 agents lock 文件失败 ({}): {}", path.display(), e);
            }
            return HashMap::new();
        }
    };
    let lock: AgentsLockFile = match serde_json::from_str(&content) {
        Ok(l) => l,
        Err(e) => {
            log::warn!("解析 agents lock 文件失败 ({}): {}", path.display(), e);
            return HashMap::new();
        }
    };
    let parsed: HashMap<String, LockRepoInfo> = lock
        .skills
        .into_iter()
        .filter_map(|(name, skill)| {
            let source = skill.source?;
            if skill.source_type.as_deref() != Some("github") {
                return None;
            }
            let (owner, repo) = source.split_once('/')?;
            let branch = normalize_optional_branch(skill.branch)
                .or_else(|| normalize_optional_branch(skill.source_branch))
                .or_else(|| parse_branch_from_source_url(skill.source_url.as_deref()));
            Some((
                name,
                LockRepoInfo {
                    owner: owner.to_string(),
                    repo: repo.to_string(),
                    skill_path: skill.skill_path,
                    branch,
                },
            ))
        })
        .collect();
    log::info!(
        "agents lock 文件解析完成，共识别 {} 个 github skill",
        parsed.len()
    );
    parsed
}

/// 解析指定 HOME 下的 `.agents/.skill-lock.json`，返回 skill_name -> 仓库信息。
fn parse_agents_lock_at(home: &Path) -> HashMap<String, LockRepoInfo> {
    parse_agents_lock_file(&home.join(".agents").join(".skill-lock.json"))
}

/// skills CLI 的全局安装使用 `~/.agents/.skill-lock.json`，项目安装使用工作区根目录的
/// `skills-lock.json`。
fn parse_skills_cli_lock(workspace: &Path, global: bool) -> HashMap<String, LockRepoInfo> {
    if global {
        parse_agents_lock_at(workspace)
    } else {
        parse_agents_lock_file(&workspace.join("skills-lock.json"))
    }
}

/// 解析 `~/.agents/.skill-lock.json`，返回 skill_name -> 仓库信息。
fn parse_agents_lock() -> HashMap<String, LockRepoInfo> {
    let Some(home) = dirs::home_dir() else {
        log::warn!("无法获取 HOME 目录，跳过解析 agents lock 文件");
        return HashMap::new();
    };
    parse_agents_lock_at(&home)
}

// ========== SkillService ==========

pub struct SkillService;

impl Default for SkillService {
    fn default() -> Self {
        Self::new()
    }
}

impl SkillService {
    pub fn new() -> Self {
        Self
    }

    fn skills_cli_workspace_for_app(app: &AppType) -> PathBuf {
        get_app_config_dir().join("skills-cli").join(app.as_str())
    }

    fn skills_cli_workspace_for_unified() -> PathBuf {
        get_app_config_dir().join("skills-cli").join("unified")
    }

    fn skills_cli_canonical_skill_path(workspace: &Path, install_name: &str) -> PathBuf {
        workspace.join(".agents").join("skills").join(install_name)
    }

    fn validate_skills_cli_identifier(label: &str, value: &str) -> Result<()> {
        if value.is_empty()
            || !value
                .chars()
                .all(|character| character.is_ascii_alphanumeric() || "-_.".contains(character))
        {
            return Err(anyhow!("Invalid skills CLI {label}: {value}"));
        }
        Ok(())
    }

    fn skills_cli_add_args(
        skill: &DiscoverableSkill,
        install_name: &str,
        global: bool,
    ) -> Result<Vec<String>> {
        Self::validate_skills_cli_identifier("owner", &skill.repo_owner)?;
        Self::validate_skills_cli_identifier("repository", &skill.repo_name)?;
        Self::validate_skills_cli_identifier("skill", install_name)?;

        let mut args = vec![
            "add".to_string(),
            format!("{}/{}", skill.repo_owner, skill.repo_name),
            "--skill".to_string(),
            install_name.to_string(),
        ];
        if global {
            args.push("--global".to_string());
        }
        args.extend([
            "--agent".to_string(),
            SKILLS_CLI_CANONICAL_AGENT.to_string(),
            "--yes".to_string(),
        ]);
        Ok(args)
    }

    fn skills_cli_update_args(install_name: &str, global: bool) -> Result<Vec<String>> {
        Self::validate_skills_cli_identifier("skill", install_name)?;
        let mut args = vec!["update".to_string(), install_name.to_string()];
        if global {
            args.push("--global".to_string());
        }
        args.push("--yes".to_string());
        Ok(args)
    }

    fn resolve_skills_npx_executable() -> Result<PathBuf> {
        if let Some(override_path) = std::env::var_os("AGENTSWITCH_SKILLS_NPX") {
            let override_path = PathBuf::from(override_path);
            if override_path.is_file() {
                return Ok(override_path);
            }
        }

        for directory in crate::commands::build_tool_search_paths("npx") {
            for candidate in crate::commands::tool_executable_candidates("npx", &directory) {
                if candidate.is_file() {
                    return Ok(candidate);
                }
            }
        }

        Err(anyhow!("npx was not found in the user CLI search paths"))
    }

    fn skills_cli_path(executable: &Path) -> Result<std::ffi::OsString> {
        let mut paths = Vec::new();
        if let Some(parent) = executable.parent() {
            paths.push(parent.to_path_buf());
        }
        if let Some(current) = std::env::var_os("PATH") {
            paths.extend(std::env::split_paths(&current));
        }
        std::env::join_paths(paths).context("Failed to prepare PATH for skills CLI")
    }

    async fn run_skills_cli(workspace: &Path, args: &[String]) -> Result<String> {
        fs::create_dir_all(workspace).with_context(|| {
            format!(
                "Failed to create skills CLI workspace: {}",
                workspace.display()
            )
        })?;
        let executable = Self::resolve_skills_npx_executable()?;
        let command_path = Self::skills_cli_path(&executable)?;
        let user_home = dirs::home_dir();

        let mut command = Command::new(&executable);
        command
            .arg("-y")
            .arg("skills")
            .args(args)
            .current_dir(workspace)
            .env("PATH", command_path)
            .env("NO_COLOR", "1")
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .kill_on_drop(true);

        if std::env::var_os("NPM_CONFIG_USERCONFIG").is_none() {
            if let Some(npmrc) = user_home.as_ref().map(|home| home.join(".npmrc")) {
                if npmrc.is_file() {
                    command.env("NPM_CONFIG_USERCONFIG", npmrc);
                }
            }
        }
        if std::env::var_os("NPM_CONFIG_CACHE").is_none() {
            if let Some(cache) = user_home.as_ref().map(|home| home.join(".npm")) {
                if cache.is_dir() {
                    command.env("NPM_CONFIG_CACHE", cache);
                }
            }
        }

        let output = timeout(
            std::time::Duration::from_secs(SKILLS_CLI_TIMEOUT_SECONDS),
            command.output(),
        )
        .await
        .map_err(|_| {
            anyhow!("skills CLI timed out after {SKILLS_CLI_TIMEOUT_SECONDS} seconds")
        })??;
        let stdout = String::from_utf8_lossy(&output.stdout).trim().to_string();
        let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();

        if !output.status.success() {
            let detail = if stderr.is_empty() { &stdout } else { &stderr };
            return Err(anyhow!(
                "skills CLI exited with {}: {}",
                output
                    .status
                    .code()
                    .map(|code| code.to_string())
                    .unwrap_or_else(|| "signal".to_string()),
                detail
                    .lines()
                    .rev()
                    .take(8)
                    .collect::<Vec<_>>()
                    .into_iter()
                    .rev()
                    .collect::<Vec<_>>()
                    .join("\n")
            ));
        }

        log::info!("skills CLI succeeded: npx -y skills {}", args.join(" "));
        Ok(if stdout.is_empty() { stderr } else { stdout })
    }

    async fn try_install_remote_with_skills_cli(
        &self,
        skill: &DiscoverableSkill,
        command_home: &Path,
        dest: &Path,
        install_name: &str,
        global: bool,
    ) -> Result<String> {
        let args = Self::skills_cli_add_args(skill, install_name, global)?;
        Self::run_skills_cli(command_home, &args).await?;

        let source = Self::skills_cli_canonical_skill_path(command_home, install_name);
        Self::validate_sync_source_dir(&source, install_name)?;
        if !Self::paths_are_same(&source, dest) {
            Self::copy_skill_to_new_dest(&source, dest, install_name)?;
        }

        Ok(parse_skills_cli_lock(command_home, global)
            .get(install_name)
            .and_then(|entry| entry.branch.clone())
            .unwrap_or_else(|| skill.repo_branch.clone()))
    }

    async fn install_remote_from_archive(
        &self,
        skill: &DiscoverableSkill,
        dest: &Path,
        install_name: &str,
    ) -> Result<String> {
        let source_rel = Self::sanitize_skill_source_path(&skill.directory).ok_or_else(|| {
            anyhow!(format_skill_error(
                "INVALID_SKILL_DIRECTORY",
                &[("directory", &skill.directory)],
                Some("checkZipContent"),
            ))
        })?;
        let repo = SkillRepo {
            owner: skill.repo_owner.clone(),
            name: skill.repo_name.clone(),
            branch: skill.repo_branch.clone(),
            enabled: true,
        };
        let (temp_dir, used_branch) = timeout(
            std::time::Duration::from_secs(60),
            self.download_repo(&repo),
        )
        .await
        .map_err(|_| {
            anyhow!(format_skill_error(
                "DOWNLOAD_TIMEOUT",
                &[
                    ("owner", &repo.owner),
                    ("name", &repo.name),
                    ("timeout", "60"),
                ],
                Some("checkNetwork"),
            ))
        })??;

        let result = (|| -> Result<()> {
            let source =
                Self::resolve_skill_source_dir(&temp_dir, &skill.directory).ok_or_else(|| {
                    anyhow!(format_skill_error(
                        "SKILL_DIR_NOT_FOUND",
                        &[("path", &temp_dir.join(&source_rel).display().to_string())],
                        Some("checkRepoUrl"),
                    ))
                })?;
            let canonical_temp = temp_dir.canonicalize().unwrap_or_else(|_| temp_dir.clone());
            let canonical_source = source.canonicalize().map_err(|_| {
                anyhow!(format_skill_error(
                    "SKILL_DIR_NOT_FOUND",
                    &[("path", &source.display().to_string())],
                    Some("checkRepoUrl"),
                ))
            })?;
            if !canonical_source.starts_with(&canonical_temp) || !canonical_source.is_dir() {
                return Err(anyhow!(format_skill_error(
                    "INVALID_SKILL_DIRECTORY",
                    &[("directory", &skill.directory)],
                    Some("checkZipContent"),
                )));
            }
            Self::copy_skill_to_new_dest(&canonical_source, dest, install_name)
        })();
        let _ = fs::remove_dir_all(&temp_dir);
        result?;
        Ok(used_branch)
    }

    async fn install_remote_command_first(
        &self,
        skill: &DiscoverableSkill,
        command_home: &Path,
        dest: &Path,
        install_name: &str,
        global: bool,
    ) -> Result<String> {
        match self
            .try_install_remote_with_skills_cli(skill, command_home, dest, install_name, global)
            .await
        {
            Ok(branch) => Ok(branch),
            Err(command_error) => {
                log::warn!(
                    "skills CLI install failed for {}/{}:{}; falling back to archive: {command_error}",
                    skill.repo_owner,
                    skill.repo_name,
                    install_name,
                );
                if dest.exists() || Self::is_symlink(dest) {
                    Self::remove_path(dest)?;
                }
                self.install_remote_from_archive(skill, dest, install_name)
                    .await
                    .with_context(|| format!("skills CLI install failed first: {command_error}"))
            }
        }
    }

    async fn try_update_remote_with_skills_cli(
        command_home: &Path,
        dest: &Path,
        install_name: &str,
        global: bool,
    ) -> Result<Option<String>> {
        let before_hash = Self::compute_dir_hash(dest).ok();
        let args = Self::skills_cli_update_args(install_name, global)?;
        Self::run_skills_cli(command_home, &args).await?;

        let source = Self::skills_cli_canonical_skill_path(command_home, install_name);
        Self::validate_sync_source_dir(&source, install_name)?;
        let source_hash = Self::compute_dir_hash(&source)?;

        if !Self::paths_are_same(&source, dest)
            && before_hash.as_deref() != Some(source_hash.as_str())
        {
            Self::replace_dest_with_copy(&source, dest, install_name)?;
        }

        Ok(parse_skills_cli_lock(command_home, global)
            .get(install_name)
            .and_then(|entry| entry.branch.clone()))
    }

    /// 构建 Skill 文档 URL（指向仓库中的 SKILL.md 文件）
    fn build_skill_doc_url(owner: &str, repo: &str, branch: &str, doc_path: &str) -> String {
        format!("https://github.com/{owner}/{repo}/blob/{branch}/{doc_path}")
    }

    /// 从旧 readme_url 中提取仓库内文档路径，兼容 `blob`/`tree` 两种格式
    fn extract_doc_path_from_url(url: &str) -> Option<String> {
        let marker = if url.contains("/blob/") {
            "/blob/"
        } else if url.contains("/tree/") {
            "/tree/"
        } else {
            return None;
        };

        let (_, tail) = url.split_once(marker)?;
        let (_, path) = tail.split_once('/')?;
        if path.is_empty() {
            return None;
        }
        Some(path.to_string())
    }

    // ========== 路径管理 ==========

    fn global_skills_path() -> Result<PathBuf> {
        let home = dirs::home_dir().context(format_skill_error(
            "GET_HOME_DIR_FAILED",
            &[],
            Some("checkPermission"),
        ))?;
        Ok(home.join(".agents").join("skills"))
    }

    /// 获取 skills.sh 规范全局 Skills 目录（~/.agents/skills/）。
    pub fn get_global_skills_dir() -> Result<PathBuf> {
        let dir = Self::global_skills_path()?;
        fs::create_dir_all(&dir)?;
        Ok(dir)
    }

    /// 获取 SSOT 目录（根据设置返回 ~/.agentswitch/skills/ 或 ~/.agents/skills/）
    pub fn get_ssot_dir() -> Result<PathBuf> {
        let location = crate::settings::get_skill_storage_location();
        let dir = match location {
            SkillStorageLocation::CcSwitch => get_app_config_dir().join("skills"),
            SkillStorageLocation::Unified => {
                let home = dirs::home_dir().context(format_skill_error(
                    "GET_HOME_DIR_FAILED",
                    &[],
                    Some("checkPermission"),
                ))?;
                home.join(".agents").join("skills")
            }
        };
        fs::create_dir_all(&dir)?;
        Ok(dir)
    }

    /// 获取 Skill 卸载备份目录（~/.agentswitch/skill-backups/）
    fn get_backup_dir() -> Result<PathBuf> {
        let dir = get_app_config_dir().join("skill-backups");
        fs::create_dir_all(&dir)?;
        Ok(dir)
    }

    /// 获取应用的 skills 目录
    pub fn get_app_skills_dir(app: &AppType) -> Result<PathBuf> {
        // 目录覆盖：优先使用用户在 settings.json 中配置的 override 目录
        match app {
            AppType::Claude => {
                if let Some(custom) = crate::settings::get_claude_override_dir() {
                    return Ok(custom.join("skills"));
                }
            }
            AppType::ClaudeDesktop => {}
            AppType::Codex => {
                if let Some(custom) = crate::settings::get_codex_override_dir() {
                    if custom.file_name().and_then(|name| name.to_str()) == Some(".codex") {
                        if let Some(home) = custom.parent() {
                            return Ok(home.join(".agents").join("skills"));
                        }
                    }
                    return Ok(custom.join("skills"));
                }
            }
            AppType::Gemini => {
                if let Some(custom) = crate::settings::get_gemini_override_dir() {
                    return Ok(custom.join("skills"));
                }
            }
            AppType::OpenCode => {
                if let Some(custom) = crate::settings::get_opencode_override_dir() {
                    return Ok(custom.join("skills"));
                }
            }
            AppType::OpenClaw => {
                if let Some(custom) = crate::settings::get_openclaw_override_dir() {
                    return Ok(custom.join("skills"));
                }
            }
            AppType::Hermes => {
                if let Some(custom) = crate::settings::get_hermes_override_dir() {
                    return Ok(custom.join("skills"));
                }
            }
        }

        // 默认路径：回退到用户主目录下的标准位置
        let home = dirs::home_dir().context(format_skill_error(
            "GET_HOME_DIR_FAILED",
            &[],
            Some("checkPermission"),
        ))?;

        Ok(match app {
            AppType::Claude => home.join(".claude").join("skills"),
            AppType::ClaudeDesktop => home.join(".claude-desktop").join("skills"),
            AppType::Codex => home.join(".agents").join("skills"),
            AppType::Gemini => home.join(".gemini").join("skills"),
            AppType::OpenCode => home.join(".config").join("opencode").join("skills"),
            AppType::OpenClaw => home.join(".openclaw").join("skills"),
            AppType::Hermes => crate::hermes_config::get_hermes_dir().join("skills"),
        })
    }

    // ========== 统一管理方法 ==========

    /// 获取所有已安装的 Skills
    pub fn get_all_installed(db: &Arc<Database>) -> Result<Vec<InstalledSkill>> {
        let skills = db.get_all_installed_skills()?;
        Ok(skills.into_values().collect())
    }

    fn ensure_app_skill_support(app: &AppType) -> Result<()> {
        match app {
            AppType::Claude
            | AppType::Codex
            | AppType::Gemini
            | AppType::OpenCode
            | AppType::Hermes => Ok(()),
            AppType::ClaudeDesktop | AppType::OpenClaw => Err(anyhow!(
                "{} does not support CLI Skills management",
                app.as_str()
            )),
        }
    }

    fn app_scoped_skill_id(app: &AppType, id: &str) -> String {
        let prefix = format!("{}:", app.as_str());
        if id.starts_with(&prefix) {
            id.to_string()
        } else {
            format!("{prefix}{id}")
        }
    }

    fn global_skill_id(id: &str) -> String {
        if id.starts_with("global:") {
            id.to_string()
        } else {
            format!("global:{id}")
        }
    }

    fn is_app_scoped_skill_id(id: &str) -> bool {
        ["claude:", "codex:", "gemini:", "opencode:", "hermes:"]
            .iter()
            .any(|prefix| id.starts_with(prefix))
    }

    fn find_global_record<'a, I>(records: I, directory: &str) -> Option<&'a InstalledSkill>
    where
        I: Iterator<Item = &'a InstalledSkill>,
    {
        records
            .filter(|skill| {
                skill.directory.eq_ignore_ascii_case(directory)
                    && (skill.id.starts_with("global:") || !Self::is_app_scoped_skill_id(&skill.id))
            })
            .max_by_key(|skill| skill.id.starts_with("global:"))
    }

    fn app_skill_from_path(
        app: &AppType,
        path: &Path,
        directory: &str,
        metadata: Option<&InstalledSkill>,
    ) -> AppSkill {
        let skill_md = path.join("SKILL.md");
        let (disk_name, disk_description) = Self::read_skill_name_desc(&skill_md, directory);
        let is_symlink = Self::is_symlink(path);
        let link_target =
            Self::resolved_symlink_target(path).map(|target| target.to_string_lossy().to_string());
        let global_source_path = Self::global_skills_path()
            .ok()
            .map(|root| root.join(directory));
        let global_source = global_source_path
            .as_ref()
            .is_some_and(|source| source == path);
        let managed_globally = global_source_path
            .and_then(|source| source.canonicalize().ok())
            .zip(path.canonicalize().ok())
            .is_some_and(|(source, resolved)| source == resolved);
        let installed_at = metadata.map(|skill| skill.installed_at).unwrap_or_else(|| {
            fs::metadata(path)
                .and_then(|value| value.modified())
                .ok()
                .and_then(|value| value.duration_since(UNIX_EPOCH).ok())
                .map(|value| value.as_secs() as i64)
                .unwrap_or_default()
        });

        AppSkill {
            id: metadata
                .map(|skill| skill.id.clone())
                .unwrap_or_else(|| format!("{}:local:{directory}", app.as_str())),
            name: if disk_name.trim().is_empty() {
                metadata
                    .map(|skill| skill.name.clone())
                    .unwrap_or_else(|| directory.to_string())
            } else {
                disk_name
            },
            description: disk_description
                .or_else(|| metadata.and_then(|skill| skill.description.clone())),
            directory: directory.to_string(),
            path: path.to_string_lossy().to_string(),
            is_symlink,
            link_target,
            managed_globally,
            global_source,
            repo_owner: metadata.and_then(|skill| skill.repo_owner.clone()),
            repo_name: metadata.and_then(|skill| skill.repo_name.clone()),
            repo_branch: metadata.and_then(|skill| skill.repo_branch.clone()),
            readme_url: metadata.and_then(|skill| skill.readme_url.clone()),
            installed_at,
            content_hash: metadata.and_then(|skill| skill.content_hash.clone()),
            updated_at: metadata.map(|skill| skill.updated_at).unwrap_or_default(),
        }
    }

    fn collect_app_skill_dirs(
        root: &Path,
        current: &Path,
        depth: usize,
        max_depth: usize,
        results: &mut Vec<(String, PathBuf)>,
    ) -> Result<()> {
        if depth > max_depth {
            return Ok(());
        }

        for entry in fs::read_dir(current)
            .with_context(|| format!("读取 Skills 目录失败: {}", current.display()))?
        {
            let entry = entry?;
            let name = entry.file_name().to_string_lossy().to_string();
            if name.starts_with('.') {
                continue;
            }

            let path = entry.path();
            if !path.is_dir() {
                continue;
            }
            if path.join("SKILL.md").is_file() {
                let relative = path
                    .strip_prefix(root)
                    .unwrap_or(&path)
                    .to_string_lossy()
                    .replace('\\', "/");
                results.push((relative, path));
                continue;
            }
            Self::collect_app_skill_dirs(root, &path, depth + 1, max_depth, results)?;
        }
        Ok(())
    }

    /// 直接读取指定 CLI 的原生 Skills 目录。
    ///
    /// 数据库只用于补充仓库信息，不决定技能是否存在。
    pub fn get_for_app(db: &Arc<Database>, app: &AppType) -> Result<AppSkillsResponse> {
        Self::ensure_app_skill_support(app)?;

        let skills_dir = Self::get_app_skills_dir(app)?;
        let metadata = db.get_all_installed_skills()?;
        let scoped_prefix = format!("{}:", app.as_str());
        let mut skills = Vec::new();

        if skills_dir.exists() {
            let max_depth = match app {
                AppType::OpenCode | AppType::Hermes => 8,
                AppType::Claude | AppType::Codex | AppType::Gemini => 1,
                AppType::ClaudeDesktop | AppType::OpenClaw => 0,
            };
            let mut native_skills = Vec::new();
            Self::collect_app_skill_dirs(
                &skills_dir,
                &skills_dir,
                1,
                max_depth,
                &mut native_skills,
            )?;

            for (directory, path) in native_skills {
                let entry_metadata = metadata
                    .values()
                    .filter(|skill| {
                        skill.apps.is_enabled_for(app)
                            && skill.directory.eq_ignore_ascii_case(&directory)
                    })
                    .max_by_key(|skill| skill.id.starts_with(&scoped_prefix));

                skills.push(Self::app_skill_from_path(
                    app,
                    &path,
                    &directory,
                    entry_metadata,
                ));
            }
        }

        skills.sort_by(|left, right| {
            left.name
                .to_lowercase()
                .cmp(&right.name.to_lowercase())
                .then_with(|| left.directory.cmp(&right.directory))
        });

        Ok(AppSkillsResponse {
            app: app.as_str().to_string(),
            skills_dir: skills_dir.to_string_lossy().to_string(),
            skills,
        })
    }

    fn paths_are_same(left: &Path, right: &Path) -> bool {
        if left == right {
            return true;
        }
        match (left.canonicalize(), right.canonicalize()) {
            (Ok(left), Ok(right)) => left == right,
            _ => false,
        }
    }

    /// 该 CLI 的主安装目录是否就是全局 Skills 目录。
    ///
    /// 这决定安装、卸载和备份应落在哪个目录，不能与“是否额外扫描全局目录”混用。
    fn app_primary_skills_dir_is_global(app: &AppType) -> bool {
        Self::get_app_skills_dir(app)
            .ok()
            .zip(Self::global_skills_path().ok())
            .is_some_and(|(app_dir, global_dir)| Self::paths_are_same(&app_dir, &global_dir))
    }

    fn expand_external_skill_dir(raw: &str) -> Option<PathBuf> {
        let home = dirs::home_dir()?;
        let expanded_home = if raw == "~" {
            home.to_string_lossy().to_string()
        } else if let Some(relative) = raw.strip_prefix("~/") {
            home.join(relative).to_string_lossy().to_string()
        } else {
            raw.to_string()
        };
        let variable_pattern = Regex::new(r"\$\{([A-Za-z_][A-Za-z0-9_]*)\}").ok()?;
        let expanded = variable_pattern
            .replace_all(&expanded_home, |captures: &regex::Captures<'_>| {
                std::env::var(&captures[1]).unwrap_or_else(|_| captures[0].to_string())
            })
            .into_owned();
        let path = PathBuf::from(expanded);
        path.is_absolute().then_some(path)
    }

    fn hermes_reads_global_skills_dir(global_dir: &Path) -> bool {
        let Ok(config) = crate::hermes_config::read_hermes_config() else {
            return false;
        };
        config
            .get("skills")
            .and_then(|skills| skills.get("external_dirs"))
            .and_then(serde_yaml::Value::as_sequence)
            .into_iter()
            .flatten()
            .filter_map(serde_yaml::Value::as_str)
            .filter_map(Self::expand_external_skill_dir)
            .any(|directory| Self::paths_are_same(&directory, global_dir))
    }

    /// 该 CLI 是否会把 `~/.agents/skills` 作为任一原生发现目录。
    ///
    /// Codex、Gemini CLI 和 OpenCode 原生支持 Agent Skills 兼容目录；Hermes 仅在
    /// `skills.external_dirs` 显式配置后扫描它。Claude Code 仍通过 `~/.claude/skills`
    /// 中的链接使用全局 Skill。
    fn app_reads_global_skills_dir(app: &AppType) -> bool {
        if Self::app_primary_skills_dir_is_global(app) {
            return true;
        }
        match app {
            AppType::Codex | AppType::Gemini | AppType::OpenCode => true,
            AppType::Hermes => Self::global_skills_path()
                .ok()
                .is_some_and(|global_dir| Self::hermes_reads_global_skills_dir(&global_dir)),
            AppType::Claude | AppType::ClaudeDesktop | AppType::OpenClaw => false,
        }
    }

    fn global_direct_apps() -> SkillApps {
        let mut apps = SkillApps::default();
        for app in [
            AppType::Claude,
            AppType::Codex,
            AppType::Gemini,
            AppType::OpenCode,
            AppType::Hermes,
        ] {
            apps.set_enabled_for(&app, Self::app_reads_global_skills_dir(&app));
        }
        apps
    }

    fn global_link_states(source: &Path, directory: &str) -> SkillApps {
        let mut apps = SkillApps::default();
        for app in [
            AppType::Claude,
            AppType::Codex,
            AppType::Gemini,
            AppType::OpenCode,
            AppType::Hermes,
        ] {
            let available = if Self::app_reads_global_skills_dir(&app) {
                source.join("SKILL.md").is_file()
            } else {
                Self::get_app_skills_dir(&app)
                    .map(|dir| dir.join(directory))
                    .is_ok_and(|dest| Self::symlink_points_to(&dest, source))
            };
            apps.set_enabled_for(&app, available);
        }
        apps
    }

    fn global_skill_from_path(
        path: &Path,
        directory: &str,
        metadata: Option<&InstalledSkill>,
    ) -> GlobalSkill {
        let (disk_name, disk_description) =
            Self::read_skill_name_desc(&path.join("SKILL.md"), directory);
        let installed_at = metadata.map(|skill| skill.installed_at).unwrap_or_else(|| {
            fs::metadata(path)
                .and_then(|value| value.modified())
                .ok()
                .and_then(|value| value.duration_since(UNIX_EPOCH).ok())
                .map(|value| value.as_secs() as i64)
                .unwrap_or_default()
        });

        GlobalSkill {
            id: metadata
                .map(|skill| skill.id.clone())
                .unwrap_or_else(|| format!("global:local:{directory}")),
            name: if disk_name.trim().is_empty() {
                metadata
                    .map(|skill| skill.name.clone())
                    .unwrap_or_else(|| directory.to_string())
            } else {
                disk_name
            },
            description: disk_description
                .or_else(|| metadata.and_then(|skill| skill.description.clone())),
            directory: directory.to_string(),
            path: path.to_string_lossy().to_string(),
            repo_owner: metadata.and_then(|skill| skill.repo_owner.clone()),
            repo_name: metadata.and_then(|skill| skill.repo_name.clone()),
            repo_branch: metadata.and_then(|skill| skill.repo_branch.clone()),
            readme_url: metadata.and_then(|skill| skill.readme_url.clone()),
            apps: Self::global_link_states(path, directory),
            installed_at,
            content_hash: metadata.and_then(|skill| skill.content_hash.clone()),
            updated_at: metadata.map(|skill| skill.updated_at).unwrap_or_default(),
        }
    }

    /// 读取 skills.sh 全局 Skills 目录及其对各 CLI 的实际可用状态。
    pub fn get_global(db: &Arc<Database>) -> Result<GlobalSkillsResponse> {
        let skills_dir = Self::get_global_skills_dir()?;
        let records = db.get_all_installed_skills()?;
        let mut native_skills = Vec::new();
        let mut skills = Vec::new();

        Self::collect_app_skill_dirs(&skills_dir, &skills_dir, 1, 1, &mut native_skills)?;
        for (directory, path) in native_skills {
            let metadata = Self::find_global_record(records.values(), &directory);
            skills.push(Self::global_skill_from_path(&path, &directory, metadata));
        }

        skills.sort_by(|left, right| {
            left.name
                .to_lowercase()
                .cmp(&right.name.to_lowercase())
                .then_with(|| left.directory.cmp(&right.directory))
        });

        Ok(GlobalSkillsResponse {
            skills_dir: skills_dir.to_string_lossy().to_string(),
            direct_apps: Self::global_direct_apps(),
            skills,
        })
    }

    /// 将全局 Skill 启用到指定 CLI。原生读取者无需链接，其他 CLI 创建或移除链接。
    ///
    /// 同名本地目录和指向其他位置的链接都不会被覆盖。
    pub fn set_global_link(
        db: &Arc<Database>,
        directory: &str,
        app: &AppType,
        enabled: bool,
    ) -> Result<GlobalSkill> {
        Self::ensure_app_skill_support(app)?;
        let relative = Self::sanitize_skill_source_path(directory)
            .ok_or_else(|| anyhow!("Invalid skill directory: {directory}"))?;
        let directory = relative.to_string_lossy().replace('\\', "/");
        let global_dir = Self::get_global_skills_dir()?;
        let source = global_dir.join(&relative);
        Self::validate_sync_source_dir(&source, &directory)?;

        let app_dir = Self::get_app_skills_dir(app)?;
        let dest = app_dir.join(&relative);
        if Self::app_reads_global_skills_dir(app) {
            if !enabled {
                return Err(anyhow!(
                    "{} reads the global Skills directory directly; uninstall the Skill from global management instead",
                    app.as_str()
                ));
            }
        } else if enabled {
            if Self::symlink_points_to(&dest, &source) {
                // 已经是正确的全局链接，无需重建。
            } else if dest.exists() || Self::is_symlink(&dest) {
                return Err(anyhow!(
                    "Cannot link global Skill '{}': {} already exists in {}",
                    directory,
                    dest.display(),
                    app.as_str()
                ));
            } else {
                let parent = dest
                    .parent()
                    .ok_or_else(|| anyhow!("Invalid skill destination: {}", dest.display()))?;
                fs::create_dir_all(parent)?;
                Self::create_symlink(&source, &dest)?;
            }
        } else if Self::symlink_points_to(&dest, &source) {
            Self::remove_path(&dest)?;
        }

        let records = db.get_all_installed_skills()?;
        let existing = Self::find_global_record(records.values(), &directory).cloned();
        let (disk_name, disk_description) =
            Self::read_skill_name_desc(&source.join("SKILL.md"), &directory);
        let mut record = existing.clone().unwrap_or_else(|| InstalledSkill {
            id: format!("global:local:{directory}"),
            name: disk_name,
            description: disk_description,
            directory: directory.clone(),
            repo_owner: None,
            repo_name: None,
            repo_branch: None,
            readme_url: None,
            apps: SkillApps::default(),
            installed_at: Utc::now().timestamp(),
            content_hash: Self::compute_dir_hash(&source).ok(),
            updated_at: 0,
        });
        let old_id = record.id.clone();
        record.id = Self::global_skill_id(&record.id);
        record.apps = Self::global_link_states(&source, &directory);
        db.save_skill(&record)?;
        if old_id != record.id {
            let _ = db.delete_skill(&old_id);
        }

        Ok(Self::global_skill_from_path(
            &source,
            &directory,
            Some(&record),
        ))
    }

    /// 将仓库中的 Skill 安装到全局目录。
    ///
    /// 直接读取 ~/.agents/skills 的 CLI 会立即可见；不会额外创建其他 CLI 链接。
    pub async fn install_global(
        &self,
        db: &Arc<Database>,
        skill: &DiscoverableSkill,
    ) -> Result<GlobalSkill> {
        let source_rel = Self::sanitize_skill_source_path(&skill.directory).ok_or_else(|| {
            anyhow!(format_skill_error(
                "INVALID_SKILL_DIRECTORY",
                &[("directory", &skill.directory)],
                Some("checkZipContent"),
            ))
        })?;
        let install_name = source_rel
            .file_name()
            .and_then(|name| Self::sanitize_install_name(&name.to_string_lossy()))
            .ok_or_else(|| {
                anyhow!(format_skill_error(
                    "INVALID_SKILL_DIRECTORY",
                    &[("directory", &skill.directory)],
                    Some("checkZipContent"),
                ))
            })?;

        let global_dir = Self::get_global_skills_dir()?;
        let dest = global_dir.join(&install_name);
        if dest.exists() || Self::is_symlink(&dest) {
            return Err(anyhow!(format_skill_error(
                "SKILL_DIRECTORY_CONFLICT",
                &[
                    ("directory", &install_name),
                    ("existing_repo", "global library"),
                    (
                        "new_repo",
                        &format!("{}/{}", skill.repo_owner, skill.repo_name)
                    ),
                ],
                Some("uninstallFirst"),
            )));
        }

        let command_home = dirs::home_dir().context(format_skill_error(
            "GET_HOME_DIR_FAILED",
            &[],
            Some("checkPermission"),
        ))?;
        let repo_branch = self
            .install_remote_command_first(skill, &command_home, &dest, &install_name, true)
            .await?;

        let doc_path = skill
            .readme_url
            .as_deref()
            .and_then(Self::extract_doc_path_from_url)
            .map(|path| {
                if path.ends_with("/SKILL.md") || path == "SKILL.md" {
                    path
                } else {
                    format!("{}/SKILL.md", path.trim_end_matches('/'))
                }
            })
            .unwrap_or_else(|| format!("{}/SKILL.md", skill.directory.trim_end_matches('/')));
        let (disk_name, disk_description) =
            Self::read_skill_name_desc(&dest.join("SKILL.md"), &install_name);
        let record = InstalledSkill {
            id: Self::global_skill_id(&skill.key),
            name: if disk_name.trim().is_empty() {
                skill.name.clone()
            } else {
                disk_name
            },
            description: disk_description
                .or_else(|| (!skill.description.is_empty()).then(|| skill.description.clone())),
            directory: install_name.clone(),
            repo_owner: Some(skill.repo_owner.clone()),
            repo_name: Some(skill.repo_name.clone()),
            repo_branch: Some(repo_branch.clone()),
            readme_url: Some(Self::build_skill_doc_url(
                &skill.repo_owner,
                &skill.repo_name,
                &repo_branch,
                &doc_path,
            )),
            apps: Self::global_link_states(&dest, &install_name),
            installed_at: Utc::now().timestamp(),
            content_hash: Self::compute_dir_hash(&dest).ok(),
            updated_at: 0,
        };
        if let Err(error) = db.save_skill(&record) {
            let _ = Self::remove_path(&dest);
            return Err(error.into());
        }
        Ok(Self::global_skill_from_path(
            &dest,
            &install_name,
            Some(&record),
        ))
    }

    /// 将仓库中的 Skill 直接安装到指定 CLI 的原生目录。
    pub async fn install_for_app(
        &self,
        db: &Arc<Database>,
        skill: &DiscoverableSkill,
        app: &AppType,
    ) -> Result<AppSkill> {
        Self::ensure_app_skill_support(app)?;
        if Self::app_primary_skills_dir_is_global(app) {
            let installed = self.install_global(db, skill).await?;
            let path = Self::get_global_skills_dir()?.join(&installed.directory);
            let records = db.get_all_installed_skills()?;
            let metadata = Self::find_global_record(records.values(), &installed.directory);
            return Ok(Self::app_skill_from_path(
                app,
                &path,
                &installed.directory,
                metadata,
            ));
        }

        let source_rel = Self::sanitize_skill_source_path(&skill.directory).ok_or_else(|| {
            anyhow!(format_skill_error(
                "INVALID_SKILL_DIRECTORY",
                &[("directory", &skill.directory)],
                Some("checkZipContent"),
            ))
        })?;
        let install_name = source_rel
            .file_name()
            .and_then(|name| Self::sanitize_install_name(&name.to_string_lossy()))
            .ok_or_else(|| {
                anyhow!(format_skill_error(
                    "INVALID_SKILL_DIRECTORY",
                    &[("directory", &skill.directory)],
                    Some("checkZipContent"),
                ))
            })?;

        let app_dir = Self::get_app_skills_dir(app)?;
        fs::create_dir_all(&app_dir)?;
        let dest = app_dir.join(&install_name);
        if dest.exists() || Self::is_symlink(&dest) {
            return Err(anyhow!(format_skill_error(
                "SKILL_DIRECTORY_CONFLICT",
                &[
                    ("directory", &install_name),
                    ("existing_repo", "current CLI"),
                    (
                        "new_repo",
                        &format!("{}/{}", skill.repo_owner, skill.repo_name)
                    ),
                ],
                Some("uninstallFirst"),
            )));
        }

        let command_home = Self::skills_cli_workspace_for_app(app);
        let repo_branch = self
            .install_remote_command_first(skill, &command_home, &dest, &install_name, false)
            .await?;

        let doc_path = skill
            .readme_url
            .as_deref()
            .and_then(Self::extract_doc_path_from_url)
            .map(|path| {
                if path.ends_with("/SKILL.md") || path == "SKILL.md" {
                    path
                } else {
                    format!("{}/SKILL.md", path.trim_end_matches('/'))
                }
            })
            .unwrap_or_else(|| format!("{}/SKILL.md", skill.directory.trim_end_matches('/')));
        let (disk_name, disk_description) =
            Self::read_skill_name_desc(&dest.join("SKILL.md"), &install_name);
        let installed_skill = InstalledSkill {
            id: Self::app_scoped_skill_id(app, &skill.key),
            name: if disk_name.trim().is_empty() {
                skill.name.clone()
            } else {
                disk_name
            },
            description: disk_description
                .or_else(|| (!skill.description.is_empty()).then(|| skill.description.clone())),
            directory: install_name.clone(),
            repo_owner: Some(skill.repo_owner.clone()),
            repo_name: Some(skill.repo_name.clone()),
            repo_branch: Some(repo_branch.clone()),
            readme_url: Some(Self::build_skill_doc_url(
                &skill.repo_owner,
                &skill.repo_name,
                &repo_branch,
                &doc_path,
            )),
            apps: SkillApps::only(app),
            installed_at: Utc::now().timestamp(),
            content_hash: Self::compute_dir_hash(&dest).ok(),
            updated_at: 0,
        };

        if let Err(error) = db.save_skill(&installed_skill) {
            let _ = Self::remove_path(&dest);
            return Err(error.into());
        }

        Ok(Self::app_skill_from_path(
            app,
            &dest,
            &install_name,
            Some(&installed_skill),
        ))
    }

    /// 安装 Skill
    ///
    /// 流程：
    /// 1. 下载到 SSOT 目录
    /// 2. 保存到数据库
    /// 3. 同步到启用的应用目录
    pub async fn install(
        &self,
        db: &Arc<Database>,
        skill: &DiscoverableSkill,
        current_app: &AppType,
    ) -> Result<InstalledSkill> {
        let ssot_dir = Self::get_ssot_dir()?;

        // 允许多级目录（如 a/b/c），但必须是安全的相对路径。
        let source_rel = Self::sanitize_skill_source_path(&skill.directory).ok_or_else(|| {
            anyhow!(format_skill_error(
                "INVALID_SKILL_DIRECTORY",
                &[("directory", &skill.directory)],
                Some("checkZipContent"),
            ))
        })?;
        // 安装目录名始终使用最后一段，避免在 SSOT 中创建多级目录。
        let install_name = source_rel
            .file_name()
            .and_then(|name| Self::sanitize_install_name(&name.to_string_lossy()))
            .ok_or_else(|| {
                anyhow!(format_skill_error(
                    "INVALID_SKILL_DIRECTORY",
                    &[("directory", &skill.directory)],
                    Some("checkZipContent"),
                ))
            })?;

        // 检查数据库中是否已有同名 directory 的 skill（来自其他仓库）
        let existing_skills = db.get_all_installed_skills()?;
        for existing in existing_skills.values() {
            if existing.directory.eq_ignore_ascii_case(&install_name) {
                // 检查是否来自同一仓库
                let same_repo = existing.repo_owner.as_deref() == Some(&skill.repo_owner)
                    && existing.repo_name.as_deref() == Some(&skill.repo_name);
                if same_repo {
                    // 同一仓库的同名 skill，返回现有记录（可能需要更新启用状态）
                    let mut updated = existing.clone();
                    updated.apps.set_enabled_for(current_app, true);
                    db.save_skill(&updated)?;
                    Self::sync_to_app_dir(&updated.directory, current_app)?;
                    log::info!(
                        "Skill {} 已存在，更新 {:?} 启用状态",
                        updated.name,
                        current_app
                    );
                    return Ok(updated);
                } else {
                    // 不同仓库的同名 skill，报错
                    return Err(anyhow!(format_skill_error(
                        "SKILL_DIRECTORY_CONFLICT",
                        &[
                            ("directory", &install_name),
                            (
                                "existing_repo",
                                &format!(
                                    "{}/{}",
                                    existing.repo_owner.as_deref().unwrap_or("unknown"),
                                    existing.repo_name.as_deref().unwrap_or("unknown")
                                )
                            ),
                            (
                                "new_repo",
                                &format!("{}/{}", skill.repo_owner, skill.repo_name)
                            ),
                        ],
                        Some("uninstallFirst"),
                    )));
                }
            }
        }

        let dest = ssot_dir.join(&install_name);

        let mut repo_branch = skill.repo_branch.clone();

        // 如果已存在则跳过下载
        if !dest.exists() {
            let global_path = Self::global_skills_path()?;
            let global_command = Self::paths_are_same(&ssot_dir, &global_path);
            let command_home = if global_command {
                dirs::home_dir().context(format_skill_error(
                    "GET_HOME_DIR_FAILED",
                    &[],
                    Some("checkPermission"),
                ))?
            } else {
                Self::skills_cli_workspace_for_unified()
            };
            repo_branch = self
                .install_remote_command_first(
                    skill,
                    &command_home,
                    &dest,
                    &install_name,
                    global_command,
                )
                .await?;

            // 使用实际下载成功的分支，避免 readme_url / repo_branch 与真实分支不一致。
            if repo_branch != skill.repo_branch {
                log::info!(
                    "Skill {}/{} 分支自动回退: {} -> {}",
                    skill.repo_owner,
                    skill.repo_name,
                    skill.repo_branch,
                    repo_branch
                );
            }
        }

        let doc_path = skill
            .readme_url
            .as_deref()
            .and_then(Self::extract_doc_path_from_url)
            .map(|path| {
                if path.ends_with("/SKILL.md") || path == "SKILL.md" {
                    path
                } else {
                    format!("{}/SKILL.md", path.trim_end_matches('/'))
                }
            })
            .unwrap_or_else(|| format!("{}/SKILL.md", skill.directory.trim_end_matches('/')));

        let readme_url = Some(Self::build_skill_doc_url(
            &skill.repo_owner,
            &skill.repo_name,
            &repo_branch,
            &doc_path,
        ));

        // 创建 InstalledSkill 记录
        // 计算内容哈希
        let content_hash = Self::compute_dir_hash(&dest).map(Some).unwrap_or_else(|e| {
            log::warn!("Failed to compute content hash for {}: {e}", install_name);
            None
        });

        let installed_skill = InstalledSkill {
            id: skill.key.clone(),
            name: skill.name.clone(),
            description: if skill.description.is_empty() {
                None
            } else {
                Some(skill.description.clone())
            },
            directory: install_name.clone(),
            repo_owner: Some(skill.repo_owner.clone()),
            repo_name: Some(skill.repo_name.clone()),
            repo_branch: Some(repo_branch),
            readme_url,
            apps: SkillApps::only(current_app),
            installed_at: chrono::Utc::now().timestamp(),
            content_hash,
            updated_at: 0,
        };

        // 保存到数据库
        db.save_skill(&installed_skill)?;

        // 同步到当前应用目录
        Self::sync_to_app_dir(&install_name, current_app)?;

        log::info!(
            "Skill {} 安装成功，已启用 {:?}",
            installed_skill.name,
            current_app
        );

        Ok(installed_skill)
    }

    /// 卸载 Skill
    ///
    /// 流程：
    /// 1. 从所有应用目录删除
    /// 2. 从 SSOT 删除
    /// 3. 从数据库删除
    pub fn uninstall(db: &Arc<Database>, id: &str) -> Result<SkillUninstallResult> {
        // 获取 skill 信息
        let skill = db
            .get_installed_skill(id)?
            .ok_or_else(|| anyhow!("Skill not found: {id}"))?;

        let backup_path =
            Self::create_uninstall_backup(&skill)?.map(|path| path.to_string_lossy().to_string());

        // 从所有应用目录删除
        for app in AppType::all() {
            let _ = Self::remove_from_app(&skill.directory, &app);
        }

        // 从 SSOT 删除
        let ssot_dir = Self::get_ssot_dir()?;
        let skill_path = ssot_dir.join(&skill.directory);
        if skill_path.exists() {
            fs::remove_dir_all(&skill_path)?;
        }

        // 从数据库删除
        db.delete_skill(id)?;

        log::info!(
            "Skill {} 卸载成功{}",
            skill.name,
            backup_path
                .as_deref()
                .map(|path| format!(", backup: {path}"))
                .unwrap_or_default()
        );

        Ok(SkillUninstallResult { backup_path })
    }

    /// 仅从指定 CLI 的原生目录卸载 Skill。
    pub fn uninstall_for_app(
        db: &Arc<Database>,
        app: &AppType,
        directory: &str,
    ) -> Result<SkillUninstallResult> {
        Self::ensure_app_skill_support(app)?;
        let relative = Self::sanitize_skill_source_path(directory)
            .ok_or_else(|| anyhow!("Invalid skill directory: {directory}"))?;
        let directory = relative.to_string_lossy().replace('\\', "/");
        let app_dir = Self::get_app_skills_dir(app)?;
        let path = app_dir.join(&relative);
        if (!path.exists() && !Self::is_symlink(&path)) || !path.join("SKILL.md").is_file() {
            return Err(anyhow!(
                "Skill not found in {}: {}",
                app.as_str(),
                directory
            ));
        }
        let global_source = Self::get_global_skills_dir()?.join(&relative);
        if path == global_source {
            return Err(anyhow!(
                "{} reads this Skill directly from the global directory; uninstall it from global management instead",
                app.as_str()
            ));
        }

        let records = db.get_all_installed_skills()?;
        let scoped_prefix = format!("{}:", app.as_str());
        let selected = records
            .values()
            .filter(|skill| {
                skill.apps.is_enabled_for(app) && skill.directory.eq_ignore_ascii_case(&directory)
            })
            .max_by_key(|skill| skill.id.starts_with(&scoped_prefix))
            .cloned();
        let skill = selected.unwrap_or_else(|| {
            let (name, description) =
                Self::read_skill_name_desc(&path.join("SKILL.md"), &directory);
            InstalledSkill {
                id: format!("{}:local:{directory}", app.as_str()),
                name,
                description,
                directory: directory.clone(),
                repo_owner: None,
                repo_name: None,
                repo_branch: None,
                readme_url: None,
                apps: SkillApps::only(app),
                installed_at: Utc::now().timestamp(),
                content_hash: Self::compute_dir_hash(&path).ok(),
                updated_at: 0,
            }
        });

        let backup_path = Self::create_backup_from_source(&skill, &path, Some(app.clone()))?
            .map(|value| value.to_string_lossy().to_string());
        Self::remove_path(&path)?;

        for mut record in records.into_values().filter(|record| {
            record.apps.is_enabled_for(app) && record.directory.eq_ignore_ascii_case(&directory)
        }) {
            record.apps.set_enabled_for(app, false);
            if record.apps.is_empty() {
                db.delete_skill(&record.id)?;
            } else {
                db.save_skill(&record)?;
            }
        }

        Ok(SkillUninstallResult { backup_path })
    }

    /// 从全局库卸载 Skill，并移除所有由它创建的 CLI 软链接。
    pub fn uninstall_global(db: &Arc<Database>, directory: &str) -> Result<SkillUninstallResult> {
        let relative = Self::sanitize_skill_source_path(directory)
            .ok_or_else(|| anyhow!("Invalid skill directory: {directory}"))?;
        let directory = relative.to_string_lossy().replace('\\', "/");
        let global_dir = Self::get_global_skills_dir()?;
        let source = global_dir.join(&relative);
        if !source.is_dir() || !source.join("SKILL.md").is_file() {
            return Err(anyhow!("Global Skill not found: {directory}"));
        }

        let records = db.get_all_installed_skills()?;
        let selected = Self::find_global_record(records.values(), &directory).cloned();
        let skill = selected.unwrap_or_else(|| {
            let (name, description) =
                Self::read_skill_name_desc(&source.join("SKILL.md"), &directory);
            InstalledSkill {
                id: format!("global:local:{directory}"),
                name,
                description,
                directory: directory.clone(),
                repo_owner: None,
                repo_name: None,
                repo_branch: None,
                readme_url: None,
                apps: Self::global_link_states(&source, &directory),
                installed_at: Utc::now().timestamp(),
                content_hash: Self::compute_dir_hash(&source).ok(),
                updated_at: 0,
            }
        });
        let backup_path = Self::create_backup_from_source(&skill, &source, None)?
            .map(|path| path.to_string_lossy().to_string());

        for app in [
            AppType::Claude,
            AppType::Codex,
            AppType::Gemini,
            AppType::OpenCode,
            AppType::Hermes,
        ] {
            let dest = Self::get_app_skills_dir(&app)?.join(&relative);
            if Self::symlink_points_to(&dest, &source) {
                Self::remove_path(&dest)?;
            }
        }
        Self::remove_path(&source)?;

        for record in records.values().filter(|record| {
            record.directory.eq_ignore_ascii_case(&directory)
                && (record.id.starts_with("global:") || !Self::is_app_scoped_skill_id(&record.id))
        }) {
            db.delete_skill(&record.id)?;
        }

        Ok(SkillUninstallResult { backup_path })
    }

    pub fn list_global_backups() -> Result<Vec<SkillBackupEntry>> {
        Ok(Self::list_backups()?
            .into_iter()
            .filter(|entry| entry.source_app.is_none())
            .collect())
    }

    /// 将备份恢复到全局目录；恢复后不会自动创建额外的 CLI 链接。
    pub fn restore_global(db: &Arc<Database>, backup_id: &str) -> Result<GlobalSkill> {
        let backup_path = Self::backup_path_for_id(backup_id)?;
        let metadata = Self::read_backup_metadata(&backup_path)?;
        if metadata.source_app.is_some() {
            return Err(anyhow!("This backup belongs to a CLI Skill"));
        }

        let source = backup_path.join("skill");
        if !source.join("SKILL.md").is_file() {
            return Err(anyhow!(
                "Skill backup is invalid or missing SKILL.md: {}",
                backup_path.display()
            ));
        }
        let relative = Self::sanitize_skill_source_path(&metadata.skill.directory)
            .ok_or_else(|| anyhow!("Invalid skill directory: {}", metadata.skill.directory))?;
        let install_name = relative
            .file_name()
            .and_then(|name| Self::sanitize_install_name(&name.to_string_lossy()))
            .ok_or_else(|| anyhow!("Invalid skill directory: {}", metadata.skill.directory))?;
        let global_dir = Self::get_global_skills_dir()?;
        let dest = global_dir.join(&install_name);
        Self::copy_skill_to_new_dest(&source, &dest, &install_name)?;

        let mut restored = metadata.skill;
        let old_id = restored.id.clone();
        restored.id = Self::global_skill_id(&restored.id);
        restored.directory = install_name.clone();
        restored.apps = Self::global_link_states(&dest, &install_name);
        restored.installed_at = Utc::now().timestamp();
        restored.updated_at = 0;
        restored.content_hash = Self::compute_dir_hash(&dest).ok();
        if let Err(error) = db.save_skill(&restored) {
            let _ = Self::remove_path(&dest);
            return Err(error.into());
        }
        if old_id != restored.id {
            let _ = db.delete_skill(&old_id);
        }

        Ok(Self::global_skill_from_path(
            &dest,
            &install_name,
            Some(&restored),
        ))
    }

    // ========== 更新检测 ==========

    /// 计算目录内容的 SHA-256 哈希
    ///
    /// 递归遍历目录下所有非隐藏文件，按相对路径字典序排列，
    /// 将 "相对路径\0内容\0" 逐文件 feed 给同一个 hasher。
    pub fn compute_dir_hash(dir: &Path) -> Result<String> {
        use sha2::{Digest, Sha256};

        let mut files: Vec<PathBuf> = Vec::new();
        Self::collect_files_for_hash(dir, dir, &mut files)?;
        files.sort();

        let mut hasher = Sha256::new();
        for file_path in &files {
            let relative = file_path.strip_prefix(dir).unwrap_or(file_path);
            let rel_str = relative.to_string_lossy().replace('\\', "/");
            hasher.update(rel_str.as_bytes());
            hasher.update(b"\0");
            let content = fs::read(file_path)
                .with_context(|| format!("读取文件失败: {}", file_path.display()))?;
            hasher.update(&content);
            hasher.update(b"\0");
        }

        Ok(format!("{:x}", hasher.finalize()))
    }

    /// 递归收集目录下所有非隐藏文件
    #[allow(clippy::only_used_in_recursion)]
    fn collect_files_for_hash(base: &Path, current: &Path, files: &mut Vec<PathBuf>) -> Result<()> {
        let entries = fs::read_dir(current)
            .with_context(|| format!("读取目录失败: {}", current.display()))?;
        for entry in entries {
            let entry = entry?;
            let name = entry.file_name().to_string_lossy().to_string();
            if name.starts_with('.') {
                continue;
            }
            let path = entry.path();
            if path.is_dir() {
                Self::collect_files_for_hash(base, &path, files)?;
            } else {
                files.push(path);
            }
        }
        Ok(())
    }

    /// 检查所有已安装 Skill 的更新
    ///
    /// 仅检查有 repo_owner 的 Skill（本地 Skill 跳过），
    /// 按仓库分组下载，避免重复下载同一仓库。
    pub async fn check_updates(&self, db: &Arc<Database>) -> Result<Vec<SkillUpdateInfo>> {
        let skills = db.get_all_installed_skills()?;
        let mut updates = Vec::new();

        // 按 (owner, name, branch) 分组
        let mut repo_groups: HashMap<(String, String, String), Vec<InstalledSkill>> =
            HashMap::new();

        for skill in skills.into_values() {
            let (owner, name, branch) =
                match (&skill.repo_owner, &skill.repo_name, &skill.repo_branch) {
                    (Some(o), Some(n), Some(b)) => (o.clone(), n.clone(), b.clone()),
                    (Some(o), Some(n), None) => (o.clone(), n.clone(), "main".to_string()),
                    _ => continue,
                };
            repo_groups
                .entry((owner, name, branch))
                .or_default()
                .push(skill);
        }

        let ssot_dir = Self::get_ssot_dir()?;

        for ((owner, name, branch), group_skills) in &repo_groups {
            let repo = SkillRepo {
                owner: owner.clone(),
                name: name.clone(),
                branch: branch.clone(),
                enabled: true,
            };

            // 下载仓库 ZIP
            let (temp_dir, _used_branch) = match timeout(
                std::time::Duration::from_secs(60),
                self.download_repo(&repo),
            )
            .await
            {
                Ok(Ok(result)) => result,
                Ok(Err(e)) => {
                    log::warn!("检查更新时下载 {}/{} 失败: {e}", owner, name);
                    continue;
                }
                Err(_) => {
                    log::warn!("检查更新时下载 {}/{} 超时", owner, name);
                    continue;
                }
            };

            // 扫描仓库中的所有 Skill 目录
            let mut remote_skills: Vec<DiscoverableSkill> = Vec::new();
            let _ = self.scan_dir_recursive(&temp_dir, &temp_dir, &repo, &mut remote_skills);

            for skill in group_skills {
                // 在远程仓库中找到匹配的 Skill 目录
                let remote_match = remote_skills.iter().find(|rs| {
                    // 匹配方式：安装名称的最后一段
                    let remote_install_name =
                        rs.directory.rsplit('/').next().unwrap_or(&rs.directory);
                    remote_install_name.eq_ignore_ascii_case(&skill.directory)
                });

                let remote_skill_dir = match remote_match {
                    Some(rs) => match Self::resolve_skill_source_dir(&temp_dir, &rs.directory) {
                        Some(path) => path,
                        None => continue,
                    },
                    None => continue,
                };

                let remote_hash = match Self::compute_dir_hash(&remote_skill_dir) {
                    Ok(h) => h,
                    Err(e) => {
                        log::warn!("计算远程哈希失败 {}: {e}", skill.id);
                        continue;
                    }
                };

                // 本地哈希：优先数据库，否则实时计算
                let local_hash = match &skill.content_hash {
                    Some(h) => Some(h.clone()),
                    None => {
                        let local_dir = ssot_dir.join(&skill.directory);
                        if local_dir.exists() {
                            match Self::compute_dir_hash(&local_dir) {
                                Ok(h) => {
                                    let _ = db.update_skill_hash(&skill.id, &h, 0);
                                    Some(h)
                                }
                                Err(_) => None,
                            }
                        } else {
                            None
                        }
                    }
                };

                if local_hash.as_deref() != Some(&remote_hash) {
                    updates.push(SkillUpdateInfo {
                        id: skill.id.clone(),
                        name: skill.name.clone(),
                        current_hash: local_hash,
                        remote_hash,
                    });
                }
            }

            let _ = fs::remove_dir_all(&temp_dir);
        }

        Ok(updates)
    }

    /// 仅检查全局库中带仓库元数据的 Skills。
    pub async fn check_global_updates(&self, db: &Arc<Database>) -> Result<Vec<SkillUpdateInfo>> {
        let global = Self::get_global(db)?;
        let mut updates = Vec::new();
        let mut repo_groups: HashMap<(String, String, String), Vec<GlobalSkill>> = HashMap::new();

        for skill in global.skills {
            let (owner, name, branch) =
                match (&skill.repo_owner, &skill.repo_name, &skill.repo_branch) {
                    (Some(owner), Some(name), Some(branch)) => {
                        (owner.clone(), name.clone(), branch.clone())
                    }
                    (Some(owner), Some(name), None) => {
                        (owner.clone(), name.clone(), "main".to_string())
                    }
                    _ => continue,
                };
            repo_groups
                .entry((owner, name, branch))
                .or_default()
                .push(skill);
        }

        for ((owner, name, branch), skills) in repo_groups {
            let repo = SkillRepo {
                owner: owner.clone(),
                name: name.clone(),
                branch,
                enabled: true,
            };
            let (temp_dir, _) = match timeout(
                std::time::Duration::from_secs(60),
                self.download_repo(&repo),
            )
            .await
            {
                Ok(Ok(result)) => result,
                Ok(Err(error)) => {
                    log::warn!("检查全局 Skill 更新时下载 {owner}/{name} 失败: {error}");
                    continue;
                }
                Err(_) => {
                    log::warn!("检查全局 Skill 更新时下载 {owner}/{name} 超时");
                    continue;
                }
            };

            let mut remote_skills = Vec::new();
            let _ = self.scan_dir_recursive(&temp_dir, &temp_dir, &repo, &mut remote_skills);
            for skill in skills {
                let Some(remote) = remote_skills.iter().find(|candidate| {
                    candidate
                        .directory
                        .rsplit('/')
                        .next()
                        .unwrap_or(&candidate.directory)
                        .eq_ignore_ascii_case(&skill.directory)
                }) else {
                    continue;
                };
                let Some(remote_dir) = Self::resolve_skill_source_dir(&temp_dir, &remote.directory)
                else {
                    continue;
                };
                let Ok(remote_hash) = Self::compute_dir_hash(&remote_dir) else {
                    continue;
                };
                let current_hash = Self::compute_dir_hash(Path::new(&skill.path)).ok();
                if current_hash.as_deref() != Some(&remote_hash) {
                    updates.push(SkillUpdateInfo {
                        id: skill.id,
                        name: skill.name,
                        current_hash,
                        remote_hash,
                    });
                }
            }
            let _ = fs::remove_dir_all(&temp_dir);
        }

        Ok(updates)
    }

    /// 仅检查指定 CLI 原生目录中的仓库型 Skills；全局软链接由全局页更新。
    pub async fn check_app_updates(
        &self,
        db: &Arc<Database>,
        app: &AppType,
    ) -> Result<Vec<SkillUpdateInfo>> {
        Self::ensure_app_skill_support(app)?;
        let snapshot = Self::get_for_app(db, app)?;
        let mut updates = Vec::new();
        let mut repo_groups: HashMap<(String, String, String), Vec<AppSkill>> = HashMap::new();

        for skill in snapshot
            .skills
            .into_iter()
            .filter(|skill| !skill.managed_globally)
        {
            let (owner, name, branch) =
                match (&skill.repo_owner, &skill.repo_name, &skill.repo_branch) {
                    (Some(owner), Some(name), Some(branch)) => {
                        (owner.clone(), name.clone(), branch.clone())
                    }
                    (Some(owner), Some(name), None) => {
                        (owner.clone(), name.clone(), "main".to_string())
                    }
                    _ => continue,
                };
            repo_groups
                .entry((owner, name, branch))
                .or_default()
                .push(skill);
        }

        for ((owner, name, branch), skills) in repo_groups {
            let repo = SkillRepo {
                owner: owner.clone(),
                name: name.clone(),
                branch,
                enabled: true,
            };
            let (temp_dir, _) = match timeout(
                std::time::Duration::from_secs(60),
                self.download_repo(&repo),
            )
            .await
            {
                Ok(Ok(result)) => result,
                Ok(Err(error)) => {
                    log::warn!(
                        "检查 {} Skill 更新时下载 {owner}/{name} 失败: {error}",
                        app.as_str()
                    );
                    continue;
                }
                Err(_) => {
                    log::warn!("检查 {} Skill 更新时下载 {owner}/{name} 超时", app.as_str());
                    continue;
                }
            };

            let mut remote_skills = Vec::new();
            let _ = self.scan_dir_recursive(&temp_dir, &temp_dir, &repo, &mut remote_skills);
            for skill in skills {
                let install_name = skill
                    .directory
                    .rsplit('/')
                    .next()
                    .unwrap_or(&skill.directory);
                let Some(remote) = remote_skills.iter().find(|candidate| {
                    candidate
                        .directory
                        .rsplit('/')
                        .next()
                        .unwrap_or(&candidate.directory)
                        .eq_ignore_ascii_case(install_name)
                }) else {
                    continue;
                };
                let Some(remote_dir) = Self::resolve_skill_source_dir(&temp_dir, &remote.directory)
                else {
                    continue;
                };
                let Ok(remote_hash) = Self::compute_dir_hash(&remote_dir) else {
                    continue;
                };
                let current_hash = Self::compute_dir_hash(Path::new(&skill.path)).ok();
                if current_hash.as_deref() != Some(&remote_hash) {
                    updates.push(SkillUpdateInfo {
                        id: skill.id,
                        name: skill.name,
                        current_hash,
                        remote_hash,
                    });
                }
            }
            let _ = fs::remove_dir_all(&temp_dir);
        }

        Ok(updates)
    }

    fn save_updated_global_skill(
        db: &Arc<Database>,
        skill: &InstalledSkill,
        owner: &str,
        repository: &str,
        branch: &str,
        dest: &Path,
    ) -> Result<GlobalSkill> {
        let (new_name, new_description) =
            Self::read_skill_name_desc(&dest.join("SKILL.md"), &skill.directory);
        let doc_path = skill
            .readme_url
            .as_deref()
            .and_then(Self::extract_doc_path_from_url)
            .unwrap_or_else(|| format!("{}/SKILL.md", skill.directory.trim_end_matches('/')));
        let old_id = skill.id.clone();
        let updated = InstalledSkill {
            id: Self::global_skill_id(&skill.id),
            name: new_name,
            description: new_description,
            directory: skill.directory.clone(),
            repo_owner: skill.repo_owner.clone(),
            repo_name: skill.repo_name.clone(),
            repo_branch: Some(branch.to_string()),
            readme_url: Some(Self::build_skill_doc_url(
                owner, repository, branch, &doc_path,
            )),
            apps: Self::global_link_states(dest, &skill.directory),
            installed_at: skill.installed_at,
            content_hash: Self::compute_dir_hash(dest).ok(),
            updated_at: Utc::now().timestamp(),
        };
        db.save_skill(&updated)?;
        if old_id != updated.id {
            let _ = db.delete_skill(&old_id);
        }
        Ok(Self::global_skill_from_path(
            dest,
            &skill.directory,
            Some(&updated),
        ))
    }

    fn save_updated_app_skill(
        db: &Arc<Database>,
        app: &AppType,
        app_skill: &AppSkill,
        skill_id: &str,
        owner: &str,
        repository: &str,
        branch: &str,
        dest: &Path,
    ) -> Result<AppSkill> {
        let records = db.get_all_installed_skills()?;
        let previous = records.get(skill_id).cloned();
        let (new_name, new_description) =
            Self::read_skill_name_desc(&dest.join("SKILL.md"), &app_skill.directory);
        let doc_path = app_skill
            .readme_url
            .as_deref()
            .and_then(Self::extract_doc_path_from_url)
            .unwrap_or_else(|| format!("{}/SKILL.md", app_skill.directory.trim_end_matches('/')));
        let updated = InstalledSkill {
            id: Self::app_scoped_skill_id(app, skill_id),
            name: new_name,
            description: new_description,
            directory: app_skill.directory.clone(),
            repo_owner: app_skill.repo_owner.clone(),
            repo_name: app_skill.repo_name.clone(),
            repo_branch: Some(branch.to_string()),
            readme_url: Some(Self::build_skill_doc_url(
                owner, repository, branch, &doc_path,
            )),
            apps: SkillApps::only(app),
            installed_at: app_skill.installed_at,
            content_hash: Self::compute_dir_hash(dest).ok(),
            updated_at: Utc::now().timestamp(),
        };
        db.save_skill(&updated)?;
        if let Some(mut previous) = previous.filter(|record| record.id != updated.id) {
            previous.apps.set_enabled_for(app, false);
            if previous.apps.is_empty() {
                let _ = db.delete_skill(&previous.id);
            } else {
                db.save_skill(&previous)?;
            }
        }
        Ok(Self::app_skill_from_path(
            app,
            dest,
            &app_skill.directory,
            Some(&updated),
        ))
    }

    fn save_updated_unified_skill(
        db: &Arc<Database>,
        skill: &InstalledSkill,
        owner: &str,
        repository: &str,
        branch: &str,
        dest: &Path,
    ) -> Result<InstalledSkill> {
        let (new_name, new_description) =
            Self::read_skill_name_desc(&dest.join("SKILL.md"), &skill.directory);
        let doc_path = skill
            .readme_url
            .as_deref()
            .and_then(Self::extract_doc_path_from_url)
            .unwrap_or_else(|| format!("{}/SKILL.md", skill.directory.trim_end_matches('/')));
        let updated = InstalledSkill {
            id: skill.id.clone(),
            name: new_name,
            description: new_description,
            directory: skill.directory.clone(),
            repo_owner: skill.repo_owner.clone(),
            repo_name: skill.repo_name.clone(),
            repo_branch: Some(branch.to_string()),
            readme_url: Some(Self::build_skill_doc_url(
                owner, repository, branch, &doc_path,
            )),
            apps: skill.apps.clone(),
            installed_at: skill.installed_at,
            content_hash: Self::compute_dir_hash(dest).ok(),
            updated_at: Utc::now().timestamp(),
        };

        db.save_skill(&updated)?;
        for app in updated.apps.enabled_apps() {
            if let Err(error) = Self::sync_to_app_dir(&updated.directory, &app) {
                log::warn!("同步更新后的 Skill 到 {:?} 失败: {error}", app);
            }
        }

        log::info!("Skill {} 更新成功", updated.name);
        Ok(updated)
    }

    /// 更新全局库中的单个 Skill；现有 CLI 软链接会继续指向同一路径。
    pub async fn update_global_skill(
        &self,
        db: &Arc<Database>,
        skill_id: &str,
    ) -> Result<GlobalSkill> {
        let records = db.get_all_installed_skills()?;
        let skill = records
            .get(skill_id)
            .filter(|skill| {
                skill.id.starts_with("global:") || !Self::is_app_scoped_skill_id(&skill.id)
            })
            .cloned()
            .ok_or_else(|| anyhow!("Global Skill not found: {skill_id}"))?;
        let (owner, name, branch) = match (&skill.repo_owner, &skill.repo_name) {
            (Some(owner), Some(name)) => (
                owner.clone(),
                name.clone(),
                skill
                    .repo_branch
                    .clone()
                    .unwrap_or_else(|| "main".to_string()),
            ),
            _ => return Err(anyhow!("Cannot update local Skill: {skill_id}")),
        };
        let repo = SkillRepo {
            owner: owner.clone(),
            name: name.clone(),
            branch: branch.clone(),
            enabled: true,
        };
        let global_dir = Self::get_global_skills_dir()?;
        let dest = global_dir.join(&skill.directory);
        Self::validate_sync_source_dir(&dest, &skill.directory)?;
        let _ = Self::create_backup_from_source(&skill, &dest, None)?;

        let command_home = dirs::home_dir().context(format_skill_error(
            "GET_HOME_DIR_FAILED",
            &[],
            Some("checkPermission"),
        ))?;
        match Self::try_update_remote_with_skills_cli(&command_home, &dest, &skill.directory, true)
            .await
        {
            Ok(recorded_branch) => {
                let updated_branch = recorded_branch.unwrap_or(branch);
                return Self::save_updated_global_skill(
                    db,
                    &skill,
                    &owner,
                    &name,
                    &updated_branch,
                    &dest,
                );
            }
            Err(command_error) => {
                log::warn!(
                    "skills CLI update failed for global Skill {}; falling back to archive: {command_error}",
                    skill.directory
                );
            }
        }

        let (temp_dir, used_branch) = timeout(
            std::time::Duration::from_secs(60),
            self.download_repo(&repo),
        )
        .await
        .map_err(|_| {
            anyhow!(format_skill_error(
                "DOWNLOAD_TIMEOUT",
                &[("owner", &owner), ("name", &name), ("timeout", "60")],
                Some("checkNetwork"),
            ))
        })??;

        let result = (|| -> Result<GlobalSkill> {
            let mut remote_skills = Vec::new();
            self.scan_dir_recursive(&temp_dir, &temp_dir, &repo, &mut remote_skills)?;
            let remote = remote_skills
                .iter()
                .find(|candidate| {
                    candidate
                        .directory
                        .rsplit('/')
                        .next()
                        .unwrap_or(&candidate.directory)
                        .eq_ignore_ascii_case(&skill.directory)
                })
                .ok_or_else(|| {
                    anyhow!(format_skill_error(
                        "SKILL_DIR_NOT_FOUND",
                        &[("path", &skill.directory)],
                        Some("checkRepoUrl"),
                    ))
                })?;
            let source =
                Self::resolve_skill_source_dir(&temp_dir, &remote.directory).ok_or_else(|| {
                    anyhow!(format_skill_error(
                        "SKILL_DIR_NOT_FOUND",
                        &[("path", &remote.directory)],
                        Some("checkRepoUrl"),
                    ))
                })?;
            Self::replace_dest_with_copy(&source, &dest, &skill.directory)?;
            Self::save_updated_global_skill(db, &skill, &owner, &name, &used_branch, &dest)
        })();
        let _ = fs::remove_dir_all(&temp_dir);
        result
    }

    /// 更新指定 CLI 原生目录中的单个 Skill，不修改其他 CLI 或全局库。
    pub async fn update_app_skill(
        &self,
        db: &Arc<Database>,
        app: &AppType,
        skill_id: &str,
    ) -> Result<AppSkill> {
        Self::ensure_app_skill_support(app)?;
        let snapshot = Self::get_for_app(db, app)?;
        let app_skill = snapshot
            .skills
            .into_iter()
            .find(|skill| skill.id == skill_id)
            .ok_or_else(|| anyhow!("Skill not found in {}: {skill_id}", app.as_str()))?;
        if app_skill.managed_globally {
            return Err(anyhow!(
                "Global linked Skills must be updated from the global library"
            ));
        }
        let (owner, name, branch) = match (&app_skill.repo_owner, &app_skill.repo_name) {
            (Some(owner), Some(name)) => (
                owner.clone(),
                name.clone(),
                app_skill
                    .repo_branch
                    .clone()
                    .unwrap_or_else(|| "main".to_string()),
            ),
            _ => return Err(anyhow!("Cannot update local Skill: {skill_id}")),
        };
        let repo = SkillRepo {
            owner: owner.clone(),
            name: name.clone(),
            branch: branch.clone(),
            enabled: true,
        };
        let dest = PathBuf::from(&app_skill.path);
        Self::validate_sync_source_dir(&dest, &app_skill.directory)?;

        let records = db.get_all_installed_skills()?;
        let backup_record = records
            .get(skill_id)
            .cloned()
            .unwrap_or_else(|| InstalledSkill {
                id: skill_id.to_string(),
                name: app_skill.name.clone(),
                description: app_skill.description.clone(),
                directory: app_skill.directory.clone(),
                repo_owner: app_skill.repo_owner.clone(),
                repo_name: app_skill.repo_name.clone(),
                repo_branch: app_skill.repo_branch.clone(),
                readme_url: app_skill.readme_url.clone(),
                apps: SkillApps::only(app),
                installed_at: app_skill.installed_at,
                content_hash: app_skill.content_hash.clone(),
                updated_at: app_skill.updated_at,
            });
        let _ = Self::create_backup_from_source(&backup_record, &dest, Some(app.clone()))?;

        let command_home = Self::skills_cli_workspace_for_app(app);
        match Self::try_update_remote_with_skills_cli(
            &command_home,
            &dest,
            &app_skill.directory,
            false,
        )
        .await
        {
            Ok(recorded_branch) => {
                let updated_branch = recorded_branch.unwrap_or(branch);
                return Self::save_updated_app_skill(
                    db,
                    app,
                    &app_skill,
                    skill_id,
                    &owner,
                    &name,
                    &updated_branch,
                    &dest,
                );
            }
            Err(command_error) => {
                log::warn!(
                    "skills CLI update failed for {} Skill {}; falling back to archive: {command_error}",
                    app.as_str(),
                    app_skill.directory
                );
            }
        }

        let (temp_dir, used_branch) = timeout(
            std::time::Duration::from_secs(60),
            self.download_repo(&repo),
        )
        .await
        .map_err(|_| {
            anyhow!(format_skill_error(
                "DOWNLOAD_TIMEOUT",
                &[("owner", &owner), ("name", &name), ("timeout", "60")],
                Some("checkNetwork"),
            ))
        })??;

        let result = (|| -> Result<AppSkill> {
            let mut remote_skills = Vec::new();
            self.scan_dir_recursive(&temp_dir, &temp_dir, &repo, &mut remote_skills)?;
            let install_name = app_skill
                .directory
                .rsplit('/')
                .next()
                .unwrap_or(&app_skill.directory);
            let remote = remote_skills
                .iter()
                .find(|candidate| {
                    candidate
                        .directory
                        .rsplit('/')
                        .next()
                        .unwrap_or(&candidate.directory)
                        .eq_ignore_ascii_case(install_name)
                })
                .ok_or_else(|| {
                    anyhow!(format_skill_error(
                        "SKILL_DIR_NOT_FOUND",
                        &[("path", &app_skill.directory)],
                        Some("checkRepoUrl"),
                    ))
                })?;
            let source =
                Self::resolve_skill_source_dir(&temp_dir, &remote.directory).ok_or_else(|| {
                    anyhow!(format_skill_error(
                        "SKILL_DIR_NOT_FOUND",
                        &[("path", &remote.directory)],
                        Some("checkRepoUrl"),
                    ))
                })?;
            Self::replace_dest_with_copy(&source, &dest, &app_skill.directory)?;
            Self::save_updated_app_skill(
                db,
                app,
                &app_skill,
                skill_id,
                &owner,
                &name,
                &used_branch,
                &dest,
            )
        })();
        let _ = fs::remove_dir_all(&temp_dir);
        result
    }

    /// 更新单个 Skill（重新下载并替换本地文件）
    pub async fn update_skill(&self, db: &Arc<Database>, skill_id: &str) -> Result<InstalledSkill> {
        let skill = db
            .get_installed_skill(skill_id)?
            .ok_or_else(|| anyhow!("Skill not found: {skill_id}"))?;

        let (owner, name, branch) = match (&skill.repo_owner, &skill.repo_name) {
            (Some(o), Some(n)) => (
                o.clone(),
                n.clone(),
                skill
                    .repo_branch
                    .clone()
                    .unwrap_or_else(|| "main".to_string()),
            ),
            _ => return Err(anyhow!("Cannot update local skill: {skill_id}")),
        };

        let ssot_dir = Self::get_ssot_dir()?;
        let dest = ssot_dir.join(&skill.directory);
        Self::validate_sync_source_dir(&dest, &skill.directory)?;
        let _ = Self::create_backup_from_source(&skill, &dest, None)?;

        let global_path = Self::global_skills_path()?;
        let global_command = Self::paths_are_same(&ssot_dir, &global_path);
        let command_home = if global_command {
            dirs::home_dir().context(format_skill_error(
                "GET_HOME_DIR_FAILED",
                &[],
                Some("checkPermission"),
            ))?
        } else {
            Self::skills_cli_workspace_for_unified()
        };
        match Self::try_update_remote_with_skills_cli(
            &command_home,
            &dest,
            &skill.directory,
            global_command,
        )
        .await
        {
            Ok(recorded_branch) => {
                let updated_branch = recorded_branch.unwrap_or(branch);
                return Self::save_updated_unified_skill(
                    db,
                    &skill,
                    &owner,
                    &name,
                    &updated_branch,
                    &dest,
                );
            }
            Err(command_error) => {
                log::warn!(
                    "skills CLI update failed for compatible Skill {}; falling back to archive: {command_error}",
                    skill.directory
                );
            }
        }

        let repo = SkillRepo {
            owner: owner.clone(),
            name: name.clone(),
            branch: branch.clone(),
            enabled: true,
        };

        let (temp_dir, used_branch) = timeout(
            std::time::Duration::from_secs(60),
            self.download_repo(&repo),
        )
        .await
        .map_err(|_| {
            anyhow!(format_skill_error(
                "DOWNLOAD_TIMEOUT",
                &[("owner", &owner), ("name", &name), ("timeout", "60")],
                Some("checkNetwork"),
            ))
        })??;

        let result = (|| -> Result<InstalledSkill> {
            let mut remote_skills = Vec::new();
            self.scan_dir_recursive(&temp_dir, &temp_dir, &repo, &mut remote_skills)?;
            let remote_match = remote_skills
                .iter()
                .find(|remote| {
                    remote
                        .directory
                        .rsplit('/')
                        .next()
                        .unwrap_or(&remote.directory)
                        .eq_ignore_ascii_case(&skill.directory)
                })
                .ok_or_else(|| {
                    anyhow!(format_skill_error(
                        "SKILL_DIR_NOT_FOUND",
                        &[("path", &skill.directory)],
                        Some("checkRepoUrl"),
                    ))
                })?;
            let source = Self::resolve_skill_source_dir(&temp_dir, &remote_match.directory)
                .ok_or_else(|| {
                    anyhow!(format_skill_error(
                        "SKILL_DIR_NOT_FOUND",
                        &[("path", &remote_match.directory)],
                        Some("checkRepoUrl"),
                    ))
                })?;
            Self::replace_dest_with_copy(&source, &dest, &skill.directory)?;
            Self::save_updated_unified_skill(db, &skill, &owner, &name, &used_branch, &dest)
        })();
        let _ = fs::remove_dir_all(&temp_dir);
        result
    }

    /// 为缺少 content_hash 的已安装 Skill 补算哈希
    pub fn backfill_content_hashes(db: &Arc<Database>) -> Result<usize> {
        let skills = db.get_all_installed_skills()?;
        let ssot_dir = Self::get_ssot_dir()?;
        let mut count = 0;

        for skill in skills.values() {
            if skill.content_hash.is_some() {
                continue;
            }
            let skill_dir = ssot_dir.join(&skill.directory);
            if !skill_dir.exists() {
                continue;
            }
            match Self::compute_dir_hash(&skill_dir) {
                Ok(hash) => {
                    let _ = db.update_skill_hash(&skill.id, &hash, 0);
                    count += 1;
                }
                Err(e) => {
                    log::warn!("补算哈希失败 {}: {e}", skill.id);
                }
            }
        }

        if count > 0 {
            log::info!("已为 {count} 个 Skill 补算内容哈希");
        }
        Ok(count)
    }

    /// 迁移 Skill 存储位置（在两个 SSOT 目录间移动文件）
    ///
    /// 安全策略：先移文件，后改设置。中途崩溃时设置仍指向旧目录。
    pub fn migrate_storage(
        db: &Arc<Database>,
        target: SkillStorageLocation,
    ) -> Result<MigrationResult> {
        let current = crate::settings::get_skill_storage_location();
        if current == target {
            return Ok(MigrationResult {
                migrated_count: 0,
                skipped_count: 0,
                errors: vec![],
            });
        }

        // 1. 解析旧目录和新目录（不改设置）
        let old_dir = Self::get_ssot_dir()?;
        let new_dir = match target {
            SkillStorageLocation::CcSwitch => get_app_config_dir().join("skills"),
            SkillStorageLocation::Unified => {
                let home = dirs::home_dir().context("Cannot determine home directory")?;
                home.join(".agents").join("skills")
            }
        };
        fs::create_dir_all(&new_dir)?;

        // 2. 逐个移动 skill 目录
        let skills = db.get_all_installed_skills()?;
        let mut result = MigrationResult {
            migrated_count: 0,
            skipped_count: 0,
            errors: vec![],
        };

        for skill in skills.values() {
            let src = old_dir.join(&skill.directory);
            let dst = new_dir.join(&skill.directory);

            if !src.exists() {
                result.skipped_count += 1;
                continue;
            }
            if dst.exists() {
                result.skipped_count += 1;
                continue;
            }

            // 优先 rename（同文件系统原子操作），失败则 copy+delete
            match fs::rename(&src, &dst) {
                Ok(()) => result.migrated_count += 1,
                Err(_) => match Self::copy_dir_recursive(&src, &dst) {
                    Ok(()) => {
                        let _ = fs::remove_dir_all(&src);
                        result.migrated_count += 1;
                    }
                    Err(e) => {
                        result.errors.push(format!("{}: {e}", skill.directory));
                    }
                },
            }
        }

        // 3. 文件移动完成后才持久化设置
        crate::settings::set_skill_storage_location(target)?;

        // 4. 刷新所有应用目录的 symlink（指向新 SSOT）
        for app in AppType::all() {
            let _ = Self::sync_to_app(db, &app);
        }

        log::info!(
            "Skill 存储迁移完成: {} 迁移, {} 跳过, {} 错误",
            result.migrated_count,
            result.skipped_count,
            result.errors.len()
        );

        Ok(result)
    }

    pub fn list_backups() -> Result<Vec<SkillBackupEntry>> {
        let backup_dir = Self::get_backup_dir()?;
        let mut entries = Vec::new();

        for entry in fs::read_dir(&backup_dir)? {
            let entry = match entry {
                Ok(entry) => entry,
                Err(err) => {
                    log::warn!("读取 Skill 备份目录项失败: {err}");
                    continue;
                }
            };
            let path = entry.path();
            if !path.is_dir() {
                continue;
            }

            match Self::read_backup_metadata(&path) {
                Ok(metadata) => entries.push(SkillBackupEntry {
                    backup_id: entry.file_name().to_string_lossy().to_string(),
                    backup_path: path.to_string_lossy().to_string(),
                    created_at: metadata.backup_created_at,
                    skill: metadata.skill,
                    source_app: metadata.source_app,
                }),
                Err(err) => {
                    log::warn!("解析 Skill 备份失败 {}: {err:#}", path.display());
                }
            }
        }

        entries.sort_by_key(|entry| std::cmp::Reverse(entry.created_at));
        Ok(entries)
    }

    pub fn list_backups_for_app(app: &AppType) -> Result<Vec<SkillBackupEntry>> {
        Self::ensure_app_skill_support(app)?;
        Ok(Self::list_backups()?
            .into_iter()
            .filter(|entry| entry.source_app.as_ref() == Some(app))
            .collect())
    }

    pub fn delete_backup(backup_id: &str) -> Result<()> {
        let backup_path = Self::backup_path_for_id(backup_id)?;
        let metadata = fs::symlink_metadata(&backup_path)
            .with_context(|| format!("failed to access {}", backup_path.display()))?;

        if !metadata.is_dir() {
            return Err(anyhow!(
                "Skill backup is not a directory: {}",
                backup_path.display()
            ));
        }

        fs::remove_dir_all(&backup_path)
            .with_context(|| format!("failed to delete {}", backup_path.display()))?;

        log::info!("Skill 备份已删除: {}", backup_path.display());
        Ok(())
    }

    pub fn restore_from_backup(
        db: &Arc<Database>,
        backup_id: &str,
        current_app: &AppType,
    ) -> Result<InstalledSkill> {
        let backup_path = Self::backup_path_for_id(backup_id)?;
        let metadata = Self::read_backup_metadata(&backup_path)?;
        let backup_skill_dir = backup_path.join("skill");
        if !backup_skill_dir.join("SKILL.md").exists() {
            return Err(anyhow!(
                "Skill backup is invalid or missing SKILL.md: {}",
                backup_path.display()
            ));
        }

        let existing_skills = db.get_all_installed_skills()?;
        if existing_skills.contains_key(&metadata.skill.id)
            || existing_skills.values().any(|skill| {
                skill
                    .directory
                    .eq_ignore_ascii_case(&metadata.skill.directory)
            })
        {
            return Err(anyhow!(
                "Skill already exists, please uninstall the current one first: {}",
                metadata.skill.directory
            ));
        }

        let ssot_dir = Self::get_ssot_dir()?;
        let restore_path = ssot_dir.join(&metadata.skill.directory);
        if restore_path.exists() || Self::is_symlink(&restore_path) {
            return Err(anyhow!(
                "Restore target already exists: {}",
                restore_path.display()
            ));
        }

        let mut restored_skill = metadata.skill;
        restored_skill.installed_at = Utc::now().timestamp();
        restored_skill.apps = SkillApps::only(current_app);
        restored_skill.updated_at = 0;

        Self::copy_dir_recursive(&backup_skill_dir, &restore_path)?;

        // 重新计算内容哈希
        restored_skill.content_hash = Self::compute_dir_hash(&restore_path).ok();

        if let Err(err) = db.save_skill(&restored_skill) {
            let _ = fs::remove_dir_all(&restore_path);
            return Err(err.into());
        }

        if !restored_skill.apps.is_empty() {
            if let Err(err) = Self::sync_to_app_dir(&restored_skill.directory, current_app) {
                let _ = db.delete_skill(&restored_skill.id);
                let _ = fs::remove_dir_all(&restore_path);
                return Err(err);
            }
        }

        log::info!(
            "Skill {} 已从备份恢复到 {}",
            restored_skill.name,
            restore_path.display()
        );

        Ok(restored_skill)
    }

    /// 将备份直接恢复到指定 CLI 的原生 Skills 目录。
    pub fn restore_for_app(db: &Arc<Database>, backup_id: &str, app: &AppType) -> Result<AppSkill> {
        Self::ensure_app_skill_support(app)?;
        if Self::app_primary_skills_dir_is_global(app) {
            return Err(anyhow!(
                "{} reads the global Skills directory directly; restore global backups from global management",
                app.as_str()
            ));
        }
        let backup_path = Self::backup_path_for_id(backup_id)?;
        let metadata = Self::read_backup_metadata(&backup_path)?;
        if metadata.source_app.as_ref() != Some(app) {
            return Err(anyhow!(
                "Skill backup belongs to {}, not {}",
                metadata
                    .source_app
                    .as_ref()
                    .map(AppType::as_str)
                    .unwrap_or("unknown"),
                app.as_str()
            ));
        }

        let relative = Self::sanitize_skill_source_path(&metadata.skill.directory)
            .ok_or_else(|| anyhow!("Invalid skill directory: {}", metadata.skill.directory))?;
        let directory = relative.to_string_lossy().replace('\\', "/");
        let source = backup_path.join("skill");
        if !source.join("SKILL.md").is_file() {
            return Err(anyhow!(
                "Skill backup is invalid or missing SKILL.md: {}",
                backup_path.display()
            ));
        }

        let app_dir = Self::get_app_skills_dir(app)?;
        fs::create_dir_all(&app_dir)?;
        let dest = app_dir.join(&relative);
        Self::copy_skill_to_new_dest(&source, &dest, &directory)?;

        let mut restored = metadata.skill;
        restored.id = Self::app_scoped_skill_id(app, &restored.id);
        restored.apps = SkillApps::only(app);
        restored.installed_at = Utc::now().timestamp();
        restored.updated_at = 0;
        restored.content_hash = Self::compute_dir_hash(&dest).ok();

        if let Err(error) = db.save_skill(&restored) {
            let _ = Self::remove_path(&dest);
            return Err(error.into());
        }

        Ok(Self::app_skill_from_path(
            app,
            &dest,
            &directory,
            Some(&restored),
        ))
    }

    /// 切换应用启用状态
    ///
    /// 启用：复制到应用目录
    /// 禁用：从应用目录删除
    pub fn toggle_app(db: &Arc<Database>, id: &str, app: &AppType, enabled: bool) -> Result<()> {
        // 获取当前 skill
        let mut skill = db
            .get_installed_skill(id)?
            .ok_or_else(|| anyhow!("Skill not found: {id}"))?;

        // 更新状态
        skill.apps.set_enabled_for(app, enabled);

        // 同步文件
        if enabled {
            Self::sync_to_app_dir(&skill.directory, app)?;
        } else {
            Self::remove_from_app(&skill.directory, app)?;
        }

        // 更新数据库
        db.update_skill_apps(id, &skill.apps)?;

        log::info!("Skill {} 的 {:?} 状态已更新为 {}", skill.name, app, enabled);

        Ok(())
    }

    /// 扫描未管理的 Skills
    ///
    /// 扫描各应用目录，找出未被 Agent Switch 管理的 Skills
    pub fn scan_unmanaged(db: &Arc<Database>) -> Result<Vec<UnmanagedSkill>> {
        let managed_skills = db.get_all_installed_skills()?;
        let managed_dirs: HashSet<String> = managed_skills
            .values()
            .map(|s| s.directory.clone())
            .collect();

        // 收集所有待扫描的目录及其来源标签
        let mut scan_sources: Vec<(PathBuf, String)> = Vec::new();
        for app in AppType::all() {
            if let Ok(d) = Self::get_app_skills_dir(&app) {
                scan_sources.push((d, app.as_str().to_string()));
            }
        }
        if let Some(agents_dir) = get_agents_skills_dir() {
            scan_sources.push((agents_dir, "agents".to_string()));
        }
        if let Ok(ssot_dir) = Self::get_ssot_dir() {
            scan_sources.push((ssot_dir, "agentswitch".to_string()));
        }

        let mut unmanaged: HashMap<String, UnmanagedSkill> = HashMap::new();

        for (scan_dir, label) in &scan_sources {
            let entries = match fs::read_dir(scan_dir) {
                Ok(e) => e,
                Err(_) => continue,
            };
            for entry in entries.flatten() {
                let path = entry.path();
                if !path.is_dir() {
                    continue;
                }
                let dir_name = entry.file_name().to_string_lossy().to_string();
                if dir_name.starts_with('.') || managed_dirs.contains(&dir_name) {
                    continue;
                }

                let skill_md = path.join("SKILL.md");
                if !skill_md.exists() {
                    continue;
                }
                let (name, description) = Self::read_skill_name_desc(&skill_md, &dir_name);

                unmanaged
                    .entry(dir_name.clone())
                    .and_modify(|s| s.found_in.push(label.clone()))
                    .or_insert(UnmanagedSkill {
                        directory: dir_name,
                        name,
                        description,
                        found_in: vec![label.clone()],
                        path: path.display().to_string(),
                    });
            }
        }

        Ok(unmanaged.into_values().collect())
    }

    /// 从应用目录导入 Skills
    ///
    /// 将未管理的 Skills 导入到 Agent Switch 统一管理
    pub fn import_from_apps(
        db: &Arc<Database>,
        imports: Vec<ImportSkillSelection>,
    ) -> Result<Vec<InstalledSkill>> {
        let ssot_dir = Self::get_ssot_dir()?;
        let agents_lock = parse_agents_lock();
        let mut imported = Vec::new();

        // 将 lock 文件中发现的仓库保存到 skill_repos
        save_repos_from_lock(
            db,
            &agents_lock,
            imports.iter().map(|selection| selection.directory.as_str()),
        );

        // 收集所有候选搜索目录
        let mut search_sources: Vec<(PathBuf, String)> = Vec::new();
        for app in AppType::all() {
            if let Ok(d) = Self::get_app_skills_dir(&app) {
                search_sources.push((d, app.as_str().to_string()));
            }
        }
        if let Some(agents_dir) = get_agents_skills_dir() {
            search_sources.push((agents_dir, "agents".to_string()));
        }
        search_sources.push((ssot_dir.clone(), "agentswitch".to_string()));

        for selection in imports {
            let dir_name = selection.directory;
            // 在所有候选目录中查找
            let mut source_path: Option<PathBuf> = None;

            for (base, label) in &search_sources {
                let skill_path = base.join(&dir_name);
                if skill_path.exists() {
                    if source_path.is_none() {
                        source_path = Some(skill_path);
                    }
                    log::debug!("Skill '{dir_name}' found in source '{label}'");
                }
            }

            let source = match source_path {
                Some(p) => p,
                None => continue,
            };
            if !source.join("SKILL.md").exists() {
                log::warn!(
                    "Skip importing '{}' because source '{}' has no SKILL.md",
                    dir_name,
                    source.display()
                );
                continue;
            }

            // 复制到 SSOT
            let dest = ssot_dir.join(&dir_name);
            if !dest.exists() {
                Self::copy_dir_recursive(&source, &dest)?;
            }

            // 解析元数据
            let skill_md = dest.join("SKILL.md");
            let (name, description) = Self::read_skill_name_desc(&skill_md, &dir_name);

            // 启用状态仅信任用户本次显式选择，不再根据“在哪些位置找到”自动推断。
            let apps = selection.apps;

            // 从 lock 文件提取仓库信息
            let (id, repo_owner, repo_name, repo_branch, readme_url) =
                build_repo_info_from_lock(&agents_lock, &dir_name);

            // 计算内容哈希
            let ssot_skill_dir = ssot_dir.join(&dir_name);
            let content_hash = Self::compute_dir_hash(&ssot_skill_dir).ok();

            // 创建记录
            let skill = InstalledSkill {
                id,
                name,
                description,
                directory: dir_name,
                repo_owner,
                repo_name,
                repo_branch,
                readme_url,
                apps,
                installed_at: chrono::Utc::now().timestamp(),
                content_hash,
                updated_at: 0,
            };

            // 保存到数据库
            db.save_skill(&skill)?;

            imported.push(skill);
        }

        log::info!("成功导入 {} 个 Skills", imported.len());

        Ok(imported)
    }

    // ========== 文件同步方法 ==========

    /// 创建符号链接（跨平台）
    ///
    /// - Unix: 使用 std::os::unix::fs::symlink
    /// - Windows: 使用 std::os::windows::fs::symlink_dir
    #[cfg(unix)]
    fn create_symlink(src: &Path, dest: &Path) -> Result<()> {
        std::os::unix::fs::symlink(src, dest)
            .with_context(|| format!("创建符号链接失败: {} -> {}", src.display(), dest.display()))
    }

    #[cfg(windows)]
    fn create_symlink(src: &Path, dest: &Path) -> Result<()> {
        std::os::windows::fs::symlink_dir(src, dest)
            .with_context(|| format!("创建符号链接失败: {} -> {}", src.display(), dest.display()))
    }

    /// 检查路径是否为符号链接
    fn is_symlink(path: &Path) -> bool {
        path.symlink_metadata()
            .map(|m| m.file_type().is_symlink())
            .unwrap_or(false)
    }

    fn resolved_symlink_target(path: &Path) -> Option<PathBuf> {
        if !Self::is_symlink(path) {
            return None;
        }
        let target = fs::read_link(path).ok()?;
        if target.is_absolute() {
            Some(target)
        } else {
            path.parent().map(|parent| parent.join(target))
        }
    }

    fn symlink_points_to(path: &Path, source: &Path) -> bool {
        let Some(target) = Self::resolved_symlink_target(path) else {
            return false;
        };
        match (target.canonicalize(), source.canonicalize()) {
            (Ok(target), Ok(source)) => target == source,
            _ => target == source,
        }
    }

    /// 获取当前同步方式配置
    fn get_sync_method() -> SyncMethod {
        crate::settings::get_skill_sync_method()
    }

    /// 同步 Skill 到应用目录（使用 symlink 或 copy）
    ///
    /// 根据配置和平台选择最佳同步方式：
    /// - Auto: 优先尝试 symlink，失败时回退到 copy
    /// - Symlink: 仅使用 symlink
    /// - Copy: 仅使用文件复制
    pub fn sync_to_app_dir(directory: &str, app: &AppType) -> Result<()> {
        if matches!(app, AppType::ClaudeDesktop) {
            return Ok(());
        }

        let ssot_dir = Self::get_ssot_dir()?;
        let source = ssot_dir.join(directory);

        Self::validate_sync_source_dir(&source, directory)?;

        let app_dir = Self::get_app_skills_dir(app)?;
        fs::create_dir_all(&app_dir)?;

        let dest = app_dir.join(directory);

        let sync_method = Self::get_sync_method();

        match sync_method {
            SyncMethod::Auto => {
                if dest.exists() && !Self::is_symlink(&dest) {
                    Self::replace_dest_with_copy(&source, &dest, directory)?;
                    log::debug!("Skill {directory} 已通过复制同步到 {app:?}");
                    return Ok(());
                }

                if Self::is_symlink(&dest) {
                    Self::remove_path(&dest)?;
                }

                // 优先尝试 symlink
                match Self::create_symlink(&source, &dest) {
                    Ok(()) => {
                        log::debug!("Skill {directory} 已通过 symlink 同步到 {app:?}");
                        return Ok(());
                    }
                    Err(err) => {
                        log::warn!(
                            "Symlink 创建失败，将回退到文件复制: {} -> {}. 错误: {err:#}",
                            source.display(),
                            dest.display()
                        );
                    }
                }
                // Fallback 到 copy
                Self::replace_dest_with_copy(&source, &dest, directory)?;
                log::debug!("Skill {directory} 已通过复制同步到 {app:?}");
            }
            SyncMethod::Symlink => {
                if dest.exists() || Self::is_symlink(&dest) {
                    Self::remove_path(&dest)?;
                }
                Self::create_symlink(&source, &dest)?;
                log::debug!("Skill {directory} 已通过 symlink 同步到 {app:?}");
            }
            SyncMethod::Copy => {
                Self::replace_dest_with_copy(&source, &dest, directory)?;
                log::debug!("Skill {directory} 已通过复制同步到 {app:?}");
            }
        }

        Ok(())
    }

    /// 复制 Skill 到应用目录（保留用于向后兼容）
    #[deprecated(note = "请使用 sync_to_app_dir() 代替")]
    pub fn copy_to_app(directory: &str, app: &AppType) -> Result<()> {
        Self::sync_to_app_dir(directory, app)
    }

    /// 删除路径（支持 symlink 和真实目录）
    fn remove_path(path: &Path) -> Result<()> {
        if Self::is_symlink(path) {
            // 符号链接：仅删除链接本身，不影响源文件
            #[cfg(unix)]
            fs::remove_file(path)?;
            #[cfg(windows)]
            fs::remove_dir(path)?; // Windows 的目录 symlink 需要用 remove_dir
        } else if path.is_dir() {
            // 真实目录：递归删除
            fs::remove_dir_all(path)?;
        } else if path.exists() {
            // 普通文件
            fs::remove_file(path)?;
        }
        Ok(())
    }

    fn validate_sync_source_dir(source: &Path, directory: &str) -> Result<()> {
        if !source.is_dir() {
            return Err(anyhow!("Skill 不存在于 SSOT: {directory}"));
        }

        let manifest = source.join("SKILL.md");
        if !manifest.is_file() {
            return Err(anyhow!(
                "Skill 源目录缺少 SKILL.md，拒绝同步以避免覆盖目标目录: {}",
                source.display()
            ));
        }

        Ok(())
    }

    fn replace_dest_with_copy(source: &Path, dest: &Path, directory: &str) -> Result<()> {
        Self::validate_sync_source_dir(source, directory)?;

        let parent = dest
            .parent()
            .ok_or_else(|| anyhow!("Invalid skill destination: {}", dest.display()))?;
        fs::create_dir_all(parent)?;

        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos();
        let tmp_name = Self::sanitize_backup_segment(directory);
        let tmp = parent.join(format!(".{tmp_name}.tmp-{}-{nonce}", std::process::id()));

        if tmp.exists() || Self::is_symlink(&tmp) {
            Self::remove_path(&tmp)?;
        }

        let copy_result = Self::copy_dir_recursive(source, &tmp);
        if let Err(err) = copy_result {
            let _ = Self::remove_path(&tmp);
            return Err(err);
        }

        if dest.exists() || Self::is_symlink(dest) {
            Self::remove_path(dest)?;
        }

        fs::rename(&tmp, dest).with_context(|| {
            let _ = Self::remove_path(&tmp);
            format!(
                "替换 Skill 目录失败: {} -> {}",
                tmp.display(),
                dest.display()
            )
        })?;

        Ok(())
    }

    fn copy_skill_to_new_dest(source: &Path, dest: &Path, directory: &str) -> Result<()> {
        if !source.is_dir() || !source.join("SKILL.md").is_file() {
            return Err(anyhow!("Skill 源目录缺少 SKILL.md: {}", source.display()));
        }
        if dest.exists() || Self::is_symlink(dest) {
            return Err(anyhow!("Skill already exists: {}", dest.display()));
        }

        let parent = dest
            .parent()
            .ok_or_else(|| anyhow!("Invalid skill destination: {}", dest.display()))?;
        fs::create_dir_all(parent)?;
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos();
        let slug = Self::sanitize_backup_segment(directory);
        let tmp = parent.join(format!(".{slug}.tmp-{}-{nonce}", std::process::id()));

        if tmp.exists() || Self::is_symlink(&tmp) {
            Self::remove_path(&tmp)?;
        }
        if let Err(error) = Self::copy_dir_recursive(source, &tmp) {
            let _ = Self::remove_path(&tmp);
            return Err(error);
        }
        if dest.exists() || Self::is_symlink(dest) {
            let _ = Self::remove_path(&tmp);
            return Err(anyhow!("Skill already exists: {}", dest.display()));
        }
        fs::rename(&tmp, dest).with_context(|| {
            let _ = Self::remove_path(&tmp);
            format!("安装 Skill 失败: {} -> {}", tmp.display(), dest.display())
        })?;
        Ok(())
    }

    /// 判断路径是否为指向 SSOT 目录内的符号链接。
    fn is_symlink_to_ssot(path: &Path, ssot_dir: &Path) -> bool {
        if !Self::is_symlink(path) {
            return false;
        }

        let Ok(target) = fs::read_link(path) else {
            return false;
        };

        if target.is_absolute() && target.starts_with(ssot_dir) {
            return true;
        }

        let resolved = path
            .parent()
            .map(|parent| parent.join(&target))
            .unwrap_or(target.clone());

        let canonical_ssot = ssot_dir
            .canonicalize()
            .unwrap_or_else(|_| ssot_dir.to_path_buf());
        let canonical_target = resolved.canonicalize().unwrap_or(resolved);

        canonical_target.starts_with(&canonical_ssot)
    }

    /// 从应用目录删除 Skill（支持 symlink 和真实目录）
    pub fn remove_from_app(directory: &str, app: &AppType) -> Result<()> {
        if matches!(app, AppType::ClaudeDesktop) {
            return Ok(());
        }

        let app_dir = Self::get_app_skills_dir(app)?;
        let skill_path = app_dir.join(directory);

        if skill_path.exists() || Self::is_symlink(&skill_path) {
            Self::remove_path(&skill_path)?;
            log::debug!("Skill {directory} 已从 {app:?} 删除");
        }

        Ok(())
    }

    /// 同步所有已启用的 Skills 到指定应用
    pub fn sync_to_app(db: &Arc<Database>, app: &AppType) -> Result<()> {
        if matches!(app, AppType::ClaudeDesktop) {
            return Ok(());
        }

        let skills = db.get_all_installed_skills()?;
        let ssot_dir = Self::get_ssot_dir()?;
        let app_dir = Self::get_app_skills_dir(app)?;

        let indexed_skills: HashMap<String, &InstalledSkill> = skills
            .values()
            .map(|skill| (skill.directory.to_lowercase(), skill))
            .collect();

        if app_dir.exists() {
            for entry in fs::read_dir(&app_dir)? {
                let entry = entry?;
                let path = entry.path();
                let dir_name = entry.file_name().to_string_lossy().to_string();

                if dir_name.starts_with('.') {
                    continue;
                }

                if let Some(skill) = indexed_skills.get(&dir_name.to_lowercase()) {
                    if !skill.apps.is_enabled_for(app) {
                        Self::remove_path(&path)?;
                    }
                    continue;
                }

                if Self::is_symlink_to_ssot(&path, &ssot_dir) {
                    Self::remove_path(&path)?;
                }
            }
        }

        for skill in skills.values() {
            if skill.apps.is_enabled_for(app) {
                Self::sync_to_app_dir(&skill.directory, app)?;
            }
        }

        Ok(())
    }

    // ========== 发现功能（保留原有逻辑）==========

    /// 列出所有可发现的技能（从仓库获取）
    pub async fn discover_available(
        &self,
        repos: Vec<SkillRepo>,
    ) -> Result<Vec<DiscoverableSkill>> {
        let mut skills = Vec::new();

        // 仅使用启用的仓库
        let enabled_repos: Vec<SkillRepo> = repos.into_iter().filter(|repo| repo.enabled).collect();

        let fetch_tasks = enabled_repos
            .iter()
            .map(|repo| self.fetch_repo_skills(repo));

        let results: Vec<Result<Vec<DiscoverableSkill>>> =
            futures::future::join_all(fetch_tasks).await;

        for (repo, result) in enabled_repos.into_iter().zip(results) {
            match result {
                Ok(repo_skills) => skills.extend(repo_skills),
                Err(e) => log::warn!("获取仓库 {}/{} 技能失败: {}", repo.owner, repo.name, e),
            }
        }

        // 去重并排序
        Self::deduplicate_discoverable_skills(&mut skills);
        skills.sort_by_key(|skill| skill.name.to_lowercase());

        Ok(skills)
    }

    /// 列出所有技能（兼容旧 API）
    pub async fn list_skills(
        &self,
        repos: Vec<SkillRepo>,
        db: &Arc<Database>,
    ) -> Result<Vec<Skill>> {
        // 获取可发现的技能
        let discoverable = self.discover_available(repos).await?;

        // 获取已安装的技能
        let installed = db.get_all_installed_skills()?;
        let installed_dirs: HashSet<String> =
            installed.values().map(|s| s.directory.clone()).collect();

        // 转换为 Skill 格式
        let mut skills: Vec<Skill> = discoverable
            .into_iter()
            .map(|d| {
                let install_name = Path::new(&d.directory)
                    .file_name()
                    .map(|s| s.to_string_lossy().to_string())
                    .unwrap_or_else(|| d.directory.clone());

                Skill {
                    key: d.key,
                    name: d.name,
                    description: d.description,
                    directory: d.directory,
                    readme_url: d.readme_url,
                    installed: installed_dirs.contains(&install_name),
                    repo_owner: Some(d.repo_owner),
                    repo_name: Some(d.repo_name),
                    repo_branch: Some(d.repo_branch),
                }
            })
            .collect();

        // 添加本地已安装但不在仓库中的技能
        for skill in installed.values() {
            let already_in_list = skills.iter().any(|s| {
                let s_install_name = Path::new(&s.directory)
                    .file_name()
                    .map(|n| n.to_string_lossy().to_string())
                    .unwrap_or_else(|| s.directory.clone());
                s_install_name == skill.directory
            });

            if !already_in_list {
                skills.push(Skill {
                    key: skill.id.clone(),
                    name: skill.name.clone(),
                    description: skill.description.clone().unwrap_or_default(),
                    directory: skill.directory.clone(),
                    readme_url: skill.readme_url.clone(),
                    installed: true,
                    repo_owner: skill.repo_owner.clone(),
                    repo_name: skill.repo_name.clone(),
                    repo_branch: skill.repo_branch.clone(),
                });
            }
        }

        skills.sort_by_key(|skill| skill.name.to_lowercase());

        Ok(skills)
    }

    /// 从仓库获取技能列表
    async fn fetch_repo_skills(&self, repo: &SkillRepo) -> Result<Vec<DiscoverableSkill>> {
        let (temp_dir, resolved_branch) =
            timeout(std::time::Duration::from_secs(60), self.download_repo(repo))
                .await
                .map_err(|_| {
                    anyhow!(format_skill_error(
                        "DOWNLOAD_TIMEOUT",
                        &[
                            ("owner", &repo.owner),
                            ("name", &repo.name),
                            ("timeout", "60")
                        ],
                        Some("checkNetwork"),
                    ))
                })??;

        let mut skills = Vec::new();
        let scan_dir = temp_dir.clone();
        let mut resolved_repo = repo.clone();
        resolved_repo.branch = resolved_branch;
        self.scan_dir_recursive(&scan_dir, &scan_dir, &resolved_repo, &mut skills)?;

        let _ = fs::remove_dir_all(&temp_dir);

        Ok(skills)
    }

    /// 递归扫描目录查找 SKILL.md
    fn scan_dir_recursive(
        &self,
        current_dir: &Path,
        base_dir: &Path,
        repo: &SkillRepo,
        skills: &mut Vec<DiscoverableSkill>,
    ) -> Result<()> {
        let skill_md = current_dir.join("SKILL.md");

        if skill_md.exists() {
            let directory = if current_dir == base_dir {
                repo.name.clone()
            } else {
                current_dir
                    .strip_prefix(base_dir)
                    .unwrap_or(current_dir)
                    .to_string_lossy()
                    .replace('\\', "/")
            };

            let doc_path = skill_md
                .strip_prefix(base_dir)
                .unwrap_or(skill_md.as_path())
                .to_string_lossy()
                .replace('\\', "/");

            if let Ok(skill) =
                self.build_skill_from_metadata(&skill_md, &directory, &doc_path, repo)
            {
                skills.push(skill);
            }

            return Ok(());
        }

        for entry in fs::read_dir(current_dir)? {
            let entry = entry?;
            let path = entry.path();

            if path.is_dir() {
                self.scan_dir_recursive(&path, base_dir, repo, skills)?;
            }
        }

        Ok(())
    }

    /// 从 SKILL.md 构建技能对象
    fn build_skill_from_metadata(
        &self,
        skill_md: &Path,
        directory: &str,
        doc_path: &str,
        repo: &SkillRepo,
    ) -> Result<DiscoverableSkill> {
        let meta = self.parse_skill_metadata(skill_md)?;

        Ok(DiscoverableSkill {
            key: format!("{}/{}:{}", repo.owner, repo.name, directory),
            name: meta.name.unwrap_or_else(|| directory.to_string()),
            description: meta.description.unwrap_or_default(),
            directory: directory.to_string(),
            readme_url: Some(Self::build_skill_doc_url(
                &repo.owner,
                &repo.name,
                &repo.branch,
                doc_path,
            )),
            repo_owner: repo.owner.clone(),
            repo_name: repo.name.clone(),
            repo_branch: repo.branch.clone(),
        })
    }

    /// 解析技能元数据
    fn parse_skill_metadata(&self, path: &Path) -> Result<SkillMetadata> {
        Self::parse_skill_metadata_static(path)
    }

    /// 静态方法：解析技能元数据
    fn parse_skill_metadata_static(path: &Path) -> Result<SkillMetadata> {
        let content = fs::read_to_string(path)?;
        let content = content.trim_start_matches('\u{feff}');

        let parts: Vec<&str> = content.splitn(3, "---").collect();
        if parts.len() < 3 {
            return Ok(SkillMetadata {
                name: None,
                description: None,
            });
        }

        let front_matter = parts[1].trim();
        let meta: SkillMetadata = serde_yaml::from_str(front_matter).unwrap_or(SkillMetadata {
            name: None,
            description: None,
        });

        Ok(meta)
    }

    /// 从 SKILL.md 读取名称和描述，不存在则用目录名兜底
    fn read_skill_name_desc(skill_md: &Path, fallback_name: &str) -> (String, Option<String>) {
        if skill_md.exists() {
            match Self::parse_skill_metadata_static(skill_md) {
                Ok(meta) => (
                    meta.name.unwrap_or_else(|| fallback_name.to_string()),
                    meta.description,
                ),
                Err(_) => (fallback_name.to_string(), None),
            }
        } else {
            (fallback_name.to_string(), None)
        }
    }

    /// 校验并规范化技能源路径（允许多级目录），拒绝路径穿越和绝对路径
    fn sanitize_skill_source_path(raw: &str) -> Option<PathBuf> {
        let trimmed = raw.trim();
        if trimmed.is_empty() {
            return None;
        }

        let mut normalized = PathBuf::new();
        let mut has_component = false;

        for component in Path::new(trimmed).components() {
            match component {
                Component::Normal(name) => {
                    let segment = name.to_string_lossy().trim().to_string();
                    if segment.is_empty() || segment == "." || segment == ".." {
                        return None;
                    }
                    normalized.push(segment);
                    has_component = true;
                }
                Component::CurDir
                | Component::ParentDir
                | Component::RootDir
                | Component::Prefix(_) => {
                    return None;
                }
            }
        }

        has_component.then_some(normalized)
    }

    /// 校验并规范化安装目录名（最终落盘目录名，仅单段）
    fn sanitize_install_name(raw: &str) -> Option<String> {
        let trimmed = raw.trim();
        if trimmed.is_empty() {
            return None;
        }

        let path = Path::new(trimmed);
        let mut components = path.components();
        match (components.next(), components.next()) {
            (Some(Component::Normal(name)), None) => {
                let normalized = name.to_string_lossy().trim().to_string();
                if normalized.is_empty()
                    || normalized == "."
                    || normalized == ".."
                    || normalized.starts_with('.')
                {
                    None
                } else {
                    Some(normalized)
                }
            }
            _ => None,
        }
    }

    /// 在目录树中查找名称匹配且包含 SKILL.md 的子目录
    ///
    /// 用于 skills.sh 安装回退：API 只返回 skillId（如 "find-skills"），
    /// 但实际文件可能在仓库子目录中（如 "skills/find-skills"）。
    fn find_skill_dir_by_name(root: &Path, target_name: &str) -> Option<PathBuf> {
        fn walk(dir: &Path, target: &str, depth: usize) -> Option<PathBuf> {
            if depth > 3 {
                return None;
            }
            let entries = fs::read_dir(dir).ok()?;
            for entry in entries.flatten() {
                let path = entry.path();
                if !path.is_dir() {
                    continue;
                }
                let name = entry.file_name();
                let name_str = name.to_string_lossy();
                if name_str.starts_with('.') {
                    continue;
                }
                if name_str.eq_ignore_ascii_case(target) && path.join("SKILL.md").exists() {
                    return Some(path);
                }
                if let Some(found) = walk(&path, target, depth + 1) {
                    return Some(found);
                }
            }
            None
        }
        walk(root, target_name, 0)
    }

    /// 将 discoverable skill 的目录信息重新解析为解压目录中的真实源目录。
    ///
    /// 兼容三种情况：
    /// 1. `skills/foo` 这类直接相对路径；
    /// 2. 仅持有安装名 `foo`，需要在仓库中递归查找真实目录；
    /// 3. 仓库根目录本身就是 skill，此时回退到解压根目录。
    fn resolve_skill_source_dir(root: &Path, raw_directory: &str) -> Option<PathBuf> {
        let source_rel = Self::sanitize_skill_source_path(raw_directory)?;
        let direct = root.join(&source_rel);
        if direct.is_dir() {
            return Some(direct);
        }

        let target_name = source_rel.file_name()?.to_string_lossy().to_string();
        if let Some(found) = Self::find_skill_dir_by_name(root, &target_name) {
            log::info!(
                "Skill directory '{}' not found at direct path, using fallback: {}",
                target_name,
                found.display()
            );
            return Some(found);
        }

        if root.is_dir() && root.join("SKILL.md").exists() {
            log::info!(
                "Skill directory '{}' not found, but SKILL.md exists at root, using repo root",
                target_name,
            );
            return Some(root.to_path_buf());
        }

        None
    }

    /// 去重技能列表（基于完整 key，不同仓库的同名 skill 分开显示）
    fn deduplicate_discoverable_skills(skills: &mut Vec<DiscoverableSkill>) {
        let mut seen = HashMap::new();
        skills.retain(|skill| {
            // 使用完整 key（owner/repo:directory）作为唯一标识
            // 这样不同仓库的同名 skill 会分开显示
            let unique_key = skill.key.to_lowercase();
            if let std::collections::hash_map::Entry::Vacant(e) = seen.entry(unique_key) {
                e.insert(true);
                true
            } else {
                false
            }
        });
    }

    /// 下载仓库
    async fn download_repo(&self, repo: &SkillRepo) -> Result<(PathBuf, String)> {
        let temp_dir = tempfile::tempdir()?;
        let temp_path = temp_dir.path().to_path_buf();
        let _ = temp_dir.keep();

        let mut branches = Vec::new();
        if !repo.branch.is_empty() && !repo.branch.eq_ignore_ascii_case("HEAD") {
            branches.push(repo.branch.as_str());
        }
        if !branches.contains(&"main") {
            branches.push("main");
        }
        if !branches.contains(&"master") {
            branches.push("master");
        }

        let mut last_error = None;
        for branch in branches {
            let url = format!(
                "https://github.com/{}/{}/archive/refs/heads/{}.zip",
                repo.owner, repo.name, branch
            );

            match self.download_and_extract(&url, &temp_path).await {
                Ok(_) => {
                    return Ok((temp_path, branch.to_string()));
                }
                Err(e) => {
                    last_error = Some(e);
                    continue;
                }
            }
        }

        Err(last_error.unwrap_or_else(|| anyhow::anyhow!("所有分支下载失败")))
    }

    /// 下载并解压 ZIP
    async fn download_and_extract(&self, url: &str, dest: &Path) -> Result<()> {
        let client = crate::proxy::http_client::get();
        let response = client.get(url).send().await?;
        if !response.status().is_success() {
            let status = response.status().as_u16().to_string();
            return Err(anyhow::anyhow!(format_skill_error(
                "DOWNLOAD_FAILED",
                &[("status", &status)],
                match status.as_str() {
                    "403" => Some("http403"),
                    "404" => Some("http404"),
                    "429" => Some("http429"),
                    _ => Some("checkNetwork"),
                },
            )));
        }

        let bytes = response.bytes().await?;
        let cursor = std::io::Cursor::new(bytes);
        let mut archive = zip::ZipArchive::new(cursor)?;

        let root_name = if !archive.is_empty() {
            let first_file = archive.by_index(0)?;
            let name = first_file.name();
            name.split('/').next().unwrap_or("").to_string()
        } else {
            return Err(anyhow::anyhow!(format_skill_error(
                "EMPTY_ARCHIVE",
                &[],
                Some("checkRepoUrl"),
            )));
        };

        // 第一遍：解压普通文件和目录，收集 symlink 条目
        let mut symlinks: Vec<(PathBuf, String)> = Vec::new();

        for i in 0..archive.len() {
            let mut file = archive.by_index(i)?;
            let file_path = file.name().to_string();

            let relative_path =
                if let Some(stripped) = file_path.strip_prefix(&format!("{root_name}/")) {
                    stripped
                } else {
                    continue;
                };

            if relative_path.is_empty() {
                continue;
            }

            let outpath = dest.join(relative_path);

            if file.is_symlink() {
                // 读取 symlink 目标路径
                let mut target = String::new();
                std::io::Read::read_to_string(&mut file, &mut target)?;
                symlinks.push((outpath, target.trim().to_string()));
            } else if file.is_dir() {
                fs::create_dir_all(&outpath)?;
            } else {
                if let Some(parent) = outpath.parent() {
                    fs::create_dir_all(parent)?;
                }
                let mut outfile = fs::File::create(&outpath)?;
                std::io::copy(&mut file, &mut outfile)?;
            }
        }

        // 第二遍：解析 symlink，将目标内容复制到 symlink 位置
        Self::resolve_symlinks_in_dir(dest, &symlinks)?;

        Ok(())
    }

    /// 递归复制目录
    fn copy_dir_recursive(src: &Path, dest: &Path) -> Result<()> {
        fs::create_dir_all(dest)?;

        for entry in fs::read_dir(src)? {
            let entry = entry?;
            let path = entry.path();
            let dest_path = dest.join(entry.file_name());

            if path.is_dir() {
                Self::copy_dir_recursive(&path, &dest_path)?;
            } else {
                fs::copy(&path, &dest_path)?;
            }
        }

        Ok(())
    }

    fn resolve_uninstall_backup_source(skill: &InstalledSkill) -> Result<Option<PathBuf>> {
        let ssot_path = Self::get_ssot_dir()?.join(&skill.directory);
        if ssot_path.is_dir() {
            return Ok(Some(ssot_path));
        }

        for app in AppType::all() {
            let app_dir = match Self::get_app_skills_dir(&app) {
                Ok(dir) => dir,
                Err(_) => continue,
            };
            let candidate = app_dir.join(&skill.directory);
            if candidate.is_dir() {
                return Ok(Some(candidate));
            }
        }

        Ok(None)
    }

    fn sanitize_backup_segment(segment: &str) -> String {
        let sanitized = segment
            .chars()
            .map(|c| match c {
                'a'..='z' | 'A'..='Z' | '0'..='9' | '-' | '_' | '.' => c,
                _ => '-',
            })
            .collect::<String>()
            .trim_matches('-')
            .to_string();

        if sanitized.is_empty() {
            "skill".to_string()
        } else {
            sanitized
        }
    }

    fn cleanup_old_skill_backups(dir: &Path) -> Result<()> {
        let mut entries = fs::read_dir(dir)?
            .filter_map(|entry| entry.ok())
            .filter_map(|entry| {
                let metadata = entry.metadata().ok()?;
                if !metadata.is_dir() {
                    return None;
                }
                Some((entry.path(), metadata.modified().ok()))
            })
            .collect::<Vec<_>>();

        if entries.len() <= SKILL_BACKUP_RETAIN_COUNT {
            return Ok(());
        }

        entries.sort_by_key(|(_, modified)| *modified);
        let remove_count = entries.len().saturating_sub(SKILL_BACKUP_RETAIN_COUNT);

        for (path, _) in entries.into_iter().take(remove_count) {
            fs::remove_dir_all(&path)?;
        }

        Ok(())
    }

    fn backup_path_for_id(backup_id: &str) -> Result<PathBuf> {
        if backup_id.contains("..")
            || backup_id.contains('/')
            || backup_id.contains('\\')
            || backup_id.trim().is_empty()
        {
            return Err(anyhow!("Invalid backup id: {backup_id}"));
        }

        Ok(Self::get_backup_dir()?.join(backup_id))
    }

    fn read_backup_metadata(backup_path: &Path) -> Result<SkillBackupMetadata> {
        let metadata_path = backup_path.join("meta.json");
        let content = fs::read_to_string(&metadata_path)
            .with_context(|| format!("failed to read {}", metadata_path.display()))?;
        serde_json::from_str(&content)
            .with_context(|| format!("failed to parse {}", metadata_path.display()))
    }

    fn create_uninstall_backup(skill: &InstalledSkill) -> Result<Option<PathBuf>> {
        let Some(source_path) = Self::resolve_uninstall_backup_source(skill)? else {
            log::warn!(
                "Skill {} 卸载前未找到可备份的目录，将跳过备份",
                skill.directory
            );
            return Ok(None);
        };

        Self::create_backup_from_source(skill, &source_path, None)
    }

    fn create_backup_from_source(
        skill: &InstalledSkill,
        source_path: &Path,
        source_app: Option<AppType>,
    ) -> Result<Option<PathBuf>> {
        if !source_path.is_dir() || !source_path.join("SKILL.md").is_file() {
            return Ok(None);
        }

        let backup_root = Self::get_backup_dir()?;
        let timestamp = Utc::now().format("%Y%m%d_%H%M%S");
        let slug = Self::sanitize_backup_segment(&skill.directory);
        let mut backup_path = backup_root.join(format!("{timestamp}_{slug}"));
        let mut counter = 1;
        while backup_path.exists() {
            backup_path = backup_root.join(format!("{timestamp}_{slug}_{counter}"));
            counter += 1;
        }

        let write_backup = || -> Result<()> {
            let skill_backup_dir = backup_path.join("skill");
            Self::copy_dir_recursive(&source_path, &skill_backup_dir)?;

            let metadata = SkillBackupMetadata {
                skill: skill.clone(),
                backup_created_at: Utc::now().timestamp(),
                source_path: source_path.to_string_lossy().to_string(),
                source_app,
            };
            let metadata_path = backup_path.join("meta.json");
            let metadata_json = serde_json::to_string_pretty(&metadata)
                .context("failed to serialize skill backup metadata")?;
            fs::write(&metadata_path, metadata_json)
                .with_context(|| format!("failed to write {}", metadata_path.display()))?;
            Ok(())
        };

        if let Err(err) = write_backup() {
            let _ = fs::remove_dir_all(&backup_path);
            return Err(err);
        }

        if let Err(err) = Self::cleanup_old_skill_backups(&backup_root) {
            log::warn!("清理旧 Skill 备份失败: {err:#}");
        }

        log::info!(
            "Skill {} 已在卸载前备份到 {}",
            skill.name,
            backup_path.display()
        );

        Ok(Some(backup_path))
    }

    /// 解析 ZIP 中的符号链接：将目标内容复制到 symlink 位置
    ///
    /// GitHub ZIP 归档保留了 symlink 元数据，解压时可通过 `is_symlink()` 检测。
    /// 此方法将 symlink 解析为实际文件/目录内容（而非创建真实 symlink），
    /// 以确保跨平台兼容且 skill 内容自包含。
    fn resolve_symlinks_in_dir(base_dir: &Path, symlinks: &[(PathBuf, String)]) -> Result<()> {
        // 规范化 base_dir（macOS 上 /tmp → /private/tmp，需保持一致）
        let canonical_base = base_dir
            .canonicalize()
            .unwrap_or_else(|_| base_dir.to_path_buf());

        for (link_path, target) in symlinks {
            // 计算 symlink 的父目录，然后拼接目标的相对路径
            let parent = link_path.parent().unwrap_or(base_dir);
            let resolved = parent.join(target);

            // 规范化路径（解析 .. 等）
            let resolved = match resolved.canonicalize() {
                Ok(p) => p,
                Err(_) => {
                    log::warn!(
                        "Symlink 目标不存在，跳过: {} -> {}",
                        link_path.display(),
                        target
                    );
                    continue;
                }
            };

            // 安全检查：确保目标在 base_dir 内（防止路径穿越）
            if !resolved.starts_with(&canonical_base) {
                log::warn!(
                    "Symlink 目标超出仓库范围，跳过: {} -> {}",
                    link_path.display(),
                    resolved.display()
                );
                continue;
            }

            // 复制目标内容到 symlink 位置
            if resolved.is_dir() {
                Self::copy_dir_recursive(&resolved, link_path)?;
            } else if resolved.is_file() {
                if let Some(parent) = link_path.parent() {
                    fs::create_dir_all(parent)?;
                }
                fs::copy(&resolved, link_path)?;
            }
        }
        Ok(())
    }

    // ========== 从 ZIP 文件安装 ==========

    /// 从 ZIP 直接安装到指定 CLI 的原生 Skills 目录。
    pub fn install_from_zip_for_app(
        db: &Arc<Database>,
        zip_path: &Path,
        app: &AppType,
    ) -> Result<Vec<AppSkill>> {
        Self::ensure_app_skill_support(app)?;
        if Self::app_primary_skills_dir_is_global(app) {
            let installed = Self::install_from_zip_global(db, zip_path)?;
            let global_dir = Self::get_global_skills_dir()?;
            let records = db.get_all_installed_skills()?;
            return Ok(installed
                .into_iter()
                .map(|skill| {
                    let metadata = Self::find_global_record(records.values(), &skill.directory);
                    Self::app_skill_from_path(
                        app,
                        &global_dir.join(&skill.directory),
                        &skill.directory,
                        metadata,
                    )
                })
                .collect());
        }
        let temp_dir = Self::extract_local_zip(zip_path)?;
        let result = (|| -> Result<Vec<AppSkill>> {
            let skill_dirs = Self::scan_skills_in_dir(&temp_dir)?;
            if skill_dirs.is_empty() {
                return Err(anyhow!(format_skill_error(
                    "NO_SKILLS_IN_ZIP",
                    &[],
                    Some("checkZipContent"),
                )));
            }

            let app_dir = Self::get_app_skills_dir(app)?;
            fs::create_dir_all(&app_dir)?;
            let zip_stem = zip_path
                .file_stem()
                .and_then(|value| value.to_str())
                .map(str::to_string);
            let mut installed = Vec::new();

            for skill_dir in skill_dirs {
                let skill_md = skill_dir.join("SKILL.md");
                let metadata = Self::parse_skill_metadata_static(&skill_md).ok();
                let raw_directory = skill_dir
                    .file_name()
                    .map(|value| value.to_string_lossy().to_string())
                    .unwrap_or_default();
                let install_name = if skill_dir == temp_dir || raw_directory.starts_with('.') {
                    metadata
                        .as_ref()
                        .and_then(|value| value.name.as_deref())
                        .and_then(Self::sanitize_install_name)
                        .or_else(|| zip_stem.as_deref().and_then(Self::sanitize_install_name))
                } else {
                    Self::sanitize_install_name(&raw_directory)
                        .or_else(|| {
                            metadata
                                .as_ref()
                                .and_then(|value| value.name.as_deref())
                                .and_then(Self::sanitize_install_name)
                        })
                        .or_else(|| zip_stem.as_deref().and_then(Self::sanitize_install_name))
                }
                .ok_or_else(|| {
                    anyhow!(format_skill_error(
                        "INVALID_SKILL_DIRECTORY",
                        &[("zip", &zip_path.display().to_string())],
                        Some("checkZipContent"),
                    ))
                })?;

                let dest = app_dir.join(&install_name);
                if dest.exists() || Self::is_symlink(&dest) {
                    log::warn!(
                        "Skill directory '{}' already exists in {}, skipping",
                        install_name,
                        app.as_str()
                    );
                    continue;
                }

                Self::copy_skill_to_new_dest(&skill_dir, &dest, &install_name)?;
                let (name, description) =
                    Self::read_skill_name_desc(&dest.join("SKILL.md"), &install_name);
                let record = InstalledSkill {
                    id: format!("{}:local:{install_name}", app.as_str()),
                    name,
                    description,
                    directory: install_name.clone(),
                    repo_owner: None,
                    repo_name: None,
                    repo_branch: None,
                    readme_url: None,
                    apps: SkillApps::only(app),
                    installed_at: Utc::now().timestamp(),
                    content_hash: Self::compute_dir_hash(&dest).ok(),
                    updated_at: 0,
                };
                if let Err(error) = db.save_skill(&record) {
                    let _ = Self::remove_path(&dest);
                    return Err(error.into());
                }
                installed.push(Self::app_skill_from_path(
                    app,
                    &dest,
                    &install_name,
                    Some(&record),
                ));
            }

            Ok(installed)
        })();
        let _ = fs::remove_dir_all(&temp_dir);
        result
    }

    /// 将 ZIP 中的 Skills 安装到全局目录；不会自动创建额外的 CLI 链接。
    pub fn install_from_zip_global(
        db: &Arc<Database>,
        zip_path: &Path,
    ) -> Result<Vec<GlobalSkill>> {
        let temp_dir = Self::extract_local_zip(zip_path)?;
        let result = (|| -> Result<Vec<GlobalSkill>> {
            let skill_dirs = Self::scan_skills_in_dir(&temp_dir)?;
            if skill_dirs.is_empty() {
                return Err(anyhow!(format_skill_error(
                    "NO_SKILLS_IN_ZIP",
                    &[],
                    Some("checkZipContent"),
                )));
            }

            let global_dir = Self::get_global_skills_dir()?;
            let zip_stem = zip_path
                .file_stem()
                .and_then(|value| value.to_str())
                .map(str::to_string);
            let mut installed = Vec::new();

            for skill_dir in skill_dirs {
                let skill_md = skill_dir.join("SKILL.md");
                let metadata = Self::parse_skill_metadata_static(&skill_md).ok();
                let raw_directory = skill_dir
                    .file_name()
                    .map(|value| value.to_string_lossy().to_string())
                    .unwrap_or_default();
                let install_name = if skill_dir == temp_dir || raw_directory.starts_with('.') {
                    metadata
                        .as_ref()
                        .and_then(|value| value.name.as_deref())
                        .and_then(Self::sanitize_install_name)
                        .or_else(|| zip_stem.as_deref().and_then(Self::sanitize_install_name))
                } else {
                    Self::sanitize_install_name(&raw_directory)
                        .or_else(|| {
                            metadata
                                .as_ref()
                                .and_then(|value| value.name.as_deref())
                                .and_then(Self::sanitize_install_name)
                        })
                        .or_else(|| zip_stem.as_deref().and_then(Self::sanitize_install_name))
                }
                .ok_or_else(|| {
                    anyhow!(format_skill_error(
                        "INVALID_SKILL_DIRECTORY",
                        &[("zip", &zip_path.display().to_string())],
                        Some("checkZipContent"),
                    ))
                })?;

                let dest = global_dir.join(&install_name);
                if dest.exists() || Self::is_symlink(&dest) {
                    log::warn!(
                        "Skill directory '{}' already exists in global library, skipping",
                        install_name
                    );
                    continue;
                }

                Self::copy_skill_to_new_dest(&skill_dir, &dest, &install_name)?;
                let (name, description) =
                    Self::read_skill_name_desc(&dest.join("SKILL.md"), &install_name);
                let record = InstalledSkill {
                    id: format!("global:local:{install_name}"),
                    name,
                    description,
                    directory: install_name.clone(),
                    repo_owner: None,
                    repo_name: None,
                    repo_branch: None,
                    readme_url: None,
                    apps: Self::global_link_states(&dest, &install_name),
                    installed_at: Utc::now().timestamp(),
                    content_hash: Self::compute_dir_hash(&dest).ok(),
                    updated_at: 0,
                };
                if let Err(error) = db.save_skill(&record) {
                    let _ = Self::remove_path(&dest);
                    return Err(error.into());
                }
                installed.push(Self::global_skill_from_path(
                    &dest,
                    &install_name,
                    Some(&record),
                ));
            }

            Ok(installed)
        })();
        let _ = fs::remove_dir_all(&temp_dir);
        result
    }

    /// 从本地 ZIP 文件安装 Skills
    ///
    /// 流程：
    /// 1. 解压 ZIP 到临时目录
    /// 2. 扫描目录查找包含 SKILL.md 的技能
    /// 3. 复制到 SSOT 并保存到数据库
    /// 4. 同步到当前应用目录
    pub fn install_from_zip(
        db: &Arc<Database>,
        zip_path: &Path,
        current_app: &AppType,
    ) -> Result<Vec<InstalledSkill>> {
        // 解压到临时目录
        let temp_dir = Self::extract_local_zip(zip_path)?;

        // 扫描所有包含 SKILL.md 的目录
        let skill_dirs = Self::scan_skills_in_dir(&temp_dir)?;

        if skill_dirs.is_empty() {
            let _ = fs::remove_dir_all(&temp_dir);
            return Err(anyhow!(format_skill_error(
                "NO_SKILLS_IN_ZIP",
                &[],
                Some("checkZipContent"),
            )));
        }

        let ssot_dir = Self::get_ssot_dir()?;
        let mut installed = Vec::new();
        let existing_skills = db.get_all_installed_skills()?;
        let zip_stem = zip_path
            .file_stem()
            .and_then(|s| s.to_str())
            .map(|s| s.to_string());

        for skill_dir in skill_dirs {
            // 解析元数据（提前解析，用于确定安装名）
            let skill_md = skill_dir.join("SKILL.md");
            let meta = if skill_md.exists() {
                Self::parse_skill_metadata_static(&skill_md).ok()
            } else {
                None
            };

            // 获取目录名称作为安装名
            // 当 SKILL.md 在 ZIP 根目录时，skill_dir == temp_dir，
            // file_name() 会返回临时目录名（如 .tmpDZKGpF），需要回退到其他来源
            let install_name = {
                let dir_name = skill_dir
                    .file_name()
                    .map(|s| s.to_string_lossy().to_string())
                    .unwrap_or_default();

                if skill_dir == temp_dir || dir_name.is_empty() || dir_name.starts_with('.') {
                    // SKILL.md 在根目录：优先用元数据 name，否则用 ZIP 文件名
                    meta.as_ref()
                        .and_then(|m| m.name.as_deref())
                        .and_then(Self::sanitize_install_name)
                        .or_else(|| zip_stem.as_deref().and_then(Self::sanitize_install_name))
                } else {
                    Self::sanitize_install_name(&dir_name)
                        .or_else(|| {
                            meta.as_ref()
                                .and_then(|m| m.name.as_deref())
                                .and_then(Self::sanitize_install_name)
                        })
                        .or_else(|| zip_stem.as_deref().and_then(Self::sanitize_install_name))
                }
            };
            let install_name = match install_name {
                Some(name) => name,
                None => {
                    let _ = fs::remove_dir_all(&temp_dir);
                    return Err(anyhow!(format_skill_error(
                        "INVALID_SKILL_DIRECTORY",
                        &[("zip", &zip_path.display().to_string())],
                        Some("checkZipContent"),
                    )));
                }
            };

            // 检查是否已有同名 directory 的 skill
            let conflict = existing_skills
                .values()
                .find(|s| s.directory.eq_ignore_ascii_case(&install_name));

            if let Some(existing) = conflict {
                log::warn!(
                    "Skill directory '{}' already exists (from {}), skipping",
                    install_name,
                    existing.id
                );
                continue;
            }

            let (name, description) = match meta {
                Some(m) => (
                    m.name.unwrap_or_else(|| install_name.clone()),
                    m.description,
                ),
                None => (install_name.clone(), None),
            };

            // 复制到 SSOT
            let dest = ssot_dir.join(&install_name);
            if dest.exists() {
                let _ = fs::remove_dir_all(&dest);
            }
            Self::copy_dir_recursive(&skill_dir, &dest)?;

            // 计算内容哈希
            let content_hash = Self::compute_dir_hash(&dest).ok();

            // 创建 InstalledSkill 记录
            let skill = InstalledSkill {
                id: format!("local:{install_name}"),
                name,
                description,
                directory: install_name.clone(),
                repo_owner: None,
                repo_name: None,
                repo_branch: None,
                readme_url: None,
                apps: SkillApps::only(current_app),
                installed_at: chrono::Utc::now().timestamp(),
                content_hash,
                updated_at: 0,
            };

            // 保存到数据库
            db.save_skill(&skill)?;

            // 同步到当前应用目录
            Self::sync_to_app_dir(&install_name, current_app)?;

            log::info!(
                "Skill {} installed from ZIP, enabled for {:?}",
                skill.name,
                current_app
            );
            installed.push(skill);
        }

        // 清理临时目录
        let _ = fs::remove_dir_all(&temp_dir);

        Ok(installed)
    }

    /// 解压本地 ZIP 文件到临时目录
    fn extract_local_zip(zip_path: &Path) -> Result<PathBuf> {
        let file = fs::File::open(zip_path)
            .with_context(|| format!("Failed to open ZIP file: {}", zip_path.display()))?;

        let mut archive = zip::ZipArchive::new(file)
            .with_context(|| format!("Failed to read ZIP file: {}", zip_path.display()))?;

        if archive.is_empty() {
            return Err(anyhow!(format_skill_error(
                "EMPTY_ARCHIVE",
                &[],
                Some("checkZipContent"),
            )));
        }

        let temp_dir = tempfile::tempdir()?;
        let temp_path = temp_dir.path().to_path_buf();
        let _ = temp_dir.keep(); // Keep the directory, we'll clean up later

        let mut symlinks: Vec<(PathBuf, String)> = Vec::new();

        for i in 0..archive.len() {
            let mut file = archive.by_index(i)?;
            let file_path = match file.enclosed_name() {
                Some(path) => path.to_owned(),
                None => continue,
            };

            let outpath = temp_path.join(&file_path);

            if file.is_symlink() {
                let mut target = String::new();
                std::io::Read::read_to_string(&mut file, &mut target)?;
                symlinks.push((outpath, target.trim().to_string()));
            } else if file.is_dir() {
                fs::create_dir_all(&outpath)?;
            } else {
                if let Some(parent) = outpath.parent() {
                    fs::create_dir_all(parent)?;
                }
                let mut outfile = fs::File::create(&outpath)?;
                std::io::copy(&mut file, &mut outfile)?;
            }
        }

        // 解析 symlink
        Self::resolve_symlinks_in_dir(&temp_path, &symlinks)?;

        Ok(temp_path)
    }

    /// 递归扫描目录查找包含 SKILL.md 的技能目录
    fn scan_skills_in_dir(dir: &Path) -> Result<Vec<PathBuf>> {
        let mut skill_dirs = Vec::new();
        Self::scan_skills_recursive(dir, &mut skill_dirs)?;
        Ok(skill_dirs)
    }

    /// 递归扫描辅助函数
    fn scan_skills_recursive(current: &Path, results: &mut Vec<PathBuf>) -> Result<()> {
        // 检查当前目录是否包含 SKILL.md
        let skill_md = current.join("SKILL.md");
        if skill_md.exists() {
            results.push(current.to_path_buf());
            // 找到后不再递归子目录（一个 skill 目录）
            return Ok(());
        }

        // 递归子目录
        if let Ok(entries) = fs::read_dir(current) {
            for entry in entries.flatten() {
                let path = entry.path();
                if path.is_dir() {
                    // 跳过隐藏目录
                    let dir_name = entry.file_name().to_string_lossy().to_string();
                    if dir_name.starts_with('.') {
                        continue;
                    }
                    Self::scan_skills_recursive(&path, results)?;
                }
            }
        }

        Ok(())
    }

    // ========== 仓库管理（保留原有逻辑）==========

    /// 列出仓库
    pub fn list_repos(&self, store: &SkillStore) -> Vec<SkillRepo> {
        store.repos.clone()
    }

    /// 添加仓库
    pub fn add_repo(&self, store: &mut SkillStore, repo: SkillRepo) -> Result<()> {
        if let Some(pos) = store
            .repos
            .iter()
            .position(|r| r.owner == repo.owner && r.name == repo.name)
        {
            store.repos[pos] = repo;
        } else {
            store.repos.push(repo);
        }

        Ok(())
    }

    /// 删除仓库
    pub fn remove_repo(&self, store: &mut SkillStore, owner: String, name: String) -> Result<()> {
        store
            .repos
            .retain(|r| !(r.owner == owner && r.name == name));

        Ok(())
    }

    // ========== skills.sh 搜索 ==========

    /// 搜索 skills.sh 公共目录
    pub async fn search_skills_sh(query: &str, limit: usize) -> Result<SkillsShSearchResult> {
        let query = query.trim();
        if query.chars().count() < 2 {
            return Err(anyhow!("skills.sh 搜索关键词至少需要 2 个字符"));
        }

        let client = crate::proxy::http_client::get();
        let limit = limit.clamp(1, 200);

        let url = url::Url::parse_with_params(
            "https://skills.sh/api/search",
            &[("q", query), ("limit", &limit.to_string())],
        )?;

        let resp = client
            .get(url)
            .timeout(std::time::Duration::from_secs(10))
            .send()
            .await?
            .error_for_status()?
            .json::<SkillsShApiResponse>()
            .await?;

        let skills = resp
            .skills
            .into_iter()
            .filter_map(map_skills_sh_skill)
            .collect::<Vec<_>>();
        let result_count = skills.len();

        Ok(SkillsShSearchResult {
            skills,
            result_count,
            query: resp.query,
        })
    }

    /// 读取 skills.sh 公开榜单。
    ///
    /// `/api/v1/skills` 要求 Vercel OIDC；桌面端改读官网公开页面中用于首屏渲染的
    /// `initialSkills` 数据，确保用户无需配置第三方凭据。
    pub async fn get_skills_sh_leaderboard(
        view: &str,
        limit: usize,
    ) -> Result<SkillsShLeaderboardResult> {
        let path = match view {
            "all-time" => "",
            "trending" => "trending",
            "hot" => "hot",
            _ => return Err(anyhow!("Unsupported skills.sh leaderboard view: {view}")),
        };
        let url = if path.is_empty() {
            "https://www.skills.sh/".to_string()
        } else {
            format!("https://www.skills.sh/{path}")
        };

        let html = crate::proxy::http_client::get()
            .get(url)
            .header(reqwest::header::ACCEPT, "text/html")
            .timeout(std::time::Duration::from_secs(15))
            .send()
            .await?
            .error_for_status()?
            .text()
            .await?;
        let props = parse_skills_sh_leaderboard(&html)?;
        if props.view != view {
            return Err(anyhow!(
                "skills.sh leaderboard returned '{}' for requested view '{view}'",
                props.view
            ));
        }

        let skills = props
            .initial_skills
            .into_iter()
            .filter_map(map_skills_sh_leaderboard_skill)
            .take(limit.clamp(1, 300))
            .collect::<Vec<_>>();
        let result_count = skills.len();

        Ok(SkillsShLeaderboardResult {
            skills,
            result_count,
            total_skills: props.total_skills,
            all_time_total: props.all_time_total,
            view: props.view,
        })
    }

    /// 读取 skills.sh 的公开发布者详情页。
    pub async fn get_skills_sh_publisher(owner: &str) -> Result<SkillsShPublisherDetail> {
        validate_skills_sh_segment("publisher", owner)?;
        let html = fetch_skills_sh_page(&[owner]).await?;
        parse_skills_sh_publisher(&html, owner)
    }

    /// 读取 skills.sh 的公开仓库详情页。
    pub async fn get_skills_sh_repository(
        owner: &str,
        repository: &str,
    ) -> Result<SkillsShRepositoryDetail> {
        validate_skills_sh_segment("publisher", owner)?;
        validate_skills_sh_segment("repository", repository)?;
        let html = fetch_skills_sh_page(&[owner, repository]).await?;
        parse_skills_sh_repository(&html, owner, repository)
    }

    /// 读取 skills.sh 的公开 Skill 详情页。
    pub async fn get_skills_sh_detail(
        repo_owner: &str,
        repo_name: &str,
        skill_id: &str,
    ) -> Result<SkillsShSkillDetail> {
        for (label, segment) in [
            ("repo owner", repo_owner),
            ("repo name", repo_name),
            ("skill id", skill_id),
        ] {
            validate_skills_sh_segment(label, segment)?;
        }

        let html = fetch_skills_sh_page(&[repo_owner, repo_name, skill_id]).await?;

        parse_skills_sh_detail(&html)
    }
}

fn validate_skills_sh_segment(label: &str, segment: &str) -> Result<()> {
    if segment.is_empty() || segment == "." || segment == ".." || segment.contains(['/', '\\']) {
        return Err(anyhow!("Invalid skills.sh {label}: {segment}"));
    }
    Ok(())
}

async fn fetch_skills_sh_page(segments: &[&str]) -> Result<String> {
    let mut url = url::Url::parse("https://www.skills.sh/")?;
    url.path_segments_mut()
        .map_err(|_| anyhow!("Failed to build skills.sh URL"))?
        .extend(segments.iter().copied());

    crate::proxy::http_client::get()
        .get(url)
        .header(reqwest::header::ACCEPT, "text/html")
        .timeout(std::time::Duration::from_secs(15))
        .send()
        .await?
        .error_for_status()?
        .text()
        .await
        .context("Failed to read skills.sh page")
}

fn extract_skills_sh_metric(html: &str, label: &str) -> Option<String> {
    let mut cursor = 0;

    while let Some(svg_end) = html[cursor..].find("</svg>") {
        let value_start = cursor + svg_end + "</svg>".len();
        let after_icon = &html[value_start..];
        let Some(span_end) = after_icon.find("</span>") else {
            break;
        };
        let visible_text = strip_html_tags(&after_icon[..span_end]);

        if let Some(value) = visible_text.strip_suffix(label) {
            let value = value.trim();
            if !value.is_empty() {
                return Some(value.to_string());
            }
        }

        cursor = value_start;
    }

    None
}

fn extract_skills_sh_count(html: &str, singular: &str) -> Option<usize> {
    let value = extract_skills_sh_metric(html, &format!("{singular}s"))
        .or_else(|| extract_skills_sh_metric(html, singular))?;
    value.replace(',', "").parse().ok()
}

fn parse_compact_installs(value: &str) -> Option<u64> {
    let normalized = value.trim().replace(',', "");
    let (number, multiplier) = match normalized.chars().last()? {
        'K' | 'k' => (&normalized[..normalized.len() - 1], 1_000_f64),
        'M' | 'm' => (&normalized[..normalized.len() - 1], 1_000_000_f64),
        'B' | 'b' => (&normalized[..normalized.len() - 1], 1_000_000_000_f64),
        _ => (normalized.as_str(), 1_f64),
    };
    number
        .parse::<f64>()
        .ok()
        .map(|number| (number * multiplier).round() as u64)
}

fn parse_skills_sh_publisher(html: &str, owner: &str) -> Result<SkillsShPublisherDetail> {
    let owner_pattern = regex::escape(owner);
    let row_pattern = format!(
        r#"(?s)<a[^>]+href="/{owner_pattern}/([^"/]+)"[^>]*>.*?<h3[^>]*>(.*?)</h3>.*?<p[^>]*>(.*?)</p>.*?<span[^>]*>(.*?)</span>.*?</a>"#
    );
    let row_regex = Regex::new(&row_pattern).context("Failed to build publisher row parser")?;
    let sources = row_regex
        .captures_iter(html)
        .filter_map(|captures| {
            let name = strip_html_tags(captures.get(1)?.as_str());
            let heading = strip_html_tags(captures.get(2)?.as_str());
            if name != heading {
                return None;
            }
            Some(SkillsShSourceSummary {
                name,
                skill_summary: strip_html_tags(captures.get(3)?.as_str()),
                installs: strip_html_tags(captures.get(4)?.as_str()),
            })
        })
        .collect::<Vec<_>>();

    if sources.is_empty() {
        return Err(anyhow!("skills.sh publisher sources were not found"));
    }

    Ok(SkillsShPublisherDetail {
        owner: owner.to_string(),
        source_count: extract_skills_sh_count(html, "source").unwrap_or(sources.len()),
        skill_count: extract_skills_sh_count(html, "skill")
            .ok_or_else(|| anyhow!("skills.sh publisher skill count was not found"))?,
        total_installs: extract_skills_sh_metric(html, "total installs")
            .ok_or_else(|| anyhow!("skills.sh publisher installs were not found"))?,
        sources,
    })
}

fn parse_skills_sh_repository(
    html: &str,
    owner: &str,
    repository: &str,
) -> Result<SkillsShRepositoryDetail> {
    let owner_pattern = regex::escape(owner);
    let repository_pattern = regex::escape(repository);
    let row_pattern = format!(
        r#"(?s)<a[^>]+href="/{owner_pattern}/{repository_pattern}/([^"/]+)"[^>]*>.*?<h3[^>]*>(.*?)</h3>.*?<span[^>]*>(.*?)</span>.*?</a>"#
    );
    let row_regex = Regex::new(&row_pattern).context("Failed to build repository row parser")?;
    let skills = row_regex
        .captures_iter(html)
        .filter_map(|captures| {
            let skill_id = strip_html_tags(captures.get(1)?.as_str());
            let name = strip_html_tags(captures.get(2)?.as_str());
            let installs_label = strip_html_tags(captures.get(3)?.as_str());
            Some(SkillsShRepositorySkill {
                skill_id,
                name,
                installs: parse_compact_installs(&installs_label)?,
                installs_label,
            })
        })
        .collect::<Vec<_>>();

    if skills.is_empty() {
        return Err(anyhow!("skills.sh repository skills were not found"));
    }

    Ok(SkillsShRepositoryDetail {
        owner: owner.to_string(),
        repository: repository.to_string(),
        skill_count: extract_skills_sh_count(html, "skill").unwrap_or(skills.len()),
        total_installs: extract_skills_sh_metric(html, "total installs")
            .ok_or_else(|| anyhow!("skills.sh repository installs were not found"))?,
        skills,
    })
}

fn extract_prose_section<'a>(html: &'a str, label: &str, end_marker: &str) -> Option<&'a str> {
    let label_index = html.find(label)?;
    let after_label = &html[label_index + label.len()..];
    let prose_index = after_label.find("<div class=\"prose ")?;
    let prose = &after_label[prose_index..];
    let content_start = prose.find('>')? + 1;
    let content = &prose[content_start..];
    let content_end = content.find(end_marker)?;
    Some(content[..content_end].trim())
}

fn decode_html_text(value: &str) -> String {
    value
        .replace("&amp;", "&")
        .replace("&#x27;", "'")
        .replace("&#39;", "'")
        .replace("&quot;", "\"")
        .replace("&lt;", "<")
        .replace("&#x3C;", "<")
        .replace("&gt;", ">")
}

fn strip_html_tags(value: &str) -> String {
    let mut result = String::with_capacity(value.len());
    let mut in_tag = false;
    for character in value.chars() {
        match character {
            '<' => in_tag = true,
            '>' => in_tag = false,
            _ if !in_tag => result.push(character),
            _ => {}
        }
    }
    decode_html_text(result.trim())
}

fn extract_element_text_after(html: &str, marker: &str, tag: &str) -> Option<String> {
    let marker_index = html.find(marker)?;
    let after_marker = &html[marker_index + marker.len()..];
    let tag_prefix = format!("<{tag}");
    let tag_index = after_marker.find(&tag_prefix)?;
    let tag_content = &after_marker[tag_index..];
    let content_start = tag_content.find('>')? + 1;
    let content = &tag_content[content_start..];
    let end_tag = format!("</{tag}>");
    let content_end = content.find(&end_tag)?;
    Some(strip_html_tags(&content[..content_end]))
}

fn parse_skills_sh_security_audits(html: &str) -> Vec<SkillsShSecurityAudit> {
    let Some(section_index) = html.find(">Security Audits</div>") else {
        return Vec::new();
    };
    let section_tail = &html[section_index..];
    let section_end = section_tail
        .find("</div></div></div></main>")
        .unwrap_or(section_tail.len());
    let mut section = &section_tail[..section_end];
    let mut audits = Vec::new();

    while let Some(security_index) = section.find("/security/") {
        let after_security = &section[security_index..];
        let Some(anchor_end) = after_security.find("</a>") else {
            break;
        };
        let anchor = &after_security[..anchor_end];
        let mut spans = Vec::new();
        let mut rest = anchor;
        while let Some(span_index) = rest.find("<span") {
            let span = &rest[span_index..];
            let Some(content_start) = span.find('>') else {
                break;
            };
            let content = &span[content_start + 1..];
            let Some(content_end) = content.find("</span>") else {
                break;
            };
            spans.push(strip_html_tags(&content[..content_end]));
            rest = &content[content_end + "</span>".len()..];
        }
        if spans.len() >= 2 && !spans[0].is_empty() && !spans[1].is_empty() {
            audits.push(SkillsShSecurityAudit {
                provider: spans[0].clone(),
                status: spans[1].clone(),
            });
        }
        section = &after_security[anchor_end + "</a>".len()..];
    }

    audits
}

fn parse_skills_sh_detail(html: &str) -> Result<SkillsShSkillDetail> {
    const SUMMARY_LABEL: &str = ">Summary</div>";
    const SUMMARY_END: &str = "</div></div></div><div class=\"bg-background\">";
    const CONTENT_LABEL: &str = "<span>SKILL.md</span>";
    const CONTENT_END: &str = "</div><div class=\"relative\">";

    let summary_html = extract_prose_section(html, SUMMARY_LABEL, SUMMARY_END)
        .ok_or_else(|| anyhow!("skills.sh detail summary was not found"))?;
    let content_html = extract_prose_section(html, CONTENT_LABEL, CONTENT_END)
        .ok_or_else(|| anyhow!("skills.sh detail content was not found"))?;
    let topic = html.find("href=\"/topic/").and_then(|topic_index| {
        let topic_anchor = &html[topic_index..];
        let content_start = topic_anchor.find('>')? + 1;
        let content = &topic_anchor[content_start..];
        let content_end = content.find("</a>")?;
        Some(strip_html_tags(&content[..content_end]))
    });

    Ok(SkillsShSkillDetail {
        topic,
        summary_html: summary_html.to_string(),
        content_html: content_html.to_string(),
        github_stars: extract_element_text_after(html, ">GitHub Stars</span>", "span"),
        first_seen: extract_element_text_after(html, ">First Seen</span>", "div"),
        security_audits: parse_skills_sh_security_audits(html),
    })
}

fn parse_skills_sh_leaderboard(html: &str) -> Result<SkillsShLeaderboardProps> {
    const PAYLOAD_PREFIX: &str = "self.__next_f.push([1,";
    const PAYLOAD_SUFFIX: &str = "])</script>";
    const PROPS_MARKER: &str = "\"initialSkills\":";
    const ESCAPED_PROPS_MARKER: &str = "\\\"initialSkills\\\":";

    let marker_index = html
        .find(ESCAPED_PROPS_MARKER)
        .ok_or_else(|| anyhow!("skills.sh leaderboard data was not found"))?;
    let payload_prefix_index = html[..marker_index]
        .rfind(PAYLOAD_PREFIX)
        .ok_or_else(|| anyhow!("skills.sh leaderboard payload prefix was not found"))?;
    let encoded_start = payload_prefix_index + PAYLOAD_PREFIX.len();
    let encoded_end = html[marker_index..]
        .find(PAYLOAD_SUFFIX)
        .map(|index| marker_index + index)
        .ok_or_else(|| anyhow!("skills.sh leaderboard payload suffix was not found"))?;
    let encoded_payload = &html[encoded_start..encoded_end];
    let payload: String = serde_json::from_str(encoded_payload)
        .context("Failed to decode skills.sh leaderboard payload")?;

    let props_marker_index = payload
        .find(PROPS_MARKER)
        .ok_or_else(|| anyhow!("skills.sh leaderboard props were not found"))?;
    let props_start = payload[..props_marker_index]
        .rfind('{')
        .ok_or_else(|| anyhow!("skills.sh leaderboard props start was not found"))?;
    let mut deserializer = serde_json::Deserializer::from_str(&payload[props_start..]);
    SkillsShLeaderboardProps::deserialize(&mut deserializer)
        .context("Failed to parse skills.sh leaderboard props")
}

fn map_skills_sh_leaderboard_skill(
    skill: SkillsShLeaderboardApiSkill,
) -> Option<SkillsShDiscoverableSkill> {
    let weekly_installs = skill.weekly_installs;
    let is_official = skill.is_official;
    let key = format!("{}/{}", skill.source, skill.skill_id);
    map_skills_sh_skill(SkillsShApiSkill {
        id: key,
        skill_id: skill.skill_id,
        name: skill.name,
        installs: skill.installs,
        source: skill.source,
    })
    .map(|mut mapped| {
        mapped.weekly_installs = weekly_installs;
        mapped.is_official = is_official;
        mapped
    })
}

fn map_skills_sh_skill(skill: SkillsShApiSkill) -> Option<SkillsShDiscoverableSkill> {
    let (owner, repo) = skill.source.split_once('/')?;
    if owner.is_empty()
        || repo.is_empty()
        || repo.contains('/')
        || owner.contains('.')
        || repo.contains('.')
    {
        // 当前安装器通过 GitHub 仓库下载；skills.sh 的 well-known 来源暂不混入可安装结果。
        return None;
    }

    let detail_url = format!("https://skills.sh/{}/{}/{}", owner, repo, skill.skill_id);

    Some(SkillsShDiscoverableSkill {
        key: skill.id,
        name: skill.name,
        directory: skill.skill_id,
        repo_owner: owner.to_string(),
        repo_name: repo.to_string(),
        repo_branch: "main".to_string(),
        installs: skill.installs,
        weekly_installs: Vec::new(),
        is_official: false,
        readme_url: Some(detail_url.clone()),
        detail_url,
    })
}

// ========== 迁移支持 ==========

/// 从 lock 文件信息构建 skill 的 ID、仓库字段和 readme URL
///
/// 返回 (id, repo_owner, repo_name, repo_branch, readme_url)
fn build_repo_info_from_lock(
    lock: &HashMap<String, LockRepoInfo>,
    dir_name: &str,
) -> (
    String,
    Option<String>,
    Option<String>,
    Option<String>,
    Option<String>,
) {
    match lock.get(dir_name) {
        Some(info) => {
            let branch = info.branch.clone();
            let url_branch = branch.clone().unwrap_or_else(|| "HEAD".to_string());
            // 优先使用 lock 文件中的 skillPath，否则回退到 dir_name/SKILL.md
            let fallback = format!("{dir_name}/SKILL.md");
            let doc_path = info.skill_path.as_deref().unwrap_or(&fallback);
            let url = Some(SkillService::build_skill_doc_url(
                &info.owner,
                &info.repo,
                &url_branch,
                doc_path,
            ));
            (
                format!("{}/{}:{dir_name}", info.owner, info.repo),
                Some(info.owner.clone()),
                Some(info.repo.clone()),
                branch,
                url,
            )
        }
        None => (format!("local:{dir_name}"), None, None, None, None),
    }
}

/// 将 lock 文件中发现的仓库保存到 skill_repos（去重）
fn save_repos_from_lock(
    db: &Arc<Database>,
    lock: &HashMap<String, LockRepoInfo>,
    directories: impl Iterator<Item = impl AsRef<str>>,
) {
    let existing_repos: HashSet<(String, String)> = db
        .get_skill_repos()
        .unwrap_or_default()
        .into_iter()
        .map(|r| (r.owner, r.name))
        .collect();
    let mut added = HashSet::new();

    for dir_name in directories {
        if let Some(info) = lock.get(dir_name.as_ref()) {
            let key = (info.owner.clone(), info.repo.clone());
            if !existing_repos.contains(&key) && added.insert(key) {
                let skill_repo = SkillRepo {
                    owner: info.owner.clone(),
                    name: info.repo.clone(),
                    // 未知分支时使用 HEAD 语义，后续下载会回退到 main/master。
                    branch: info.branch.clone().unwrap_or_else(|| "HEAD".to_string()),
                    enabled: true,
                };
                if let Err(e) = db.save_skill_repo(&skill_repo) {
                    log::warn!("保存 skill 仓库 {}/{} 失败: {}", info.owner, info.repo, e);
                } else {
                    log::info!(
                        "从 agents lock 文件发现并添加仓库: {}/{} ({})",
                        info.owner,
                        info.repo,
                        skill_repo.branch
                    );
                }
            }
        }
    }
}

/// 首次启动迁移：扫描应用目录，重建数据库
pub fn migrate_skills_to_ssot(db: &Arc<Database>) -> Result<usize> {
    let ssot_dir = SkillService::get_ssot_dir()?;
    let agents_lock = parse_agents_lock();
    let snapshot: Vec<LegacySkillMigrationRow> =
        match db.get_setting("skills_ssot_migration_snapshot")? {
            Some(value) if !value.trim().is_empty() => match serde_json::from_str(&value) {
                Ok(rows) => rows,
                Err(err) => {
                    log::warn!("解析 skills 迁移快照失败，将回退到文件系统扫描: {err}");
                    Vec::new()
                }
            },
            _ => Vec::new(),
        };

    let has_snapshot = !snapshot.is_empty();
    let mut discovered: HashMap<String, SkillApps> = HashMap::new();

    if has_snapshot {
        for row in &snapshot {
            if let Ok(app) = row.app_type.parse::<AppType>() {
                discovered
                    .entry(row.directory.clone())
                    .or_default()
                    .set_enabled_for(&app, true);
            }
        }
    }

    // 扫描各应用目录
    for app in AppType::all() {
        let app_dir = match SkillService::get_app_skills_dir(&app) {
            Ok(d) => d,
            Err(_) => continue,
        };

        let entries = match fs::read_dir(&app_dir) {
            Ok(e) => e,
            Err(_) => continue,
        };

        for entry in entries.flatten() {
            let path = entry.path();
            if !path.is_dir() {
                continue;
            }

            let dir_name = entry.file_name().to_string_lossy().to_string();
            if dir_name.starts_with('.') {
                continue;
            }
            if !path.join("SKILL.md").exists() {
                continue;
            }
            if has_snapshot && !discovered.contains_key(&dir_name) {
                continue;
            }

            // 复制到 SSOT（如果不存在）
            let ssot_path = ssot_dir.join(&dir_name);
            if !ssot_path.exists() {
                SkillService::copy_dir_recursive(&path, &ssot_path)?;
            }

            if !has_snapshot {
                discovered
                    .entry(dir_name)
                    .or_default()
                    .set_enabled_for(&app, true);
            }
        }
    }

    // 重建数据库
    db.clear_skills()?;

    // 将 lock 文件中发现的仓库保存到 skill_repos
    save_repos_from_lock(db, &agents_lock, discovered.keys());

    let mut count = 0;
    for (directory, apps) in discovered {
        let ssot_path = ssot_dir.join(&directory);
        let skill_md = ssot_path.join("SKILL.md");

        let (name, description) = SkillService::read_skill_name_desc(&skill_md, &directory);

        let (id, repo_owner, repo_name, repo_branch, readme_url) =
            build_repo_info_from_lock(&agents_lock, &directory);

        let content_hash = SkillService::compute_dir_hash(&ssot_path).ok();

        let skill = InstalledSkill {
            id,
            name,
            description,
            directory,
            repo_owner,
            repo_name,
            repo_branch,
            readme_url,
            apps,
            installed_at: chrono::Utc::now().timestamp(),
            content_hash,
            updated_at: 0,
        };

        db.save_skill(&skill)?;
        count += 1;
    }

    let _ = db.set_setting("skills_ssot_migration_snapshot", "");

    log::info!("Skills 迁移完成，共 {count} 个");

    Ok(count)
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    fn write_skill(dir: &Path, name: &str) {
        fs::create_dir_all(dir).expect("create skill dir");
        fs::write(
            dir.join("SKILL.md"),
            format!("---\nname: {name}\ndescription: Test skill\n---\n"),
        )
        .expect("write SKILL.md");
    }

    fn discoverable_skill() -> DiscoverableSkill {
        DiscoverableSkill {
            key: "vercel-labs/skills:find-skills".to_string(),
            name: "find-skills".to_string(),
            description: "Find agent skills".to_string(),
            directory: "find-skills".to_string(),
            readme_url: None,
            repo_owner: "vercel-labs".to_string(),
            repo_name: "skills".to_string(),
            repo_branch: "main".to_string(),
        }
    }

    #[test]
    fn skills_cli_add_args_use_non_interactive_global_install() {
        let args = SkillService::skills_cli_add_args(&discoverable_skill(), "find-skills", true)
            .expect("valid skills CLI arguments");

        assert_eq!(
            args,
            vec![
                "add",
                "vercel-labs/skills",
                "--skill",
                "find-skills",
                "--global",
                "--agent",
                "codex",
                "--yes",
            ]
        );
    }

    #[test]
    fn skills_cli_update_args_use_non_interactive_global_update() {
        let args = SkillService::skills_cli_update_args("find-skills", true)
            .expect("valid skills CLI arguments");

        assert_eq!(args, vec!["update", "find-skills", "--global", "--yes"]);
    }

    #[test]
    fn skills_cli_args_reject_shell_metacharacters() {
        let mut skill = discoverable_skill();
        skill.repo_owner = "vercel-labs;touch-pwned".to_string();

        assert!(SkillService::skills_cli_add_args(&skill, "find-skills", false).is_err());
        assert!(SkillService::skills_cli_update_args("find-skills && bad", false).is_err());
    }

    #[test]
    fn skills_cli_project_args_keep_cli_workspaces_isolated() {
        let add = SkillService::skills_cli_add_args(&discoverable_skill(), "find-skills", false)
            .expect("valid project install arguments");
        let update = SkillService::skills_cli_update_args("find-skills", false)
            .expect("valid project update arguments");

        assert!(!add.iter().any(|argument| argument == "--global"));
        assert!(!update.iter().any(|argument| argument == "--global"));
        assert_eq!(
            add,
            vec![
                "add",
                "vercel-labs/skills",
                "--skill",
                "find-skills",
                "--agent",
                "codex",
                "--yes",
            ]
        );
        assert_eq!(update, vec!["update", "find-skills", "--yes"]);
    }

    #[test]
    fn parse_agents_lock_at_reads_global_command_lock() {
        let temp = tempdir().expect("tempdir");
        let agents_dir = temp.path().join(".agents");
        fs::create_dir_all(&agents_dir).expect("create agents dir");
        fs::write(
            agents_dir.join(".skill-lock.json"),
            r#"{
              "skills": {
                "find-skills": {
                  "source": "vercel-labs/skills",
                  "sourceType": "github",
                  "skillPath": "skills/find-skills/SKILL.md",
                  "branch": "main"
                }
              }
            }"#,
        )
        .expect("write lock file");

        let lock = parse_agents_lock_at(temp.path());
        let entry = lock.get("find-skills").expect("parse isolated lock");
        assert_eq!(entry.owner, "vercel-labs");
        assert_eq!(entry.repo, "skills");
        assert_eq!(entry.branch.as_deref(), Some("main"));
    }

    #[test]
    fn parse_skills_cli_lock_reads_project_workspace_lock() {
        let temp = tempdir().expect("tempdir");
        fs::write(
            temp.path().join("skills-lock.json"),
            r#"{
              "version": 1,
              "skills": {
                "find-skills": {
                  "source": "vercel-labs/skills",
                  "sourceType": "github",
                  "skillPath": "skills/find-skills/SKILL.md"
                }
              }
            }"#,
        )
        .expect("write project lock file");

        let lock = parse_skills_cli_lock(temp.path(), false);
        let entry = lock.get("find-skills").expect("parse project lock");
        assert_eq!(entry.owner, "vercel-labs");
        assert_eq!(entry.repo, "skills");
        assert_eq!(
            entry.skill_path.as_deref(),
            Some("skills/find-skills/SKILL.md")
        );
    }

    #[test]
    fn resolve_skill_source_dir_returns_repo_root_for_root_level_skill() {
        let temp = tempdir().expect("tempdir");
        write_skill(temp.path(), "Root Skill");

        let resolved = SkillService::resolve_skill_source_dir(temp.path(), "last30days-skill-cn")
            .expect("root-level skill should resolve to the extracted repo root");

        assert_eq!(resolved, temp.path());
    }

    #[test]
    fn resolve_skill_source_dir_returns_direct_nested_directory_when_present() {
        let temp = tempdir().expect("tempdir");
        let nested = temp.path().join("skills").join("nested-skill");
        write_skill(&nested, "Nested Skill");

        let resolved = SkillService::resolve_skill_source_dir(temp.path(), "skills/nested-skill")
            .expect("nested skill should resolve from its relative source path");

        assert_eq!(resolved, nested);
    }

    #[test]
    fn resolve_skill_source_dir_falls_back_to_matching_install_name() {
        let temp = tempdir().expect("tempdir");
        let nested = temp.path().join("skills").join("nested-skill");
        write_skill(&nested, "Nested Skill");

        let resolved = SkillService::resolve_skill_source_dir(temp.path(), "nested-skill")
            .expect("install name should fall back to the matching discovered skill directory");

        assert_eq!(resolved, nested);
    }

    #[test]
    fn replace_dest_with_copy_rejects_empty_source_without_touching_existing_dest() {
        let temp = tempdir().expect("tempdir");
        let source = temp.path().join("source-skill");
        let dest = temp.path().join("app-skills").join("source-skill");
        fs::create_dir_all(&source).expect("create empty source");
        write_skill(&dest, "Existing Skill");

        let err = SkillService::replace_dest_with_copy(&source, &dest, "source-skill")
            .expect_err("empty source should not replace existing app skill");

        assert!(
            err.to_string().contains("SKILL.md"),
            "unexpected error: {err:#}"
        );
        assert!(
            dest.join("SKILL.md").is_file(),
            "existing destination skill should be preserved"
        );
    }

    #[test]
    fn map_skills_sh_skill_builds_canonical_detail_url() {
        let skill = map_skills_sh_skill(SkillsShApiSkill {
            id: "vercel-labs/skills/find-skills".to_string(),
            skill_id: "find-skills".to_string(),
            name: "find-skills".to_string(),
            installs: 42,
            source: "vercel-labs/skills".to_string(),
        })
        .expect("GitHub skills.sh entry should be installable");

        assert_eq!(skill.repo_owner, "vercel-labs");
        assert_eq!(skill.repo_name, "skills");
        assert_eq!(
            skill.detail_url,
            "https://skills.sh/vercel-labs/skills/find-skills"
        );
        assert_eq!(skill.readme_url.as_deref(), Some(skill.detail_url.as_str()));
    }

    #[test]
    fn map_skills_sh_skill_filters_well_known_sources() {
        let skill = map_skills_sh_skill(SkillsShApiSkill {
            id: "mintlify.com/mintlify".to_string(),
            skill_id: "mintlify".to_string(),
            name: "Mintlify".to_string(),
            installs: 42,
            source: "mintlify.com".to_string(),
        });

        assert!(skill.is_none());
    }

    #[test]
    fn parse_skills_sh_leaderboard_reads_public_next_payload() {
        let payload = r#"4a:["$","component",null,{"initialSkills":[{"source":"vercel-labs/skills","skillId":"find-skills","name":"find-skills","installs":24531,"weeklyInstalls":[120,140,130],"isOfficial":true}],"totalSkills":8420,"allTimeTotal":1044078,"view":"all-time"}]
next"#;
        let encoded = serde_json::to_string(payload).expect("encode RSC payload");
        let html = format!(r#"<html><script>self.__next_f.push([1,{encoded}])</script></html>"#);

        let props = parse_skills_sh_leaderboard(&html).expect("parse leaderboard payload");

        assert_eq!(props.view, "all-time");
        assert_eq!(props.total_skills, 8420);
        assert_eq!(props.all_time_total, 1_044_078);
        assert_eq!(props.initial_skills.len(), 1);
        assert_eq!(props.initial_skills[0].skill_id, "find-skills");
        assert_eq!(props.initial_skills[0].weekly_installs, vec![120, 140, 130]);
        assert!(props.initial_skills[0].is_official);
    }

    #[test]
    fn parse_skills_sh_detail_reads_public_page_sections() {
        let html = r#"
<a href="/topic/agent-workflows">Agent workflows</a>
<div class="uppercase">Summary</div>
<div><div><div class="prose detail"><p><strong>Summary text</strong></p></div></div></div><div class="bg-background">
<span>SKILL.md</span>
<div class="prose detail"><h1>Find Skills</h1><p>Body text</p></div><div class="relative">
<span>GitHub Stars</span><div><svg></svg><span>27.6K</span></div>
<span>First Seen</span><div class="value">Jan 26, 2026</div>
<div>Security Audits</div>
<a href="/owner/repo/skill/security/socket"><span class="name">Socket</span><span class="status">Pass</span></a>
<a href="/owner/repo/skill/security/snyk"><span class="name">Snyk</span><span class="status">Warn</span></a>
</div></div></div></main>
"#;

        let detail = parse_skills_sh_detail(html).expect("parse detail");

        assert_eq!(detail.topic.as_deref(), Some("Agent workflows"));
        assert_eq!(detail.summary_html, "<p><strong>Summary text</strong></p>");
        assert_eq!(detail.content_html, "<h1>Find Skills</h1><p>Body text</p>");
        assert_eq!(detail.github_stars.as_deref(), Some("27.6K"));
        assert_eq!(detail.first_seen.as_deref(), Some("Jan 26, 2026"));
        assert_eq!(detail.security_audits.len(), 2);
        assert_eq!(detail.security_audits[0].provider, "Socket");
        assert_eq!(detail.security_audits[1].status, "Warn");
    }

    #[test]
    fn parse_skills_sh_publisher_reads_sources_and_metrics() {
        let html = r#"
<div><svg></svg>49<!-- --> <!-- -->sources</span></div>
<div><svg></svg>198<!-- --> skills</span></div>
<div><svg></svg>5.3M<!-- --> <!-- -->total installs</span></div>
<a class="group grid" href="/vercel-labs/skills"><div><h3>skills</h3><p>1<!-- --> <!-- -->skill<!-- -->:<!-- --> <!-- -->find-skills</p></div><div><span>2.8M</span></div></a>
<a class="group grid" href="/vercel-labs/agent-browser"><div><h3>agent-browser</h3><p>2<!-- --> <!-- -->skills<!-- -->:<!-- --> <!-- -->agent-browser, derive-client</p></div><div><span>531.3K</span></div></a>
"#;

        let detail = parse_skills_sh_publisher(html, "vercel-labs").expect("parse publisher page");

        assert_eq!(detail.owner, "vercel-labs");
        assert_eq!(detail.source_count, 49);
        assert_eq!(detail.skill_count, 198);
        assert_eq!(detail.total_installs, "5.3M");
        assert_eq!(detail.sources.len(), 2);
        assert_eq!(detail.sources[0].name, "skills");
        assert_eq!(detail.sources[0].skill_summary, "1 skill: find-skills");
        assert_eq!(detail.sources[1].installs, "531.3K");
    }

    #[test]
    fn parse_skills_sh_repository_reads_skills_and_metrics() {
        let html = r#"
<div><svg></svg>1<!-- --> <!-- -->skill</span></div>
<div><svg></svg>2.8M<!-- --> <!-- -->total installs</span></div>
<a class="group grid" href="/vercel-labs/skills/find-skills"><div><h3>find-skills</h3></div><div><span>2.8M</span></div></a>
"#;

        let detail = parse_skills_sh_repository(html, "vercel-labs", "skills")
            .expect("parse repository page");

        assert_eq!(detail.owner, "vercel-labs");
        assert_eq!(detail.repository, "skills");
        assert_eq!(detail.skill_count, 1);
        assert_eq!(detail.total_installs, "2.8M");
        assert_eq!(detail.skills.len(), 1);
        assert_eq!(detail.skills[0].skill_id, "find-skills");
        assert_eq!(detail.skills[0].installs, 2_800_000);
    }
}
