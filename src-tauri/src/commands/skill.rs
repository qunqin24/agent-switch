//! Skills 命令层
//!
//! 新版命令直接管理各 CLI 的原生 Skills 目录。
//! 旧版统一管理命令暂时保留，用于兼容历史调用。

use crate::app_config::{AppType, InstalledSkill, UnmanagedSkill};
use crate::error::format_skill_error;
use crate::services::skill::{
    AppSkill, AppSkillsResponse, DiscoverableSkill, GlobalSkill, GlobalSkillsResponse,
    ImportSkillSelection, MigrationResult, Skill, SkillBackupEntry, SkillRepo, SkillService,
    SkillStorageLocation, SkillUninstallResult, SkillUpdateInfo, SkillsShLeaderboardResult,
    SkillsShPublisherDetail, SkillsShRepositoryDetail, SkillsShSearchResult, SkillsShSkillDetail,
};
use crate::services::skill_builtin::{
    list_cli_provided_skills, CliProvidedSkill, CliProvidedSkillSource,
};
use crate::store::AppState;
use std::str::FromStr;
use std::sync::Arc;
use tauri::State;

/// SkillService 状态包装
pub struct SkillServiceState(pub Arc<SkillService>);

/// 解析 app 参数为 AppType
fn parse_app_type(app: &str) -> Result<AppType, String> {
    AppType::from_str(app).map_err(|e| e.to_string())
}

// ========== 按 CLI 原生目录管理 ==========

#[tauri::command]
pub fn get_app_skills(
    app: String,
    app_state: State<'_, AppState>,
) -> Result<AppSkillsResponse, String> {
    let app_type = parse_app_type(&app)?;
    SkillService::get_for_app(&app_state.db, &app_type).map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn get_cli_provided_skills(app: String) -> Result<Vec<CliProvidedSkill>, String> {
    let app_type = parse_app_type(&app)?;
    list_cli_provided_skills(&app_type)
        .await
        .map_err(|error| error.to_string())
}

async fn ensure_skill_is_user_managed(app: &AppType, directory: &str) -> Result<(), String> {
    let provided = list_cli_provided_skills(app)
        .await
        .map_err(|error| error.to_string())?;
    let Some(skill) = provided
        .into_iter()
        .find(|skill| skill.directory == directory)
    else {
        return Ok(());
    };

    let owner = match skill.source {
        CliProvidedSkillSource::Builtin => format!("{} CLI", app.as_str()),
        CliProvidedSkillSource::Plugin { plugin_name } => {
            format!("OpenCode plugin {plugin_name}")
        }
    };
    Err(format!(
        "Skill {} is managed by {owner} and cannot be changed from Agent Switch",
        skill.name
    ))
}

#[tauri::command]
pub fn get_app_skill_backups(app: String) -> Result<Vec<SkillBackupEntry>, String> {
    let app_type = parse_app_type(&app)?;
    SkillService::list_backups_for_app(&app_type).map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn install_app_skill(
    app: String,
    skill: DiscoverableSkill,
    service: State<'_, SkillServiceState>,
    app_state: State<'_, AppState>,
) -> Result<AppSkill, String> {
    let app_type = parse_app_type(&app)?;
    service
        .0
        .install_for_app(&app_state.db, &skill, &app_type)
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn uninstall_app_skill(
    app: String,
    directory: String,
    app_state: State<'_, AppState>,
) -> Result<SkillUninstallResult, String> {
    let app_type = parse_app_type(&app)?;
    ensure_skill_is_user_managed(&app_type, &directory).await?;
    SkillService::uninstall_for_app(&app_state.db, &app_type, &directory).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn restore_app_skill_backup(
    app: String,
    backup_id: String,
    app_state: State<'_, AppState>,
) -> Result<AppSkill, String> {
    let app_type = parse_app_type(&app)?;
    SkillService::restore_for_app(&app_state.db, &backup_id, &app_type).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn install_app_skills_from_zip(
    app: String,
    file_path: String,
    app_state: State<'_, AppState>,
) -> Result<Vec<AppSkill>, String> {
    let app_type = parse_app_type(&app)?;
    SkillService::install_from_zip_for_app(
        &app_state.db,
        std::path::Path::new(&file_path),
        &app_type,
    )
    .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn check_app_skill_updates(
    app: String,
    service: State<'_, SkillServiceState>,
    app_state: State<'_, AppState>,
) -> Result<Vec<SkillUpdateInfo>, String> {
    let app_type = parse_app_type(&app)?;
    service
        .0
        .check_app_updates(&app_state.db, &app_type)
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn update_app_skill(
    app: String,
    id: String,
    service: State<'_, SkillServiceState>,
    app_state: State<'_, AppState>,
) -> Result<AppSkill, String> {
    let app_type = parse_app_type(&app)?;
    let directory = SkillService::get_for_app(&app_state.db, &app_type)
        .map_err(|error| error.to_string())?
        .skills
        .into_iter()
        .find(|skill| skill.id == id)
        .map(|skill| skill.directory);
    if let Some(directory) = directory {
        ensure_skill_is_user_managed(&app_type, &directory).await?;
    }
    service
        .0
        .update_app_skill(&app_state.db, &app_type, &id)
        .await
        .map_err(|e| e.to_string())
}

// ========== 全局 Skills 库 ==========

#[tauri::command]
pub fn get_global_skills(app_state: State<'_, AppState>) -> Result<GlobalSkillsResponse, String> {
    SkillService::get_global(&app_state.db).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn get_global_skill_backups() -> Result<Vec<SkillBackupEntry>, String> {
    SkillService::list_global_backups().map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn install_global_skill(
    skill: DiscoverableSkill,
    service: State<'_, SkillServiceState>,
    app_state: State<'_, AppState>,
) -> Result<GlobalSkill, String> {
    service
        .0
        .install_global(&app_state.db, &skill)
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub fn set_global_skill_link(
    directory: String,
    app: String,
    enabled: bool,
    app_state: State<'_, AppState>,
) -> Result<GlobalSkill, String> {
    let app_type = parse_app_type(&app)?;
    SkillService::set_global_link(&app_state.db, &directory, &app_type, enabled)
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub fn uninstall_global_skill(
    directory: String,
    app_state: State<'_, AppState>,
) -> Result<SkillUninstallResult, String> {
    SkillService::uninstall_global(&app_state.db, &directory).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn restore_global_skill_backup(
    backup_id: String,
    app_state: State<'_, AppState>,
) -> Result<GlobalSkill, String> {
    SkillService::restore_global(&app_state.db, &backup_id).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn install_global_skills_from_zip(
    file_path: String,
    app_state: State<'_, AppState>,
) -> Result<Vec<GlobalSkill>, String> {
    SkillService::install_from_zip_global(&app_state.db, std::path::Path::new(&file_path))
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn check_global_skill_updates(
    service: State<'_, SkillServiceState>,
    app_state: State<'_, AppState>,
) -> Result<Vec<SkillUpdateInfo>, String> {
    service
        .0
        .check_global_updates(&app_state.db)
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn update_global_skill(
    id: String,
    service: State<'_, SkillServiceState>,
    app_state: State<'_, AppState>,
) -> Result<GlobalSkill, String> {
    service
        .0
        .update_global_skill(&app_state.db, &id)
        .await
        .map_err(|e| e.to_string())
}

// ========== 统一管理命令 ==========

/// 获取所有已安装的 Skills
#[tauri::command]
pub fn get_installed_skills(app_state: State<'_, AppState>) -> Result<Vec<InstalledSkill>, String> {
    SkillService::get_all_installed(&app_state.db).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn get_skill_backups() -> Result<Vec<SkillBackupEntry>, String> {
    SkillService::list_backups().map_err(|e| e.to_string())
}

#[tauri::command]
pub fn delete_skill_backup(backup_id: String) -> Result<bool, String> {
    SkillService::delete_backup(&backup_id).map_err(|e| e.to_string())?;
    Ok(true)
}

/// 安装 Skill（新版统一安装）
///
/// 参数：
/// - skill: 从发现列表获取的技能信息
/// - current_app: 当前选中的应用，安装后默认启用该应用
#[tauri::command]
pub async fn install_skill_unified(
    skill: DiscoverableSkill,
    current_app: String,
    service: State<'_, SkillServiceState>,
    app_state: State<'_, AppState>,
) -> Result<InstalledSkill, String> {
    let app_type = parse_app_type(&current_app)?;

    service
        .0
        .install(&app_state.db, &skill, &app_type)
        .await
        .map_err(|e| e.to_string())
}

/// 卸载 Skill（新版统一卸载）
#[tauri::command]
pub fn uninstall_skill_unified(
    id: String,
    app_state: State<'_, AppState>,
) -> Result<SkillUninstallResult, String> {
    SkillService::uninstall(&app_state.db, &id).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn restore_skill_backup(
    backup_id: String,
    current_app: String,
    app_state: State<'_, AppState>,
) -> Result<InstalledSkill, String> {
    let app_type = parse_app_type(&current_app)?;
    SkillService::restore_from_backup(&app_state.db, &backup_id, &app_type)
        .map_err(|e| e.to_string())
}

/// 切换 Skill 的应用启用状态
#[tauri::command]
pub fn toggle_skill_app(
    id: String,
    app: String,
    enabled: bool,
    app_state: State<'_, AppState>,
) -> Result<bool, String> {
    let app_type = parse_app_type(&app)?;
    SkillService::toggle_app(&app_state.db, &id, &app_type, enabled).map_err(|e| e.to_string())?;
    Ok(true)
}

/// 扫描未管理的 Skills
#[tauri::command]
pub fn scan_unmanaged_skills(
    app_state: State<'_, AppState>,
) -> Result<Vec<UnmanagedSkill>, String> {
    SkillService::scan_unmanaged(&app_state.db).map_err(|e| e.to_string())
}

/// 从应用目录导入 Skills
#[tauri::command]
pub fn import_skills_from_apps(
    imports: Vec<ImportSkillSelection>,
    app_state: State<'_, AppState>,
) -> Result<Vec<InstalledSkill>, String> {
    SkillService::import_from_apps(&app_state.db, imports).map_err(|e| e.to_string())
}

// ========== 发现功能命令 ==========

/// 发现可安装的 Skills（从仓库获取）
#[tauri::command]
pub async fn discover_available_skills(
    service: State<'_, SkillServiceState>,
    app_state: State<'_, AppState>,
) -> Result<Vec<DiscoverableSkill>, String> {
    let repos = app_state.db.get_skill_repos().map_err(|e| e.to_string())?;
    service
        .0
        .discover_available(repos)
        .await
        .map_err(|e| e.to_string())
}

/// 检查 Skills 更新
#[tauri::command]
pub async fn check_skill_updates(
    service: State<'_, SkillServiceState>,
    app_state: State<'_, AppState>,
) -> Result<Vec<SkillUpdateInfo>, String> {
    service
        .0
        .check_updates(&app_state.db)
        .await
        .map_err(|e| e.to_string())
}

/// 更新单个 Skill
#[tauri::command]
pub async fn update_skill(
    id: String,
    service: State<'_, SkillServiceState>,
    app_state: State<'_, AppState>,
) -> Result<InstalledSkill, String> {
    service
        .0
        .update_skill(&app_state.db, &id)
        .await
        .map_err(|e| e.to_string())
}

/// 迁移 Skill 存储位置
#[tauri::command]
pub async fn migrate_skill_storage(
    target: SkillStorageLocation,
    app_state: State<'_, AppState>,
) -> Result<MigrationResult, String> {
    SkillService::migrate_storage(&app_state.db, target).map_err(|e| e.to_string())
}

/// 搜索 skills.sh 公共目录
#[tauri::command]
pub async fn search_skills_sh(query: String, limit: usize) -> Result<SkillsShSearchResult, String> {
    SkillService::search_skills_sh(&query, limit)
        .await
        .map_err(|e| e.to_string())
}

/// 读取 skills.sh 公开榜单
#[tauri::command]
pub async fn get_skills_sh_leaderboard(
    view: String,
    limit: usize,
) -> Result<SkillsShLeaderboardResult, String> {
    SkillService::get_skills_sh_leaderboard(&view, limit)
        .await
        .map_err(|e| e.to_string())
}

/// 读取 skills.sh 公开发布者页
#[tauri::command]
pub async fn get_skills_sh_publisher(owner: String) -> Result<SkillsShPublisherDetail, String> {
    SkillService::get_skills_sh_publisher(&owner)
        .await
        .map_err(|e| e.to_string())
}

/// 读取 skills.sh 公开仓库页
#[tauri::command]
pub async fn get_skills_sh_repository(
    owner: String,
    repository: String,
) -> Result<SkillsShRepositoryDetail, String> {
    SkillService::get_skills_sh_repository(&owner, &repository)
        .await
        .map_err(|e| e.to_string())
}

/// 读取 skills.sh 公开 Skill 详情页
#[tauri::command]
pub async fn get_skills_sh_detail(
    repo_owner: String,
    repo_name: String,
    skill_id: String,
) -> Result<SkillsShSkillDetail, String> {
    SkillService::get_skills_sh_detail(&repo_owner, &repo_name, &skill_id)
        .await
        .map_err(|e| e.to_string())
}

// ========== 兼容旧 API 的命令 ==========

/// 获取技能列表（兼容旧 API）
#[tauri::command]
pub async fn get_skills(
    service: State<'_, SkillServiceState>,
    app_state: State<'_, AppState>,
) -> Result<Vec<Skill>, String> {
    let repos = app_state.db.get_skill_repos().map_err(|e| e.to_string())?;
    service
        .0
        .list_skills(repos, &app_state.db)
        .await
        .map_err(|e| e.to_string())
}

/// 获取指定应用的技能列表（兼容旧 API）
#[tauri::command]
pub async fn get_skills_for_app(
    app: String,
    service: State<'_, SkillServiceState>,
    app_state: State<'_, AppState>,
) -> Result<Vec<Skill>, String> {
    // 新版本不再区分应用，统一返回所有技能
    let _ = parse_app_type(&app)?; // 验证 app 参数有效
    get_skills(service, app_state).await
}

/// 安装技能（兼容旧 API）
#[tauri::command]
pub async fn install_skill(
    directory: String,
    service: State<'_, SkillServiceState>,
    app_state: State<'_, AppState>,
) -> Result<bool, String> {
    install_skill_for_app("claude".to_string(), directory, service, app_state).await
}

/// 安装指定应用的技能（兼容旧 API）
#[tauri::command]
pub async fn install_skill_for_app(
    app: String,
    directory: String,
    service: State<'_, SkillServiceState>,
    app_state: State<'_, AppState>,
) -> Result<bool, String> {
    let app_type = parse_app_type(&app)?;

    // 先获取技能信息
    let repos = app_state.db.get_skill_repos().map_err(|e| e.to_string())?;
    let skills = service
        .0
        .discover_available(repos)
        .await
        .map_err(|e| e.to_string())?;

    let skill = skills
        .into_iter()
        .find(|s| {
            let install_name = std::path::Path::new(&s.directory)
                .file_name()
                .map(|n| n.to_string_lossy().to_string())
                .unwrap_or_else(|| s.directory.clone());
            install_name.eq_ignore_ascii_case(&directory)
                || s.directory.eq_ignore_ascii_case(&directory)
        })
        .ok_or_else(|| {
            format_skill_error(
                "SKILL_NOT_FOUND",
                &[("directory", &directory)],
                Some("checkRepoUrl"),
            )
        })?;

    service
        .0
        .install(&app_state.db, &skill, &app_type)
        .await
        .map_err(|e| e.to_string())?;

    Ok(true)
}

/// 卸载技能（兼容旧 API）
#[tauri::command]
pub fn uninstall_skill(
    directory: String,
    app_state: State<'_, AppState>,
) -> Result<SkillUninstallResult, String> {
    uninstall_skill_for_app("claude".to_string(), directory, app_state)
}

/// 卸载指定应用的技能（兼容旧 API）
#[tauri::command]
pub fn uninstall_skill_for_app(
    app: String,
    directory: String,
    app_state: State<'_, AppState>,
) -> Result<SkillUninstallResult, String> {
    let _ = parse_app_type(&app)?; // 验证参数

    // 通过 directory 找到对应的 skill id
    let skills = SkillService::get_all_installed(&app_state.db).map_err(|e| e.to_string())?;

    let skill = skills
        .into_iter()
        .find(|s| s.directory.eq_ignore_ascii_case(&directory))
        .ok_or_else(|| format!("未找到已安装的 Skill: {directory}"))?;

    SkillService::uninstall(&app_state.db, &skill.id).map_err(|e| e.to_string())
}

// ========== 仓库管理命令 ==========

/// 获取技能仓库列表
#[tauri::command]
pub fn get_skill_repos(app_state: State<'_, AppState>) -> Result<Vec<SkillRepo>, String> {
    app_state.db.get_skill_repos().map_err(|e| e.to_string())
}

/// 添加技能仓库
#[tauri::command]
pub fn add_skill_repo(repo: SkillRepo, app_state: State<'_, AppState>) -> Result<bool, String> {
    app_state
        .db
        .save_skill_repo(&repo)
        .map_err(|e| e.to_string())?;
    Ok(true)
}

/// 删除技能仓库
#[tauri::command]
pub fn remove_skill_repo(
    owner: String,
    name: String,
    app_state: State<'_, AppState>,
) -> Result<bool, String> {
    app_state
        .db
        .delete_skill_repo(&owner, &name)
        .map_err(|e| e.to_string())?;
    Ok(true)
}

/// 从 ZIP 文件安装 Skills
#[tauri::command]
pub fn install_skills_from_zip(
    file_path: String,
    current_app: String,
    app_state: State<'_, AppState>,
) -> Result<Vec<InstalledSkill>, String> {
    let app_type = parse_app_type(&current_app)?;
    let path = std::path::Path::new(&file_path);

    SkillService::install_from_zip(&app_state.db, path, &app_type).map_err(|e| e.to_string())
}
