use serde_json::Value;
use std::fs;
use std::path::PathBuf;
use std::sync::{OnceLock, RwLock};
use tauri_plugin_store::StoreExt;

use crate::error::AppError;

/// Store 中的键名
const STORE_KEY_APP_CONFIG_DIR: &str = "app_config_dir_override";
/// Marks that the pre-rebrand Store has already been inspected. Without this
/// marker, clearing the override in Agent Switch would resurrect the old value
/// on every launch.
const STORE_KEY_LEGACY_PATHS_MIGRATED: &str = "legacy_cc_switch_paths_migrated";
const LEGACY_BUNDLE_IDENTIFIER: &str = "com.ccswitch.desktop";
const STORE_FILE_NAME: &str = "app_paths.json";

/// 缓存当前的 app_config_dir 覆盖路径，避免存储 AppHandle
static APP_CONFIG_DIR_OVERRIDE: OnceLock<RwLock<Option<PathBuf>>> = OnceLock::new();

fn override_cache() -> &'static RwLock<Option<PathBuf>> {
    APP_CONFIG_DIR_OVERRIDE.get_or_init(|| RwLock::new(None))
}

fn update_cached_override(value: Option<PathBuf>) {
    if let Ok(mut guard) = override_cache().write() {
        *guard = value;
    }
}

/// 获取缓存中的 app_config_dir 覆盖路径
pub fn get_app_config_dir_override() -> Option<PathBuf> {
    override_cache().read().ok()?.clone()
}

fn parse_override_value(value: Option<Value>, source: &str) -> Option<PathBuf> {
    match value {
        Some(Value::String(path_str)) => {
            let path_str = path_str.trim();
            if path_str.is_empty() {
                return None;
            }

            let path = resolve_path(path_str);

            if !path.exists() {
                log::warn!("{source} 中配置的 app_config_dir 不存在: {path:?}");
                return None;
            }

            log::info!("使用 {source} 中的 app_config_dir: {path:?}");
            Some(path)
        }
        Some(_) => {
            log::warn!("{source} 中的 {STORE_KEY_APP_CONFIG_DIR} 类型不正确，应为字符串");
            None
        }
        None => None,
    }
}

fn legacy_store_path() -> Option<PathBuf> {
    dirs::data_dir().map(|dir| legacy_store_path_from_data_dir(&dir))
}

fn legacy_store_path_from_data_dir(data_dir: &std::path::Path) -> PathBuf {
    data_dir
        .join(LEGACY_BUNDLE_IDENTIFIER)
        .join(STORE_FILE_NAME)
}

fn read_override_from_legacy_store_file(
    path: &std::path::Path,
) -> Result<Option<PathBuf>, AppError> {
    let bytes = match fs::read(path) {
        Ok(bytes) => bytes,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(e) => {
            log::warn!("读取旧版 Store {} 失败: {e}", path.display());
            return Err(AppError::io(path, e));
        }
    };
    let object = match serde_json::from_slice::<serde_json::Map<String, Value>>(&bytes) {
        Ok(object) => object,
        Err(e) => {
            log::warn!("解析旧版 Store {} 失败: {e}", path.display());
            return Err(AppError::json(path, e));
        }
    };
    let Some(value) = object.get(STORE_KEY_APP_CONFIG_DIR).cloned() else {
        return Ok(None);
    };
    parse_override_value(Some(value), "旧版 Store")
        .map(Some)
        .ok_or_else(|| {
            AppError::Message(
                "旧版 Store 中的 app_config_dir 当前不可用；将在下次启动时重试".to_string(),
            )
        })
}

/// 从 Store 刷新 app_config_dir 覆盖值。启动阶段使用此版本，确保明确
/// 配置但暂时不可用的目录不会被静默替换为默认目录。
pub fn refresh_app_config_dir_override_checked(
    app: &tauri::AppHandle,
) -> Result<Option<PathBuf>, AppError> {
    let store = app
        .store_builder(STORE_FILE_NAME)
        .build()
        .map_err(|e| AppError::Message(format!("无法创建 Store: {e}")))?;

    // An explicit value in the new Store is always authoritative, even when
    // it currently points to a missing directory.
    if let Some(raw_value) = store.get(STORE_KEY_APP_CONFIG_DIR) {
        let value = parse_override_value(Some(raw_value), "Store").ok_or_else(|| {
            AppError::Message(
                "Store 中配置的 app_config_dir 当前不可用；请恢复该目录后重试".to_string(),
            )
        })?;
        let value = Some(value);
        update_cached_override(value.clone());
        return Ok(value);
    }

    let already_migrated = store
        .get(STORE_KEY_LEGACY_PATHS_MIGRATED)
        .and_then(|value| value.as_bool())
        .unwrap_or(false);
    let value = if already_migrated {
        None
    } else {
        let legacy_result = legacy_store_path()
            .as_deref()
            .map(read_override_from_legacy_store_file)
            .unwrap_or(Ok(None));
        match legacy_result {
            Ok(legacy_value) => {
                if let Some(path) = &legacy_value {
                    store.set(
                        STORE_KEY_APP_CONFIG_DIR,
                        Value::String(path.to_string_lossy().into_owned()),
                    );
                    log::info!("已从旧 bundle Store 迁移 app_config_dir: {path:?}");
                }
                store.set(STORE_KEY_LEGACY_PATHS_MIGRATED, Value::Bool(true));
                if let Err(e) = store.save() {
                    log::warn!("保存旧版 Store 迁移结果失败: {e}");
                }
                legacy_value
            }
            Err(e) => {
                // Do not write the completion marker. A removable drive or
                // temporarily unavailable network path may return next launch.
                log::warn!("旧版 Store 迁移暂未完成: {e}");
                update_cached_override(None);
                return Err(e);
            }
        }
    };
    update_cached_override(value.clone());
    Ok(value)
}

/// 非启动调用保留原有 Option 接口；错误会记录日志，但不会覆盖缓存为默认路径。
pub fn refresh_app_config_dir_override(app: &tauri::AppHandle) -> Option<PathBuf> {
    match refresh_app_config_dir_override_checked(app) {
        Ok(value) => value,
        Err(e) => {
            log::warn!("刷新 app_config_dir 失败: {e}");
            get_app_config_dir_override()
        }
    }
}

/// 写入 app_config_dir 到 Tauri Store
pub fn set_app_config_dir_to_store(
    app: &tauri::AppHandle,
    path: Option<&str>,
) -> Result<(), AppError> {
    let store = app
        .store_builder(STORE_FILE_NAME)
        .build()
        .map_err(|e| AppError::Message(format!("创建 Store 失败: {e}")))?;

    match path {
        Some(p) => {
            let trimmed = p.trim();
            if !trimmed.is_empty() {
                let resolved = resolve_path(trimmed);
                if !resolved.exists() {
                    return Err(AppError::InvalidInput(format!(
                        "app_config_dir 不存在: {}",
                        resolved.display()
                    )));
                }
                store.set(STORE_KEY_APP_CONFIG_DIR, Value::String(trimmed.to_string()));
                log::info!("已将 app_config_dir 写入 Store: {trimmed}");
            } else {
                store.delete(STORE_KEY_APP_CONFIG_DIR);
                log::info!("已从 Store 中删除 app_config_dir 配置");
            }
        }
        None => {
            store.delete(STORE_KEY_APP_CONFIG_DIR);
            log::info!("已从 Store 中删除 app_config_dir 配置");
        }
    }

    store.set(STORE_KEY_LEGACY_PATHS_MIGRATED, Value::Bool(true));

    store
        .save()
        .map_err(|e| AppError::Message(format!("保存 Store 失败: {e}")))?;

    refresh_app_config_dir_override(app);
    Ok(())
}

/// 解析路径，支持 ~ 开头的相对路径
fn resolve_path(raw: &str) -> PathBuf {
    if raw == "~" {
        if let Some(home) = dirs::home_dir() {
            return home;
        }
    } else if let Some(stripped) = raw.strip_prefix("~/") {
        if let Some(home) = dirs::home_dir() {
            return home.join(stripped);
        }
    } else if let Some(stripped) = raw.strip_prefix("~\\") {
        if let Some(home) = dirs::home_dir() {
            return home.join(stripped);
        }
    }

    PathBuf::from(raw)
}

/// 从旧的 settings.json 迁移 app_config_dir 到 Store
pub fn migrate_app_config_dir_from_settings(app: &tauri::AppHandle) -> Result<(), AppError> {
    // app_config_dir 已从 settings.json 移除，此函数保留但不再执行迁移
    // 如果用户在旧版本设置过 app_config_dir，需要在 Store 中手动配置
    log::info!("app_config_dir 迁移功能已移除，请在设置中重新配置");

    let _ = refresh_app_config_dir_override(app);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_override_from_legacy_store_file() {
        let dir = tempfile::tempdir().unwrap();
        let custom_dir = dir.path().join("custom-data");
        fs::create_dir_all(&custom_dir).unwrap();
        let store_path = dir.path().join(STORE_FILE_NAME);
        fs::write(
            &store_path,
            serde_json::to_vec(&serde_json::json!({
                (STORE_KEY_APP_CONFIG_DIR): custom_dir.to_string_lossy()
            }))
            .unwrap(),
        )
        .unwrap();

        assert_eq!(
            read_override_from_legacy_store_file(&store_path).unwrap(),
            Some(custom_dir)
        );
    }

    #[test]
    fn legacy_store_path_uses_pre_rebrand_bundle_identifier() {
        assert_eq!(
            legacy_store_path_from_data_dir(std::path::Path::new("/app-data")),
            PathBuf::from("/app-data/com.ccswitch.desktop/app_paths.json")
        );
    }

    #[test]
    fn ignores_missing_override_directory_from_legacy_store() {
        let dir = tempfile::tempdir().unwrap();
        let store_path = dir.path().join(STORE_FILE_NAME);
        fs::write(
            &store_path,
            serde_json::to_vec(&serde_json::json!({
                (STORE_KEY_APP_CONFIG_DIR): dir.path().join("missing").to_string_lossy()
            }))
            .unwrap(),
        )
        .unwrap();

        assert!(read_override_from_legacy_store_file(&store_path).is_err());
    }
}
