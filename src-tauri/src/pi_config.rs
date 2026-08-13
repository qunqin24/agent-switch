//! Pi provider configuration management.
//!
//! Pi stores custom provider/model definitions in `~/.pi/agent/models.json`.
//! Providers are additive: Agent Switch owns only the selected entry under
//! `providers` and preserves every unrelated top-level field and provider.

use crate::config::{atomic_write, get_home_dir};
use crate::error::AppError;
use serde_json::{json, Map, Value};
use std::ffi::OsString;
use std::fs;
use std::path::Path;
use std::path::PathBuf;
use std::sync::{Mutex, OnceLock};

const SUPPORTED_APIS: &[&str] = &[
    "openai-completions",
    "openai-responses",
    "anthropic-messages",
    "google-generative-ai",
];

// Pi ships model catalogs for these API-key providers. Their models.json
// entries may therefore be partial overrides (for example, only `apiKey` or
// `baseUrl`) and must not be validated as complete custom providers.
const BUILTIN_PROVIDER_IDS: &[&str] = &[
    "anthropic",
    "ant-ling",
    "azure-openai-responses",
    "openai",
    "deepseek",
    "nvidia",
    "google",
    "amazon-bedrock",
    "mistral",
    "groq",
    "cerebras",
    "cloudflare-ai-gateway",
    "cloudflare-workers-ai",
    "xai",
    "openrouter",
    "vercel-ai-gateway",
    "zai",
    "zai-coding-cn",
    "opencode",
    "opencode-go",
    "radius",
    "huggingface",
    "fireworks",
    "together",
    "baseten",
    "kimi-coding",
    "minimax",
    "minimax-cn",
    "qwen-token-plan",
    "qwen-token-plan-individual",
    "qwen-token-plan-cn",
    "xiaomi",
    "xiaomi-token-plan-cn",
    "xiaomi-token-plan-ams",
    "xiaomi-token-plan-sgp",
];

fn write_lock() -> &'static Mutex<()> {
    static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
    LOCK.get_or_init(|| Mutex::new(()))
}

pub fn get_pi_dir() -> PathBuf {
    select_pi_dir(
        crate::settings::get_pi_override_dir(),
        std::env::var_os("PI_CODING_AGENT_DIR"),
    )
    .unwrap_or_else(|| get_home_dir().join(".pi").join("agent"))
}

fn select_pi_dir(override_dir: Option<PathBuf>, env_dir: Option<OsString>) -> Option<PathBuf> {
    override_dir
        .or_else(|| env_dir.and_then(|raw| resolve_pi_configured_path(&raw.to_string_lossy())))
}

pub fn get_pi_models_path() -> PathBuf {
    get_pi_dir().join("models.json")
}

/// Resolve the Pi session directory that Agent Switch can safely scan.
///
/// Pi resolves relative `sessionDir` values from the CLI process's launch cwd.
/// Agent Switch does not know that cwd, so only absolute/`~` paths can be used
/// here unless the user supplies the explicit absolute override in Settings.
pub fn get_pi_sessions_dir() -> PathBuf {
    let agent_dir = get_pi_dir();

    if let Some(path) = crate::settings::get_pi_session_override_dir() {
        return path;
    }

    if let Some(raw) = std::env::var_os("PI_CODING_AGENT_SESSION_DIR") {
        if let Some(path) = resolve_pi_session_path(&raw.to_string_lossy(), "environment") {
            return path;
        }
    }

    let settings_path = agent_dir.join("settings.json");
    if let Ok(content) = fs::read_to_string(&settings_path) {
        if let Ok(settings) = serde_json::from_str::<Value>(&content) {
            if let Some(raw) = settings.get("sessionDir").and_then(Value::as_str) {
                if let Some(path) = resolve_pi_session_path(raw, "global settings.json") {
                    return path;
                }
            }
        }
    }

    agent_dir.join("sessions")
}

fn resolve_pi_session_path(raw: &str, source: &str) -> Option<PathBuf> {
    let path = resolve_pi_configured_path(raw)?;
    let raw_path = Path::new(raw.trim());
    let uses_home =
        raw.trim() == "~" || raw.trim().starts_with("~/") || raw.trim().starts_with("~\\");
    if raw_path.is_absolute() || uses_home {
        return Some(path);
    }

    log::warn!(
        "Ignoring relative Pi session directory from {source}: {raw}. Its launch cwd is unavailable to Agent Switch; configure the resolved absolute directory in Settings instead."
    );
    None
}

pub(crate) fn resolve_pi_configured_path(raw: &str) -> Option<PathBuf> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return None;
    }

    if trimmed == "~" {
        return dirs::home_dir();
    }
    if let Some(rest) = trimmed
        .strip_prefix("~/")
        .or_else(|| trimmed.strip_prefix("~\\"))
    {
        return dirs::home_dir().map(|home| home.join(rest));
    }

    let path = Path::new(trimmed);
    if path.is_absolute() {
        Some(path.to_path_buf())
    } else {
        // Pi leaves relative sessionDir values relative to the process cwd.
        std::env::current_dir().ok().map(|cwd| cwd.join(path))
    }
}

fn default_config() -> Value {
    json!({ "providers": {} })
}

pub fn read_pi_config() -> Result<Value, AppError> {
    let path = get_pi_models_path();
    if !path.exists() {
        return Ok(default_config());
    }
    secure_pi_config_permissions(&path)?;

    let content = fs::read_to_string(&path).map_err(|e| AppError::io(&path, e))?;
    let value: Value = serde_json::from_str(&content).map_err(|e| AppError::json(&path, e))?;
    if !value.is_object() {
        return Err(AppError::Config(
            "Pi models.json root must be a JSON object".to_string(),
        ));
    }
    Ok(value)
}

fn write_pi_config(value: &Value) -> Result<(), AppError> {
    let path = get_pi_models_path();
    let content = serde_json::to_string_pretty(value)
        .map_err(|e| AppError::Config(format!("Failed to serialize Pi models.json: {e}")))?;
    if path.exists() {
        secure_pi_config_permissions(&path)?;
        crate::services::ConfigService::create_backup(&path)?;
    }
    atomic_write(&path, format!("{content}\n").as_bytes())?;
    secure_pi_config_permissions(&path)
}

#[cfg(unix)]
fn secure_pi_config_permissions(path: &Path) -> Result<(), AppError> {
    use std::os::unix::fs::PermissionsExt;

    fs::set_permissions(path, fs::Permissions::from_mode(0o600)).map_err(|e| AppError::io(path, e))
}

#[cfg(not(unix))]
fn secure_pi_config_permissions(_path: &Path) -> Result<(), AppError> {
    Ok(())
}

pub fn validate_provider(provider_id: &str, config: &Value) -> Result<(), AppError> {
    if provider_id.trim().is_empty() {
        return Err(AppError::Config(
            "Pi provider key cannot be empty".to_string(),
        ));
    }
    let obj = config.as_object().ok_or_else(|| {
        AppError::Config("Pi provider configuration must be a JSON object".to_string())
    })?;

    let is_builtin_override =
        BUILTIN_PROVIDER_IDS.contains(&provider_id) && !obj.contains_key("models");
    if is_builtin_override {
        if let Some(base_url) = obj.get("baseUrl") {
            let base_url = base_url.as_str().unwrap_or_default().trim();
            if base_url.is_empty()
                || !(base_url.starts_with("https://") || base_url.starts_with("http://"))
            {
                return Err(AppError::Config(
                    "Pi provider baseUrl must use http:// or https://".to_string(),
                ));
            }
        }
        if let Some(api) = obj.get("api") {
            let api = api.as_str().unwrap_or_default();
            if !SUPPORTED_APIS.contains(&api) {
                return Err(AppError::Config(format!(
                    "Unsupported Pi provider API '{api}'. Allowed: {}",
                    SUPPORTED_APIS.join(", ")
                )));
            }
        }
        return Ok(());
    }

    let base_url = obj
        .get("baseUrl")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .trim();
    if base_url.is_empty() {
        return Err(AppError::Config(
            "Pi custom provider configuration requires baseUrl".to_string(),
        ));
    }
    if !(base_url.starts_with("https://") || base_url.starts_with("http://")) {
        return Err(AppError::Config(
            "Pi provider baseUrl must use http:// or https://".to_string(),
        ));
    }

    let provider_api = match obj.get("api") {
        Some(value) => {
            let api = value.as_str().unwrap_or_default();
            if !SUPPORTED_APIS.contains(&api) {
                return Err(AppError::Config(format!(
                    "Unsupported Pi provider API '{api}'. Allowed: {}",
                    SUPPORTED_APIS.join(", ")
                )));
            }
            Some(api)
        }
        None => None,
    };

    let models = obj
        .get("models")
        .and_then(Value::as_array)
        .ok_or_else(|| AppError::Config("Pi provider requires a models array".to_string()))?;
    if models.is_empty() {
        return Err(AppError::Config(
            "Pi provider requires at least one model".to_string(),
        ));
    }
    for (index, model) in models.iter().enumerate() {
        let id = model
            .get("id")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .trim();
        if id.is_empty() {
            return Err(AppError::Config(format!(
                "Pi provider model at index {index} requires a non-empty id"
            )));
        }
        match model.get("api") {
            Some(value) => {
                let api = value.as_str().unwrap_or_default();
                if !SUPPORTED_APIS.contains(&api) {
                    return Err(AppError::Config(format!(
                        "Unsupported Pi model API '{api}' at index {index}. Allowed: {}",
                        SUPPORTED_APIS.join(", ")
                    )));
                }
            }
            None if provider_api.is_none() => {
                return Err(AppError::Config(format!(
                    "Pi provider requires api at provider level or on every model; model at index {index} has none"
                )));
            }
            None => {}
        }
    }

    Ok(())
}

pub fn get_providers() -> Result<Map<String, Value>, AppError> {
    Ok(read_pi_config()?
        .get("providers")
        .and_then(Value::as_object)
        .cloned()
        .unwrap_or_default())
}

pub fn set_provider(provider_id: &str, config: Value) -> Result<(), AppError> {
    validate_provider(provider_id, &config)?;
    let _guard = write_lock()
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    let mut root = read_pi_config()?;
    let root_obj = root
        .as_object_mut()
        .ok_or_else(|| AppError::Config("Pi models.json root must be a JSON object".to_string()))?;
    if root_obj
        .get("providers")
        .is_some_and(|providers| !providers.is_object())
    {
        return Err(AppError::Config(
            "Pi models.json 'providers' must be a JSON object; refusing to overwrite it"
                .to_string(),
        ));
    }
    let providers = root_obj
        .entry("providers")
        .or_insert_with(|| json!({}))
        .as_object_mut()
        .ok_or_else(|| {
            AppError::Config(
                "Pi models.json 'providers' must be a JSON object; refusing to overwrite it"
                    .to_string(),
            )
        })?;
    providers.insert(provider_id.to_string(), config);
    write_pi_config(&root)
}

pub fn remove_provider(provider_id: &str) -> Result<(), AppError> {
    let _guard = write_lock()
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    let path = get_pi_models_path();
    if !path.exists() {
        return Ok(());
    }
    let mut root = read_pi_config()?;
    if let Some(providers) = root.get_mut("providers").and_then(Value::as_object_mut) {
        providers.remove(provider_id);
    }
    write_pi_config(&root)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serial_test::serial;

    #[test]
    #[serial]
    fn provider_updates_preserve_unrelated_pi_config() {
        let temp = tempfile::tempdir().expect("tempdir");
        let previous = std::env::var_os("CC_SWITCH_TEST_HOME");
        std::env::set_var("CC_SWITCH_TEST_HOME", temp.path());

        let path = get_pi_models_path();
        fs::create_dir_all(path.parent().expect("parent")).expect("create pi dir");
        fs::write(
            &path,
            r#"{"theme":"dark","providers":{"existing":{"baseUrl":"https://old.example/v1","api":"openai-completions","apiKey":"old","models":[{"id":"old-model"}]}}}"#,
        )
        .expect("seed models.json");

        set_provider(
            "new-provider",
            json!({
                "baseUrl": "https://new.example/v1",
                "api": "openai-responses",
                "apiKey": "new",
                "models": [{ "id": "new-model" }]
            }),
        )
        .expect("set provider");

        let config = read_pi_config().expect("read config");
        assert_eq!(config["theme"], json!("dark"));
        assert!(config["providers"]["existing"].is_object());
        assert_eq!(
            config["providers"]["new-provider"]["api"],
            json!("openai-responses")
        );

        remove_provider("new-provider").expect("remove provider");
        let config = read_pi_config().expect("read config after removal");
        assert!(config["providers"]["new-provider"].is_null());
        assert!(config["providers"]["existing"].is_object());

        match previous {
            Some(value) => std::env::set_var("CC_SWITCH_TEST_HOME", value),
            None => std::env::remove_var("CC_SWITCH_TEST_HOME"),
        }
    }

    #[test]
    fn built_in_provider_allows_api_key_only_override() {
        validate_provider("anthropic", &json!({ "apiKey": "$ANTHROPIC_API_KEY" }))
            .expect("built-in provider override should not require models");

        let error = validate_provider("custom-provider", &json!({ "apiKey": "secret" }))
            .expect_err("custom providers still require endpoint and models");
        assert!(error.to_string().contains("requires baseUrl"));
    }

    #[test]
    fn custom_provider_accepts_model_level_api() {
        validate_provider(
            "mixed-provider",
            &json!({
                "baseUrl": "https://example.com/v1",
                "models": [
                    { "id": "chat-model", "api": "openai-completions" },
                    { "id": "responses-model", "api": "openai-responses" }
                ]
            }),
        )
        .expect("Pi supports api on every model instead of the provider");

        let error = validate_provider(
            "incomplete-provider",
            &json!({
                "baseUrl": "https://example.com/v1",
                "models": [
                    { "id": "chat-model", "api": "openai-completions" },
                    { "id": "missing-api" }
                ]
            }),
        )
        .expect_err("every model needs an api when the provider has none");
        assert!(error.to_string().contains("model at index 1 has none"));
    }

    #[test]
    fn pi_directory_prefers_app_override_then_official_environment_override() {
        let app_override = PathBuf::from("/app/override");
        assert_eq!(
            select_pi_dir(
                Some(app_override.clone()),
                Some(OsString::from("/env/override"))
            ),
            Some(app_override)
        );
        assert_eq!(
            select_pi_dir(None, Some(OsString::from("/env/override"))),
            Some(PathBuf::from("/env/override"))
        );
    }

    #[test]
    fn relative_session_directory_requires_absolute_override() {
        assert_eq!(
            resolve_pi_session_path(".pi/sessions", "global settings.json"),
            None
        );
        assert_eq!(
            resolve_pi_session_path("/tmp/pi-sessions", "global settings.json"),
            Some(PathBuf::from("/tmp/pi-sessions"))
        );
        assert_eq!(resolve_pi_session_path(".pi/sessions", "environment"), None);
    }

    #[cfg(unix)]
    #[test]
    #[serial]
    fn provider_write_secures_models_and_backup_files() {
        use std::os::unix::fs::PermissionsExt;

        let temp = tempfile::tempdir().expect("tempdir");
        let previous = std::env::var_os("CC_SWITCH_TEST_HOME");
        std::env::set_var("CC_SWITCH_TEST_HOME", temp.path());

        let path = get_pi_models_path();
        fs::create_dir_all(path.parent().expect("parent")).expect("create pi dir");
        fs::write(&path, r#"{"providers":{}}"#).expect("seed models.json");
        fs::set_permissions(&path, fs::Permissions::from_mode(0o644))
            .expect("seed permissive mode");

        set_provider("anthropic", json!({ "apiKey": "literal-secret" })).expect("write provider");

        assert_eq!(
            fs::metadata(&path)
                .expect("models metadata")
                .permissions()
                .mode()
                & 0o777,
            0o600
        );
        let backup_dir = path.parent().expect("parent").join("backups");
        let backup = fs::read_dir(backup_dir)
            .expect("backup dir")
            .next()
            .expect("backup entry")
            .expect("read backup")
            .path();
        assert_eq!(
            fs::metadata(backup)
                .expect("backup metadata")
                .permissions()
                .mode()
                & 0o777,
            0o600
        );

        match previous {
            Some(value) => std::env::set_var("CC_SWITCH_TEST_HOME", value),
            None => std::env::remove_var("CC_SWITCH_TEST_HOME"),
        }
    }

    #[test]
    #[serial]
    fn provider_update_refuses_to_replace_malformed_providers_field() {
        let temp = tempfile::tempdir().expect("tempdir");
        let previous = std::env::var_os("CC_SWITCH_TEST_HOME");
        std::env::set_var("CC_SWITCH_TEST_HOME", temp.path());
        let path = get_pi_models_path();
        fs::create_dir_all(path.parent().expect("parent")).expect("create pi dir");

        for malformed in [json!("keep-me"), json!(["keep-me"]), Value::Null] {
            let original = json!({ "providers": malformed, "theme": "dark" });
            fs::write(
                &path,
                format!("{}\n", serde_json::to_string_pretty(&original).unwrap()),
            )
            .expect("seed malformed models.json");

            let error = set_provider(
                "new-provider",
                json!({
                    "baseUrl": "https://new.example/v1",
                    "api": "openai-completions",
                    "models": [{ "id": "new-model" }]
                }),
            )
            .expect_err("malformed providers must not be overwritten");
            assert!(error.to_string().contains("refusing to overwrite"));
            assert_eq!(read_pi_config().expect("config remains readable"), original);
        }

        match previous {
            Some(value) => std::env::set_var("CC_SWITCH_TEST_HOME", value),
            None => std::env::remove_var("CC_SWITCH_TEST_HOME"),
        }
    }
}
