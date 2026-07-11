use crate::error::AppError;

/// Returns one official model record from the local Models.dev cache.
#[tauri::command]
pub async fn get_models_dev_model_metadata(
    model_id: String,
    display_name: Option<String>,
) -> Result<Option<serde_json::Value>, AppError> {
    crate::services::models_dev_cache::get_official_model_metadata(
        &model_id,
        display_name.as_deref(),
    )
    .await
}

/// Used by the pricing picker. The payload comes from the local cache, not a
/// browser-side network request.
#[tauri::command]
pub async fn get_models_dev_api_catalog() -> Result<serde_json::Value, AppError> {
    crate::services::models_dev_cache::get_models_dev_api_catalog().await
}

#[tauri::command]
pub async fn refresh_models_dev_cache(
    force: bool,
) -> Result<crate::services::models_dev_cache::ModelsDevCacheRefreshResult, AppError> {
    crate::services::models_dev_cache::refresh_models_dev_cache(force).await
}

#[tauri::command]
pub fn get_models_dev_cache_status(
) -> Result<crate::services::models_dev_cache::ModelsDevCacheStatus, AppError> {
    crate::services::models_dev_cache::get_models_dev_cache_status()
}
