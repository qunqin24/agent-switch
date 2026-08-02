//! Local, official Models.dev metadata cache shared by model configuration and pricing.

use crate::error::AppError;
use chrono::Utc;
use reqwest::header::USER_AGENT;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::fs;
use std::path::PathBuf;
use std::time::Duration;

pub const MODELS_DEV_API_URL: &str = "https://models.dev/api.json";
pub const MODELS_DEV_MODELS_URL: &str = "https://models.dev/models.json";
const MAX_RESPONSE_BYTES: u64 = 32 * 1024 * 1024;
const REQUEST_TIMEOUT_SECS: u64 = 30;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ModelsDevCache {
    pub fetched_at: i64,
    pub api: Value,
    pub models: Value,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ModelsDevCacheStatus {
    pub auto_update: bool,
    pub interval_hours: u32,
    pub fetched_at: Option<i64>,
    pub due: bool,
    pub cache_path: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ModelsDevCacheRefreshResult {
    /// `updated`, `cached`, or `disabled`.
    pub outcome: String,
    pub status: ModelsDevCacheStatus,
}

fn cache_path() -> PathBuf {
    crate::config::get_app_config_dir()
        .join("cache")
        .join("models-dev.json")
}

fn interval_hours() -> u32 {
    let interval = crate::settings::get_settings().models_dev_update_interval_hours;
    match interval {
        6 | 24 | 168 => interval,
        _ => 24,
    }
}

fn is_due(cache: Option<&ModelsDevCache>, now: i64, interval: u32) -> bool {
    cache.is_none_or(|cache| now.saturating_sub(cache.fetched_at) >= i64::from(interval) * 60 * 60)
}

fn load_cache() -> Result<Option<ModelsDevCache>, AppError> {
    let path = cache_path();
    if !path.exists() {
        return Ok(None);
    }
    let content = fs::read(&path).map_err(|error| AppError::io(&path, error))?;
    let cache = serde_json::from_slice(&content).map_err(|error| AppError::json(&path, error))?;
    Ok(Some(cache))
}

fn write_cache(cache: &ModelsDevCache) -> Result<(), AppError> {
    let path = cache_path();
    let parent = path
        .parent()
        .ok_or_else(|| AppError::Message("Models.dev 缓存路径无父目录".to_string()))?;
    fs::create_dir_all(parent).map_err(|error| AppError::io(parent, error))?;

    let payload = serde_json::to_vec(cache).map_err(|source| AppError::JsonSerialize { source })?;
    let temporary = path.with_extension("json.tmp");
    fs::write(&temporary, payload).map_err(|error| AppError::io(&temporary, error))?;
    if let Err(error) = fs::rename(&temporary, &path) {
        // Windows cannot replace an existing target with rename. The temporary
        // file still makes the write durable before that platform fallback.
        if path.exists() {
            fs::remove_file(&path).map_err(|remove_error| AppError::io(&path, remove_error))?;
            fs::rename(&temporary, &path)
                .map_err(|rename_error| AppError::io(&path, rename_error))?;
        } else {
            return Err(AppError::io(&path, error));
        }
    }
    Ok(())
}

async fn fetch_json(url: &str) -> Result<Value, AppError> {
    let response = crate::proxy::http_client::get()
        .get(url)
        .header(USER_AGENT, "agentswitch")
        .timeout(Duration::from_secs(REQUEST_TIMEOUT_SECS))
        .send()
        .await
        .map_err(|error| AppError::Message(format!("Models.dev 请求失败: {error}")))?;
    if !response.status().is_success() {
        return Err(AppError::Message(format!(
            "Models.dev 返回 HTTP {}",
            response.status()
        )));
    }
    if response
        .content_length()
        .is_some_and(|length| length > MAX_RESPONSE_BYTES)
    {
        return Err(AppError::Message("Models.dev 响应超过 32 MiB".to_string()));
    }
    let bytes = response
        .bytes()
        .await
        .map_err(|error| AppError::Message(format!("读取 Models.dev 响应失败: {error}")))?;
    if bytes.len() as u64 > MAX_RESPONSE_BYTES {
        return Err(AppError::Message("Models.dev 响应超过 32 MiB".to_string()));
    }
    let value: Value = serde_json::from_slice(&bytes)
        .map_err(|error| AppError::Message(format!("解析 Models.dev 响应失败: {error}")))?;
    if !value.is_object() {
        return Err(AppError::Message(
            "Models.dev 响应不是 JSON 对象".to_string(),
        ));
    }
    Ok(value)
}

fn validate_models_index(value: &Value) -> Result<(), AppError> {
    let Some(models) = value.as_object() else {
        return Err(AppError::Message(
            "Models.dev models.json 不是 JSON 对象".to_string(),
        ));
    };
    let valid = models.iter().any(|(canonical_id, model)| {
        canonical_id.contains('/')
            && model
                .get("id")
                .and_then(Value::as_str)
                .is_some_and(|id| id == canonical_id)
    });
    if !valid {
        return Err(AppError::Message(
            "Models.dev models.json 缺少官方 provider/model 索引".to_string(),
        ));
    }
    Ok(())
}

fn validate_api_catalog(value: &Value) -> Result<(), AppError> {
    let valid = value.as_object().is_some_and(|providers| {
        providers.values().any(|provider| {
            provider
                .get("models")
                .and_then(Value::as_object)
                .is_some_and(|models| !models.is_empty())
        })
    });
    if !valid {
        return Err(AppError::Message(
            "Models.dev api.json 缺少供应商模型目录".to_string(),
        ));
    }
    Ok(())
}

pub fn get_models_dev_cache_status() -> Result<ModelsDevCacheStatus, AppError> {
    let cache = load_cache()?;
    let settings = crate::settings::get_settings();
    let interval = interval_hours();
    Ok(ModelsDevCacheStatus {
        auto_update: settings.models_dev_auto_update,
        interval_hours: interval,
        fetched_at: cache.as_ref().map(|cache| cache.fetched_at),
        due: settings.models_dev_auto_update
            && is_due(cache.as_ref(), Utc::now().timestamp(), interval),
        cache_path: cache_path().display().to_string(),
    })
}

pub async fn refresh_models_dev_cache(
    force: bool,
) -> Result<ModelsDevCacheRefreshResult, AppError> {
    let cache = load_cache()?;
    let settings = crate::settings::get_settings();
    let interval = interval_hours();
    let now = Utc::now().timestamp();

    if !force && !settings.models_dev_auto_update {
        return Ok(ModelsDevCacheRefreshResult {
            outcome: "disabled".to_string(),
            status: get_models_dev_cache_status()?,
        });
    }
    if !force && !is_due(cache.as_ref(), now, interval) {
        return Ok(ModelsDevCacheRefreshResult {
            outcome: "cached".to_string(),
            status: get_models_dev_cache_status()?,
        });
    }

    let (api, models) = tokio::try_join!(
        fetch_json(MODELS_DEV_API_URL),
        fetch_json(MODELS_DEV_MODELS_URL),
    )?;
    validate_api_catalog(&api)?;
    validate_models_index(&models)?;
    write_cache(&ModelsDevCache {
        fetched_at: now,
        api,
        models,
    })?;
    Ok(ModelsDevCacheRefreshResult {
        outcome: "updated".to_string(),
        status: get_models_dev_cache_status()?,
    })
}

/// Read the locally cached data. A first use initializes the cache; later
/// reads refresh only when the user's schedule says it is due. Failed refreshes
/// intentionally fall back to the last valid local snapshot.
pub async fn get_models_dev_cache_for_use() -> Result<ModelsDevCache, AppError> {
    let cached = load_cache()?;
    if cached.is_none() {
        refresh_models_dev_cache(true).await?;
        return load_cache()?
            .ok_or_else(|| AppError::Message("Models.dev 缓存初始化后仍不可用".to_string()));
    }

    if crate::settings::get_settings().models_dev_auto_update
        && is_due(cached.as_ref(), Utc::now().timestamp(), interval_hours())
    {
        match refresh_models_dev_cache(false).await {
            Ok(result) if result.outcome == "updated" => {
                return load_cache()?
                    .ok_or_else(|| AppError::Message("Models.dev 缓存更新后不可用".to_string()));
            }
            Ok(_) => {}
            Err(error) => {
                log::warn!("Models.dev 自动更新失败，继续使用本地缓存: {error}");
            }
        }
    }
    Ok(cached.expect("checked above"))
}

fn normalize_identifier(value: &str) -> String {
    value
        .trim()
        .to_lowercase()
        .replace(['@', '_', ' '], "-")
        .split('-')
        .filter(|part| !part.is_empty())
        .collect::<Vec<_>>()
        .join("-")
}

fn model_suffix(value: &str) -> String {
    normalize_identifier(value.rsplit_once('/').map_or(value, |(_, suffix)| suffix))
}

fn find_official_model_id(
    cache: &ModelsDevCache,
    model_id: &str,
    display_name: Option<&str>,
) -> Option<String> {
    let models = cache.models.as_object()?;
    let normalized_id = normalize_identifier(model_id);
    let id_matches: Vec<&String> = models
        .keys()
        .filter(|id| normalize_identifier(id) == normalized_id || model_suffix(id) == normalized_id)
        .collect();
    if id_matches.len() == 1 {
        return Some((*id_matches[0]).clone());
    }

    let normalized_name = display_name.map(normalize_identifier).unwrap_or_default();
    let name_matches: Vec<&String> = models
        .iter()
        .filter(|(_, model)| {
            !normalized_name.is_empty()
                && model
                    .get("name")
                    .and_then(Value::as_str)
                    .is_some_and(|name| normalize_identifier(name) == normalized_name)
        })
        .map(|(id, _)| id)
        .collect();
    (name_matches.len() == 1).then(|| (*name_matches[0]).clone())
}

pub async fn get_official_model_metadata(
    model_id: &str,
    display_name: Option<&str>,
) -> Result<Option<Value>, AppError> {
    let cache = get_models_dev_cache_for_use().await?;
    let Some(canonical_id) = find_official_model_id(&cache, model_id, display_name) else {
        return Ok(None);
    };
    let Some((provider_id, provider_model_id)) = canonical_id.split_once('/') else {
        return Ok(None);
    };
    Ok(cache
        .api
        .get(provider_id)
        .and_then(|provider| provider.get("models"))
        .and_then(|models| models.get(provider_model_id))
        .cloned())
}

pub async fn get_models_dev_api_catalog() -> Result<Value, AppError> {
    Ok(get_models_dev_cache_for_use().await?.api)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resolves_a_bare_model_id_through_the_official_index() {
        let cache = ModelsDevCache {
            fetched_at: 1,
            models: serde_json::json!({
                "zhipuai/glm-5.2": { "name": "GLM-5.2" },
            }),
            api: serde_json::json!({}),
        };
        assert_eq!(
            find_official_model_id(&cache, "glm-5.2", None).as_deref(),
            Some("zhipuai/glm-5.2")
        );
    }

    #[test]
    fn due_check_uses_the_user_selected_hours() {
        let cache = ModelsDevCache {
            fetched_at: 10,
            api: serde_json::json!({}),
            models: serde_json::json!({}),
        };
        assert!(!is_due(Some(&cache), 10 + 23 * 60 * 60, 24));
        assert!(is_due(Some(&cache), 10 + 24 * 60 * 60, 24));
    }

    #[test]
    fn validates_the_live_models_dev_index_shape() {
        assert!(validate_models_index(&serde_json::json!({
            "zhipuai/glm-5.2": {
                "id": "zhipuai/glm-5.2",
                "name": "GLM-5.2"
            }
        }))
        .is_ok());
        assert!(validate_models_index(&serde_json::json!({
            "data": [{ "id": "zhipuai/glm-5.2" }]
        }))
        .is_err());
    }

    #[test]
    fn validates_the_live_api_catalog_shape() {
        assert!(validate_api_catalog(&serde_json::json!({
            "zhipuai": { "models": { "glm-5.2": { "id": "glm-5.2" } } }
        }))
        .is_ok());
        assert!(validate_api_catalog(&serde_json::json!({ "data": [] })).is_err());
    }
}
