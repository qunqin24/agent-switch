use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};

use crate::error::AppError;

/// 应用数据目录名（~/.agentswitch）
const APP_DIR_NAME: &str = ".agentswitch";
/// 旧版（CC Switch 时代）数据目录名，仅用于一次性迁移
const LEGACY_APP_DIR_NAME: &str = ".cc-switch";
/// SQLite 数据库文件名
const DB_FILE_NAME: &str = "agentswitch.db";
/// 旧版数据库文件名，仅用于一次性迁移
const LEGACY_DB_FILE_NAME: &str = "cc-switch.db";

/// 获取用户主目录，带回退和日志
///
/// ## Windows 注意事项
///
/// - `dirs::home_dir()` 在 Windows 上使用 `SHGetKnownFolderPath(FOLDERID_Profile)`，
///   返回的是真实用户目录（类似 `C:\\Users\\Alice`），与 v3.10.2 行为一致。
/// - 不要直接使用 `HOME` 环境变量：它可能由 Git/Cygwin/MSYS 等第三方工具注入，
///   且不一定等于用户目录，可能导致 `.agentswitch/agentswitch.db` 路径变化，从而“看起来像数据丢失”。
///
/// ## 测试隔离
///
/// 为了让 Windows CI/本地测试能稳定隔离真实用户数据，可通过 `CC_SWITCH_TEST_HOME`
/// 显式覆盖 home dir（仅用于测试/调试场景）。
pub fn get_home_dir() -> PathBuf {
    if let Ok(home) = std::env::var("CC_SWITCH_TEST_HOME") {
        let trimmed = home.trim();
        if !trimmed.is_empty() {
            return PathBuf::from(trimmed);
        }
    }

    dirs::home_dir().unwrap_or_else(|| {
        log::warn!("无法获取用户主目录，回退到当前目录");
        PathBuf::from(".")
    })
}

/// 获取 Claude Code 配置目录路径
pub fn get_claude_config_dir() -> PathBuf {
    if let Some(custom) = crate::settings::get_claude_override_dir() {
        return custom;
    }

    get_home_dir().join(".claude")
}

/// 默认 Claude MCP 配置文件路径 (~/.claude.json)
pub fn get_default_claude_mcp_path() -> PathBuf {
    get_home_dir().join(".claude.json")
}

fn derive_mcp_path_from_override(dir: &Path) -> Option<PathBuf> {
    let file_name = dir
        .file_name()
        .map(|name| name.to_string_lossy().to_string())?
        .trim()
        .to_string();
    if file_name.is_empty() {
        return None;
    }
    let parent = dir.parent().unwrap_or_else(|| Path::new(""));
    Some(parent.join(format!("{file_name}.json")))
}

/// 获取 Claude MCP 配置文件路径，若设置了目录覆盖则与覆盖目录同级
pub fn get_claude_mcp_path() -> PathBuf {
    if let Some(custom_dir) = crate::settings::get_claude_override_dir() {
        if let Some(path) = derive_mcp_path_from_override(&custom_dir) {
            return path;
        }
    }
    get_default_claude_mcp_path()
}

/// 获取 Claude Code 主配置文件路径
pub fn get_claude_settings_path() -> PathBuf {
    let dir = get_claude_config_dir();
    let settings = dir.join("settings.json");
    if settings.exists() {
        return settings;
    }
    // 兼容旧版命名：若存在旧文件则继续使用
    let legacy = dir.join("claude.json");
    if legacy.exists() {
        return legacy;
    }
    // 默认新建：回落到标准文件名 settings.json（不再生成 claude.json）
    settings
}

/// 获取应用配置目录路径 (~/.agentswitch)
pub fn get_app_config_dir() -> PathBuf {
    if let Some(custom) = crate::app_store::get_app_config_dir_override() {
        return custom;
    }

    let default_dir = get_home_dir().join(APP_DIR_NAME);

    // 兼容 v3.10.3：当用户环境存在 `HOME` 且与真实用户目录不同，
    // v3.10.3 可能在 `HOME/.agentswitch/` 下创建/使用了数据库。
    // 这里仅在“默认位置没有数据库”时回退到旧位置，避免再次出现“供应商消失”问题，
    // 同时也避免新安装因为 `HOME` 被设置而写入非预期路径。
    #[cfg(windows)]
    {
        let default_db = default_dir.join(DB_FILE_NAME);
        if !default_db.exists() {
            if let Ok(home_env) = std::env::var("HOME") {
                let trimmed = home_env.trim();
                if !trimmed.is_empty() {
                    let legacy_dir = PathBuf::from(trimmed).join(APP_DIR_NAME);
                    if legacy_dir.join(DB_FILE_NAME).exists() {
                        log::info!(
                            "Detected v3.10.3 legacy database at {}, using it instead of {}",
                            legacy_dir.display(),
                            default_dir.display()
                        );
                        return legacy_dir;
                    }
                }
            }
        }
    }

    default_dir
}

/// 获取应用配置文件路径
pub fn get_app_config_path() -> PathBuf {
    get_app_config_dir().join("config.json")
}

/// 将旧版（CC Switch 时代）数据迁移到 Agent Switch 使用的路径。
///
/// 必须在应用启动早期、任何组件读写 `get_app_config_dir()` 之前调用。
/// 迁移在全部步骤成功前保留旧目录；失败会返回错误，由启动流程阻止创建空数据库。
pub fn migrate_legacy_app_data_dir() -> std::io::Result<()> {
    let home = get_home_dir();

    if let Some(custom_dir) = crate::app_store::get_app_config_dir_override() {
        // 自定义目录不会整体移动，但数据库文件名仍属于应用管理范围。
        migrate_legacy_database_file(&custom_dir)?;
        migrate_legacy_device_settings(&home)?;
        return Ok(());
    }

    let new_dir = home.join(APP_DIR_NAME);

    #[cfg(windows)]
    let home_env = std::env::var_os("HOME").map(PathBuf::from);
    #[cfg(not(windows))]
    let home_env: Option<PathBuf> = None;

    // Windows v3.10.3 may have used HOME instead of the real Known Folder.
    // Iterate instead of selecting only one source so a settings-only default
    // directory cannot hide a database in the divergent HOME directory.
    for legacy_dir in legacy_data_dir_candidates(&home, home_env.as_deref()) {
        if legacy_dir == new_dir || !has_primary_legacy_payload(&legacy_dir) {
            continue;
        }
        migrate_legacy_directory(&legacy_dir, &new_dir)?;
    }

    migrate_legacy_database_file(&new_dir)?;
    migrate_legacy_device_settings(&home)?;
    Ok(())
}

fn legacy_data_dir_candidates(home: &Path, home_env: Option<&Path>) -> Vec<PathBuf> {
    let mut candidates = vec![home.join(LEGACY_APP_DIR_NAME)];
    if let Some(home_env) = home_env.filter(|path| *path != home) {
        candidates.push(home_env.join(LEGACY_APP_DIR_NAME));
        // Compatibility with early Agent Switch builds that retained the
        // v3.10.3 HOME fallback after the rename.
        candidates.push(home_env.join(APP_DIR_NAME));
    }
    candidates
}

fn has_primary_legacy_payload(dir: &Path) -> bool {
    dir.join(LEGACY_DB_FILE_NAME).exists()
        || dir.join(DB_FILE_NAME).exists()
        || dir.join("config.json").exists()
}

fn migrate_legacy_database_file(dir: &Path) -> std::io::Result<()> {
    let old_db = dir.join(LEGACY_DB_FILE_NAME);
    let new_db = dir.join(DB_FILE_NAME);
    if old_db.exists() && !new_db.exists() {
        fs::rename(&old_db, &new_db).map_err(|e| {
            std::io::Error::new(
                e.kind(),
                format!(
                    "无法将旧数据库 {} 重命名为 {}: {e}",
                    old_db.display(),
                    new_db.display()
                ),
            )
        })?;
    }
    Ok(())
}

fn migrate_legacy_directory(from: &Path, to: &Path) -> std::io::Result<()> {
    if to.join(DB_FILE_NAME).exists() {
        log::warn!(
            "新目录 {} 已包含数据库，保留旧目录 {} 不做覆盖",
            to.display(),
            from.display()
        );
        return Ok(());
    }

    log::info!(
        "检测到旧版数据目录 {}，开始迁移到 {}",
        from.display(),
        to.display()
    );

    if !to.exists() {
        match fs::rename(from, to) {
            Ok(()) => {
                if let Err(rename_error) = migrate_legacy_database_file(to) {
                    return match fs::rename(to, from) {
                        Ok(()) => Err(rename_error),
                        Err(rollback_error) => Err(std::io::Error::new(
                            rename_error.kind(),
                            format!(
                                "{rename_error}; 同时无法回滚目录到 {}: {rollback_error}",
                                from.display()
                            ),
                        )),
                    };
                }
                log::info!("✓ 数据目录迁移完成: {}", to.display());
                return Ok(());
            }
            Err(e) => {
                log::warn!("直接重命名迁移数据目录失败（{e}），尝试安全复制…");
            }
        }

        let staging = to.with_file_name(format!(".agentswitch.migrating-{}", std::process::id()));
        if staging.exists() {
            return Err(std::io::Error::new(
                std::io::ErrorKind::AlreadyExists,
                format!("迁移暂存目录已存在: {}", staging.display()),
            ));
        }
        if let Err(e) = copy_dir_recursive_no_overwrite(from, &staging)
            .and_then(|_| migrate_legacy_database_file(&staging))
            .and_then(|_| fs::rename(&staging, to))
        {
            let _ = fs::remove_dir_all(&staging);
            return Err(e);
        }
    } else {
        // A crash log or another harmless early-start artifact may already
        // have created the new directory. Merge only missing files and keep
        // the source intact until the database rename succeeds.
        copy_dir_recursive_no_overwrite(from, to)?;
        migrate_legacy_database_file(to)?;
    }

    if let Err(e) = fs::remove_dir_all(from) {
        log::warn!("迁移完成，但清理旧目录失败: {e}（不影响使用，可手动删除）");
    }
    log::info!("✓ 数据目录迁移完成: {}", to.display());
    Ok(())
}

fn migrate_legacy_device_settings(home: &Path) -> std::io::Result<()> {
    let old_settings = home.join(LEGACY_APP_DIR_NAME).join("settings.json");
    let new_settings = home.join(APP_DIR_NAME).join("settings.json");
    if !old_settings.exists() || new_settings.exists() {
        return Ok(());
    }
    if let Some(parent) = new_settings.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::rename(&old_settings, &new_settings).map_err(|e| {
        std::io::Error::new(
            e.kind(),
            format!(
                "无法迁移设备设置 {} 到 {}: {e}",
                old_settings.display(),
                new_settings.display()
            ),
        )
    })
}

fn copy_dir_recursive_no_overwrite(from: &Path, to: &Path) -> std::io::Result<()> {
    let created = !to.exists();
    if created {
        fs::create_dir_all(to)?;
    }
    for entry in fs::read_dir(from)? {
        let entry = entry?;
        let source = entry.path();
        let dest = to.join(entry.file_name());
        let file_type = entry.file_type()?;
        if file_type.is_dir() {
            copy_dir_recursive_no_overwrite(&source, &dest)?;
        } else if file_type.is_symlink() {
            if dest.exists() || dest.symlink_metadata().is_ok() {
                continue;
            }
            let target = fs::read_link(&source)?;
            #[cfg(unix)]
            std::os::unix::fs::symlink(target, &dest)?;
            #[cfg(windows)]
            {
                if source.is_dir() {
                    std::os::windows::fs::symlink_dir(target, &dest)?;
                } else {
                    std::os::windows::fs::symlink_file(target, &dest)?;
                }
            }
        } else if !dest.exists() {
            fs::copy(&source, &dest)?;
        }
    }
    if created {
        fs::set_permissions(to, fs::metadata(from)?.permissions())?;
    }
    Ok(())
}

/// 清理供应商名称，确保文件名安全
#[allow(dead_code)]
pub fn sanitize_provider_name(name: &str) -> String {
    name.chars()
        .map(|c| match c {
            '<' | '>' | ':' | '"' | '/' | '\\' | '|' | '?' | '*' => '-',
            _ => c,
        })
        .collect::<String>()
        .to_lowercase()
}

/// 获取供应商配置文件路径
#[allow(dead_code)]
pub fn get_provider_config_path(provider_id: &str, provider_name: Option<&str>) -> PathBuf {
    let base_name = provider_name
        .map(sanitize_provider_name)
        .unwrap_or_else(|| sanitize_provider_name(provider_id));

    get_claude_config_dir().join(format!("settings-{base_name}.json"))
}

/// 读取 JSON 配置文件
pub fn read_json_file<T: for<'a> Deserialize<'a>>(path: &Path) -> Result<T, AppError> {
    if !path.exists() {
        return Err(AppError::Config(format!("文件不存在: {}", path.display())));
    }

    let content = fs::read_to_string(path).map_err(|e| AppError::io(path, e))?;

    serde_json::from_str(&content).map_err(|e| AppError::json(path, e))
}

/// 递归排序 JSON 对象的键（按字母顺序），确保序列化输出是确定性的
fn sort_json_keys(value: &Value) -> Value {
    match value {
        Value::Object(map) => {
            let mut sorted_map = Map::new();
            let mut keys: Vec<_> = map.keys().collect();
            keys.sort();
            for key in keys {
                sorted_map.insert(key.clone(), sort_json_keys(&map[key]));
            }
            Value::Object(sorted_map)
        }
        Value::Array(arr) => Value::Array(arr.iter().map(sort_json_keys).collect()),
        other => other.clone(),
    }
}

/// 写入 JSON 配置文件（键按字母排序，确保确定性输出）
pub fn write_json_file<T: Serialize>(path: &Path, data: &T) -> Result<(), AppError> {
    // 确保目录存在
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|e| AppError::io(parent, e))?;
    }

    let value = serde_json::to_value(data).map_err(|e| AppError::JsonSerialize { source: e })?;
    let sorted_value = sort_json_keys(&value);
    let json = serde_json::to_string_pretty(&sorted_value)
        .map_err(|e| AppError::JsonSerialize { source: e })?;

    atomic_write(path, json.as_bytes())
}

/// 原子写入文本文件（用于 TOML/纯文本）
pub fn write_text_file(path: &Path, data: &str) -> Result<(), AppError> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|e| AppError::io(parent, e))?;
    }
    atomic_write(path, data.as_bytes())
}

/// 原子写入：写入临时文件后 rename 替换，避免半写状态
pub fn atomic_write(path: &Path, data: &[u8]) -> Result<(), AppError> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|e| AppError::io(parent, e))?;
    }

    let parent = path
        .parent()
        .ok_or_else(|| AppError::Config("无效的路径".to_string()))?;
    let mut tmp = parent.to_path_buf();
    let file_name = path
        .file_name()
        .ok_or_else(|| AppError::Config("无效的文件名".to_string()))?
        .to_string_lossy()
        .to_string();
    let ts = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    tmp.push(format!("{file_name}.tmp.{ts}"));

    {
        let mut f = fs::File::create(&tmp).map_err(|e| AppError::io(&tmp, e))?;
        f.write_all(data).map_err(|e| AppError::io(&tmp, e))?;
        f.flush().map_err(|e| AppError::io(&tmp, e))?;
    }

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        if let Ok(meta) = fs::metadata(path) {
            let perm = meta.permissions().mode();
            let _ = fs::set_permissions(&tmp, fs::Permissions::from_mode(perm));
        }
    }

    #[cfg(windows)]
    {
        // Windows 上 rename 目标存在会失败，先移除再重命名（尽量接近原子性）
        if path.exists() {
            let _ = fs::remove_file(path);
        }
        fs::rename(&tmp, path).map_err(|e| AppError::IoContext {
            context: format!("原子替换失败: {} -> {}", tmp.display(), path.display()),
            source: e,
        })?;
    }

    #[cfg(not(windows))]
    {
        fs::rename(&tmp, path).map_err(|e| AppError::IoContext {
            context: format!("原子替换失败: {} -> {}", tmp.display(), path.display()),
            source: e,
        })?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{Mutex, OnceLock};

    fn test_guard() -> std::sync::MutexGuard<'static, ()> {
        static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
        LOCK.get_or_init(|| Mutex::new(()))
            .lock()
            .unwrap_or_else(|err| err.into_inner())
    }

    /// 在隔离的临时 HOME 目录下运行测试，运行前后保存/恢复 `CC_SWITCH_TEST_HOME`。
    fn with_test_home<T>(test_fn: impl FnOnce(&Path) -> T) -> T {
        let _guard = test_guard();
        let tmp = tempfile::tempdir().unwrap();
        let old_test_home = std::env::var_os("CC_SWITCH_TEST_HOME");
        std::env::set_var("CC_SWITCH_TEST_HOME", tmp.path());
        let result = test_fn(tmp.path());
        match old_test_home {
            Some(value) => std::env::set_var("CC_SWITCH_TEST_HOME", value),
            None => std::env::remove_var("CC_SWITCH_TEST_HOME"),
        }
        result
    }

    #[test]
    fn migrate_legacy_app_data_dir_moves_db_and_renames_file() {
        with_test_home(|home| {
            let legacy_dir = home.join(LEGACY_APP_DIR_NAME);
            fs::create_dir_all(legacy_dir.join("backups")).unwrap();
            fs::write(legacy_dir.join(LEGACY_DB_FILE_NAME), b"db-bytes").unwrap();
            fs::write(legacy_dir.join("settings.json"), b"{}").unwrap();
            fs::write(legacy_dir.join("backups").join("snap.sql"), b"snap").unwrap();

            migrate_legacy_app_data_dir().unwrap();

            let new_dir = home.join(APP_DIR_NAME);
            assert!(!legacy_dir.exists(), "旧目录应被迁移后移除");
            assert!(
                new_dir.join(DB_FILE_NAME).exists(),
                "数据库应重命名为新文件名"
            );
            assert!(!new_dir.join(LEGACY_DB_FILE_NAME).exists());
            assert_eq!(fs::read(new_dir.join(DB_FILE_NAME)).unwrap(), b"db-bytes");
            assert!(new_dir.join("settings.json").exists());
            assert!(new_dir.join("backups").join("snap.sql").exists());
        });
    }

    #[test]
    fn migrate_legacy_app_data_dir_skips_when_new_dir_exists() {
        with_test_home(|home| {
            let legacy_dir = home.join(LEGACY_APP_DIR_NAME);
            fs::create_dir_all(&legacy_dir).unwrap();
            fs::write(legacy_dir.join(LEGACY_DB_FILE_NAME), b"old").unwrap();

            let new_dir = home.join(APP_DIR_NAME);
            fs::create_dir_all(&new_dir).unwrap();
            fs::write(new_dir.join(DB_FILE_NAME), b"new").unwrap();

            migrate_legacy_app_data_dir().unwrap();

            // 新目录已存在，旧目录应保持原样，不做任何迁移
            assert!(legacy_dir.exists());
            assert_eq!(fs::read(new_dir.join(DB_FILE_NAME)).unwrap(), b"new");
        });
    }

    #[test]
    fn migrate_legacy_app_data_dir_noop_on_fresh_install() {
        with_test_home(|home| {
            migrate_legacy_app_data_dir().unwrap();
            assert!(!home.join(APP_DIR_NAME).exists());
            assert!(!home.join(LEGACY_APP_DIR_NAME).exists());
        });
    }

    #[test]
    fn migrate_legacy_app_data_dir_ignores_empty_legacy_dir() {
        with_test_home(|home| {
            // 旧目录存在但既没有数据库也没有 config.json（例如残留的空目录）
            fs::create_dir_all(home.join(LEGACY_APP_DIR_NAME)).unwrap();

            migrate_legacy_app_data_dir().unwrap();

            assert!(!home.join(APP_DIR_NAME).exists());
            assert!(home.join(LEGACY_APP_DIR_NAME).exists());
        });
    }

    #[test]
    fn migrate_legacy_app_data_dir_merges_when_new_dir_only_has_crash_log() {
        with_test_home(|home| {
            let legacy_dir = home.join(LEGACY_APP_DIR_NAME);
            fs::create_dir_all(&legacy_dir).unwrap();
            fs::write(legacy_dir.join(LEGACY_DB_FILE_NAME), b"db-bytes").unwrap();
            fs::write(legacy_dir.join("settings.json"), b"settings").unwrap();

            let new_dir = home.join(APP_DIR_NAME);
            fs::create_dir_all(&new_dir).unwrap();
            fs::write(new_dir.join("crash.log"), b"early crash").unwrap();

            migrate_legacy_app_data_dir().unwrap();

            assert!(!legacy_dir.exists());
            assert_eq!(fs::read(new_dir.join(DB_FILE_NAME)).unwrap(), b"db-bytes");
            assert_eq!(
                fs::read(new_dir.join("settings.json")).unwrap(),
                b"settings"
            );
            assert_eq!(fs::read(new_dir.join("crash.log")).unwrap(), b"early crash");
        });
    }

    #[test]
    fn migrate_legacy_database_file_renames_database_in_custom_directory() {
        let dir = tempfile::tempdir().unwrap();
        fs::write(dir.path().join(LEGACY_DB_FILE_NAME), b"custom-db").unwrap();

        migrate_legacy_database_file(dir.path()).unwrap();

        assert!(!dir.path().join(LEGACY_DB_FILE_NAME).exists());
        assert_eq!(
            fs::read(dir.path().join(DB_FILE_NAME)).unwrap(),
            b"custom-db"
        );
    }

    #[test]
    fn migrate_legacy_device_settings_handles_custom_data_directory_case() {
        let dir = tempfile::tempdir().unwrap();
        let old_dir = dir.path().join(LEGACY_APP_DIR_NAME);
        fs::create_dir_all(&old_dir).unwrap();
        fs::write(old_dir.join("settings.json"), b"device-settings").unwrap();

        migrate_legacy_device_settings(dir.path()).unwrap();

        assert!(!old_dir.join("settings.json").exists());
        assert_eq!(
            fs::read(dir.path().join(APP_DIR_NAME).join("settings.json")).unwrap(),
            b"device-settings"
        );
    }

    #[test]
    fn legacy_candidates_include_divergent_windows_home_paths() {
        let real_home = Path::new("/real-home");
        let env_home = Path::new("/env-home");
        assert_eq!(
            legacy_data_dir_candidates(real_home, Some(env_home)),
            vec![
                real_home.join(LEGACY_APP_DIR_NAME),
                env_home.join(LEGACY_APP_DIR_NAME),
                env_home.join(APP_DIR_NAME),
            ]
        );
    }

    #[test]
    fn derive_mcp_path_from_override_preserves_folder_name() {
        let override_dir = PathBuf::from("/tmp/profile/.claude");
        let derived = derive_mcp_path_from_override(&override_dir)
            .expect("should derive path for nested dir");
        assert_eq!(derived, PathBuf::from("/tmp/profile/.claude.json"));
    }

    #[test]
    fn derive_mcp_path_from_override_handles_non_hidden_folder() {
        let override_dir = PathBuf::from("/data/claude-config");
        let derived = derive_mcp_path_from_override(&override_dir)
            .expect("should derive path for standard dir");
        assert_eq!(derived, PathBuf::from("/data/claude-config.json"));
    }

    #[test]
    fn derive_mcp_path_from_override_supports_relative_rootless_dir() {
        let override_dir = PathBuf::from("claude");
        let derived = derive_mcp_path_from_override(&override_dir)
            .expect("should derive path for single segment");
        assert_eq!(derived, PathBuf::from("claude.json"));
    }

    #[test]
    fn derive_mcp_path_from_root_like_dir_returns_none() {
        let override_dir = PathBuf::from("/");
        assert!(derive_mcp_path_from_override(&override_dir).is_none());
    }

    #[test]
    fn sort_json_keys_sorts_top_level_object() {
        let input = serde_json::json!({
            "z": 1,
            "a": 2,
            "m": 3,
        });
        let sorted = sort_json_keys(&input);
        let serialized = serde_json::to_string(&sorted).unwrap();
        assert_eq!(serialized, r#"{"a":2,"m":3,"z":1}"#);
    }

    #[test]
    fn sort_json_keys_recurses_into_nested_objects() {
        let input = serde_json::json!({
            "outer_b": {"z": 1, "a": 2},
            "outer_a": {"y": 3, "b": 4},
        });
        let sorted = sort_json_keys(&input);
        let serialized = serde_json::to_string(&sorted).unwrap();
        assert_eq!(
            serialized,
            r#"{"outer_a":{"b":4,"y":3},"outer_b":{"a":2,"z":1}}"#
        );
    }

    #[test]
    fn sort_json_keys_preserves_array_order() {
        let input = serde_json::json!([3, 1, 2]);
        let sorted = sort_json_keys(&input);
        let serialized = serde_json::to_string(&sorted).unwrap();
        assert_eq!(serialized, "[3,1,2]");
    }

    #[test]
    fn sort_json_keys_sorts_objects_inside_arrays_but_keeps_array_order() {
        let input = serde_json::json!([
            {"z": 1, "a": 2},
            {"y": 3, "b": 4},
        ]);
        let sorted = sort_json_keys(&input);
        let serialized = serde_json::to_string(&sorted).unwrap();
        assert_eq!(serialized, r#"[{"a":2,"z":1},{"b":4,"y":3}]"#);
    }

    #[test]
    fn sort_json_keys_passes_through_primitives() {
        let cases = vec![
            serde_json::json!("hello"),
            serde_json::json!(42),
            serde_json::json!(3.5),
            serde_json::json!(true),
            serde_json::json!(null),
        ];
        for value in cases {
            let sorted = sort_json_keys(&value);
            assert_eq!(sorted, value);
        }
    }

    #[test]
    fn sort_json_keys_handles_empty_collections() {
        let empty_obj = serde_json::json!({});
        assert_eq!(
            serde_json::to_string(&sort_json_keys(&empty_obj)).unwrap(),
            "{}"
        );

        let empty_arr = serde_json::json!([]);
        assert_eq!(
            serde_json::to_string(&sort_json_keys(&empty_arr)).unwrap(),
            "[]"
        );
    }

    #[test]
    fn sort_json_keys_produces_identical_output_for_different_insertion_orders() {
        // 核心保证：同一逻辑配置无论键的插入顺序如何，写出的字节序列必须一致。
        let mut a = Map::new();
        a.insert("env".to_string(), serde_json::json!({"PATH": "/usr/bin"}));
        a.insert("model".to_string(), serde_json::json!("claude-sonnet-4-5"));
        a.insert("permissions".to_string(), serde_json::json!({"allow": []}));

        let mut b = Map::new();
        b.insert("permissions".to_string(), serde_json::json!({"allow": []}));
        b.insert("model".to_string(), serde_json::json!("claude-sonnet-4-5"));
        b.insert("env".to_string(), serde_json::json!({"PATH": "/usr/bin"}));

        let sorted_a = sort_json_keys(&Value::Object(a));
        let sorted_b = sort_json_keys(&Value::Object(b));

        assert_eq!(
            serde_json::to_string(&sorted_a).unwrap(),
            serde_json::to_string(&sorted_b).unwrap(),
        );
    }
}

/// 复制文件
pub fn copy_file(from: &Path, to: &Path) -> Result<(), AppError> {
    fs::copy(from, to).map_err(|e| AppError::IoContext {
        context: format!("复制文件失败 ({} -> {})", from.display(), to.display()),
        source: e,
    })?;
    Ok(())
}

/// 删除文件
pub fn delete_file(path: &Path) -> Result<(), AppError> {
    if path.exists() {
        fs::remove_file(path).map_err(|e| AppError::io(path, e))?;
    }
    Ok(())
}

/// 检查 Claude Code 配置状态
#[derive(Serialize, Deserialize)]
pub struct ConfigStatus {
    pub exists: bool,
    pub path: String,
}

/// 获取 Claude Code 配置状态
pub fn get_claude_config_status() -> ConfigStatus {
    let path = get_claude_settings_path();
    ConfigStatus {
        exists: path.exists(),
        path: path.to_string_lossy().to_string(),
    }
}
