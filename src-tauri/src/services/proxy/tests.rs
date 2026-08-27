use super::*;
use crate::provider::ProviderMeta;
use serial_test::serial;
use std::env;
use tempfile::TempDir;

struct TempHome {
    #[allow(dead_code)]
    dir: TempDir,
    original_home: Option<String>,
    original_userprofile: Option<String>,
    original_test_home: Option<String>,
}

impl TempHome {
    fn new() -> Self {
        let dir = TempDir::new().expect("failed to create temp home");
        let original_home = env::var("HOME").ok();
        let original_userprofile = env::var("USERPROFILE").ok();
        let original_test_home = env::var("CC_SWITCH_TEST_HOME").ok();

        env::set_var("HOME", dir.path());
        env::set_var("USERPROFILE", dir.path());
        env::set_var("CC_SWITCH_TEST_HOME", dir.path());

        Self {
            dir,
            original_home,
            original_userprofile,
            original_test_home,
        }
    }
}

impl Drop for TempHome {
    fn drop(&mut self) {
        match &self.original_home {
            Some(value) => env::set_var("HOME", value),
            None => env::remove_var("HOME"),
        }

        match &self.original_userprofile {
            Some(value) => env::set_var("USERPROFILE", value),
            None => env::remove_var("USERPROFILE"),
        }

        match &self.original_test_home {
            Some(value) => env::set_var("CC_SWITCH_TEST_HOME", value),
            None => env::remove_var("CC_SWITCH_TEST_HOME"),
        }
    }
}

fn assert_env_str(env: &Map<String, Value>, key: &str, expected: Option<&str>) {
    assert_eq!(env.get(key).and_then(|value| value.as_str()), expected);
}

async fn use_ephemeral_proxy_port(db: &Arc<Database>) {
    let mut proxy_config = db.get_proxy_config().await.expect("get test proxy config");
    proxy_config.listen_port = 0;
    db.update_proxy_config(proxy_config)
        .await
        .expect("set test proxy config to an ephemeral port");
}

async fn running_codex_base_url(service: &ProxyService) -> String {
    let status = service.get_status().await.expect("get proxy status");
    format!("http://127.0.0.1:{}/v1", status.port)
}

fn seed_codex_model_template() {
    let codex_dir = crate::codex_config::get_codex_config_dir();
    std::fs::create_dir_all(&codex_dir).expect("create codex dir");
    std::fs::write(
        codex_dir.join("models_cache.json"),
        serde_json::to_string(&serde_json::json!({
            "models": [{
                "slug": "gpt-5.5",
                "display_name": "GPT-5.5",
                "model_messages": { "instructions_template": "t" },
                "additional_speed_tiers": [],
                "context_window": 128000
            }]
        }))
        .expect("serialize models_cache"),
    )
    .expect("write models_cache.json");
}

#[test]
fn managed_account_claude_takeover_uses_api_key_placeholder() {
    let mut provider = Provider::with_id(
        "copilot".to_string(),
        "GitHub Copilot".to_string(),
        json!({
            "env": {
                "ANTHROPIC_BASE_URL": "https://api.githubcopilot.com",
                "ANTHROPIC_MODEL": "claude-haiku-4.5"
            }
        }),
        None,
    );
    provider.meta = Some(ProviderMeta {
        provider_type: Some("github_copilot".to_string()),
        ..Default::default()
    });

    let mut live_config = provider.settings_config.clone();
    ProxyService::apply_claude_takeover_fields_for_provider(
        &mut live_config,
        "http://127.0.0.1:15721",
        &provider,
    );

    let env = live_config
        .get("env")
        .and_then(|value| value.as_object())
        .expect("env should exist");
    assert_eq!(
        env.get("ANTHROPIC_API_KEY")
            .and_then(|value| value.as_str()),
        Some(PROXY_TOKEN_PLACEHOLDER)
    );
    assert!(
        env.get("ANTHROPIC_AUTH_TOKEN").is_none(),
        "managed OAuth providers should avoid Claude Auth Token login semantics"
    );
}

#[test]
fn managed_account_claude_takeover_sources_copilot_models_from_provider() {
    let mut provider = Provider::with_id(
        "copilot".to_string(),
        "GitHub Copilot".to_string(),
        json!({
            "env": {
                "ANTHROPIC_BASE_URL": "https://api.githubcopilot.com",
                "ANTHROPIC_MODEL": "claude-sonnet-4.6",
                "ANTHROPIC_DEFAULT_HAIKU_MODEL": "claude-haiku-4.5",
                "ANTHROPIC_DEFAULT_SONNET_MODEL": "claude-sonnet-4.6",
                "ANTHROPIC_DEFAULT_OPUS_MODEL": "claude-sonnet-4.6"
            }
        }),
        None,
    );
    provider.meta = Some(ProviderMeta {
        provider_type: Some("github_copilot".to_string()),
        ..Default::default()
    });

    let mut live_config = json!({
        "env": {
            "ANTHROPIC_BASE_URL": "https://stale.example.com",
            "ANTHROPIC_API_KEY": "stale-key",
            "ANTHROPIC_MODEL": "stale-model",
            "ANTHROPIC_DEFAULT_HAIKU_MODEL": "stale-haiku",
            "ANTHROPIC_DEFAULT_HAIKU_MODEL_NAME": "Stale Haiku",
            "ANTHROPIC_DEFAULT_SONNET_MODEL": "stale-sonnet",
            "ANTHROPIC_DEFAULT_SONNET_MODEL_NAME": "Stale Sonnet",
            "ANTHROPIC_DEFAULT_OPUS_MODEL": "stale-opus",
            "ANTHROPIC_DEFAULT_OPUS_MODEL_NAME": "Stale Opus"
        }
    });
    ProxyService::apply_claude_takeover_fields_for_provider(
        &mut live_config,
        "http://127.0.0.1:15721",
        &provider,
    );

    let env = live_config
        .get("env")
        .and_then(|value| value.as_object())
        .expect("env should exist");
    assert_env_str(env, "ANTHROPIC_MODEL", None);
    assert_env_str(
        env,
        "ANTHROPIC_DEFAULT_HAIKU_MODEL",
        Some("claude-haiku-4-5"),
    );
    assert_env_str(
        env,
        "ANTHROPIC_DEFAULT_HAIKU_MODEL_NAME",
        Some("claude-haiku-4.5"),
    );
    assert_env_str(
        env,
        "ANTHROPIC_DEFAULT_SONNET_MODEL",
        Some("claude-sonnet-4-6"),
    );
    assert_env_str(
        env,
        "ANTHROPIC_DEFAULT_SONNET_MODEL_NAME",
        Some("claude-sonnet-4.6"),
    );
    assert_env_str(env, "ANTHROPIC_DEFAULT_OPUS_MODEL", Some("claude-opus-4-8"));
    assert_env_str(
        env,
        "ANTHROPIC_DEFAULT_OPUS_MODEL_NAME",
        Some("claude-sonnet-4.6"),
    );
    assert_env_str(env, "ANTHROPIC_API_KEY", Some(PROXY_TOKEN_PLACEHOLDER));
    assert_env_str(env, "ANTHROPIC_AUTH_TOKEN", None);
}

#[test]
fn managed_account_claude_takeover_sources_codex_models_from_provider() {
    let mut provider = Provider::with_id(
        "codex".to_string(),
        "Codex".to_string(),
        json!({
            "env": {
                "ANTHROPIC_BASE_URL": "https://chatgpt.com/backend-api/codex",
                "ANTHROPIC_MODEL": "gpt-5.4",
                "ANTHROPIC_DEFAULT_HAIKU_MODEL": "gpt-5.4-mini",
                "ANTHROPIC_DEFAULT_SONNET_MODEL": "gpt-5.4",
                "ANTHROPIC_DEFAULT_OPUS_MODEL": "gpt-5.4"
            }
        }),
        None,
    );
    provider.meta = Some(ProviderMeta {
        provider_type: Some("codex_oauth".to_string()),
        ..Default::default()
    });

    let mut live_config = json!({
        "env": {
            "ANTHROPIC_BASE_URL": "https://stale.example.com",
            "ANTHROPIC_AUTH_TOKEN": "stale-token",
            "ANTHROPIC_MODEL": "stale-model",
            "ANTHROPIC_DEFAULT_HAIKU_MODEL": "stale-haiku",
            "ANTHROPIC_DEFAULT_HAIKU_MODEL_NAME": "Stale Haiku",
            "ANTHROPIC_DEFAULT_SONNET_MODEL": "stale-sonnet",
            "ANTHROPIC_DEFAULT_SONNET_MODEL_NAME": "Stale Sonnet",
            "ANTHROPIC_DEFAULT_OPUS_MODEL": "stale-opus",
            "ANTHROPIC_DEFAULT_OPUS_MODEL_NAME": "Stale Opus"
        }
    });
    ProxyService::apply_claude_takeover_fields_for_provider(
        &mut live_config,
        "http://127.0.0.1:15721",
        &provider,
    );

    let env = live_config
        .get("env")
        .and_then(|value| value.as_object())
        .expect("env should exist");
    assert_env_str(env, "ANTHROPIC_MODEL", None);
    assert_env_str(
        env,
        "ANTHROPIC_DEFAULT_HAIKU_MODEL",
        Some("claude-haiku-4-5"),
    );
    assert_env_str(
        env,
        "ANTHROPIC_DEFAULT_HAIKU_MODEL_NAME",
        Some("gpt-5.4-mini"),
    );
    assert_env_str(
        env,
        "ANTHROPIC_DEFAULT_SONNET_MODEL",
        Some("claude-sonnet-4-6"),
    );
    assert_env_str(env, "ANTHROPIC_DEFAULT_SONNET_MODEL_NAME", Some("gpt-5.4"));
    assert_env_str(env, "ANTHROPIC_DEFAULT_OPUS_MODEL", Some("claude-opus-4-8"));
    assert_env_str(env, "ANTHROPIC_DEFAULT_OPUS_MODEL_NAME", Some("gpt-5.4"));
    assert_env_str(env, "ANTHROPIC_API_KEY", Some(PROXY_TOKEN_PLACEHOLDER));
    assert_env_str(env, "ANTHROPIC_AUTH_TOKEN", Some(PROXY_TOKEN_PLACEHOLDER));
}

#[test]
fn managed_account_claude_takeover_codex_injects_auth_token_without_preexisting_key() {
    let mut provider = Provider::with_id(
        "codex".to_string(),
        "Codex".to_string(),
        json!({
            "env": {
                "ANTHROPIC_BASE_URL": "https://chatgpt.com/backend-api/codex"
            }
        }),
        None,
    );
    provider.meta = Some(ProviderMeta {
        provider_type: Some("codex_oauth".to_string()),
        ..Default::default()
    });

    // 全新安装/热切换形态：传入的 env 没有任何 token 键。
    let mut live_config = provider.settings_config.clone();
    ProxyService::apply_claude_takeover_fields_for_provider(
        &mut live_config,
        "http://127.0.0.1:15721",
        &provider,
    );

    let env = live_config
        .get("env")
        .and_then(|value| value.as_object())
        .expect("env should exist");
    assert_env_str(env, "ANTHROPIC_API_KEY", Some(PROXY_TOKEN_PLACEHOLDER));
    assert_env_str(env, "ANTHROPIC_AUTH_TOKEN", Some(PROXY_TOKEN_PLACEHOLDER));
}

#[test]
fn managed_account_claude_takeover_codex_by_base_url_keeps_auth_token() {
    // 无 provider_type meta、仅凭 base_url 识别为受管 codex 的供应商，
    // 也必须保留 AUTH_TOKEN 占位符（与策略选择共用同一判定族）。
    let provider = Provider::with_id(
        "codex-url-only".to_string(),
        "Codex (URL only)".to_string(),
        json!({
            "env": {
                "ANTHROPIC_BASE_URL": "https://chatgpt.com/backend-api/codex"
            }
        }),
        None,
    );
    assert!(provider.uses_managed_account_auth());
    assert!(!provider.is_codex_oauth());

    let mut live_config = provider.settings_config.clone();
    ProxyService::apply_claude_takeover_fields_for_provider(
        &mut live_config,
        "http://127.0.0.1:15721",
        &provider,
    );

    let env = live_config
        .get("env")
        .and_then(|value| value.as_object())
        .expect("env should exist");
    assert_env_str(env, "ANTHROPIC_API_KEY", Some(PROXY_TOKEN_PLACEHOLDER));
    assert_env_str(env, "ANTHROPIC_AUTH_TOKEN", Some(PROXY_TOKEN_PLACEHOLDER));
}

#[test]
fn managed_account_claude_takeover_copilot_removes_stale_auth_token() {
    let mut provider = Provider::with_id(
        "copilot".to_string(),
        "GitHub Copilot".to_string(),
        json!({
            "env": {
                "ANTHROPIC_BASE_URL": "https://api.githubcopilot.com"
            }
        }),
        None,
    );
    provider.meta = Some(ProviderMeta {
        provider_type: Some("github_copilot".to_string()),
        ..Default::default()
    });

    let mut live_config = json!({
        "env": {
            "ANTHROPIC_BASE_URL": "https://stale.example.com",
            "ANTHROPIC_AUTH_TOKEN": "stale-token"
        }
    });
    ProxyService::apply_claude_takeover_fields_for_provider(
        &mut live_config,
        "http://127.0.0.1:15721",
        &provider,
    );

    let env = live_config
        .get("env")
        .and_then(|value| value.as_object())
        .expect("env should exist");
    assert_env_str(env, "ANTHROPIC_API_KEY", Some(PROXY_TOKEN_PLACEHOLDER));
    assert_env_str(env, "ANTHROPIC_AUTH_TOKEN", None);
}

#[test]
fn normal_claude_takeover_without_token_keeps_auth_token_fallback() {
    let mut live_config = json!({
        "env": {
            "ANTHROPIC_BASE_URL": "https://api.example.com",
            "ANTHROPIC_MODEL": "claude-haiku-4.5"
        }
    });

    ProxyService::apply_claude_takeover_fields(&mut live_config, "http://127.0.0.1:15721");

    assert_eq!(
        live_config
            .get("env")
            .and_then(|env| env.get("ANTHROPIC_AUTH_TOKEN"))
            .and_then(|value| value.as_str()),
        Some(PROXY_TOKEN_PLACEHOLDER)
    );
    assert!(
        live_config
            .get("env")
            .and_then(|env| env.get("ANTHROPIC_API_KEY"))
            .is_none(),
        "non-managed providers should retain the legacy fallback behavior"
    );
}

#[tokio::test]
#[serial]
async fn start_with_takeover_ephemeral_port_writes_actual_live_url() {
    let _home = TempHome::new();
    crate::settings::reload_settings().expect("reload settings");

    let db = Arc::new(Database::memory().expect("init db"));
    use_ephemeral_proxy_port(&db).await;
    let service = ProxyService::new(db.clone());

    let provider = Provider::with_id(
        "p1".to_string(),
        "P1".to_string(),
        json!({
            "env": {
                "ANTHROPIC_API_KEY": "provider-key",
                "ANTHROPIC_BASE_URL": "https://api.anthropic.com"
            }
        }),
        None,
    );
    db.save_provider("claude", &provider)
        .expect("save provider");
    db.set_current_provider("claude", "p1")
        .expect("set db current provider");
    crate::settings::set_current_provider(&AppType::Claude, Some("p1"))
        .expect("set local current provider");
    service
        .write_claude_live(&json!({
            "env": {
                "ANTHROPIC_API_KEY": "live-key",
                "ANTHROPIC_BASE_URL": "https://api.anthropic.com"
            }
        }))
        .expect("seed claude live config");

    let info = service
        .start_with_takeover()
        .await
        .expect("start proxy with takeover");
    assert_ne!(info.port, 0, "OS should assign a concrete port");

    let stored_config = db.get_proxy_config().await.expect("read proxy config");
    assert_eq!(
        stored_config.listen_port, info.port,
        "resolved dynamic port should be persisted for DB-only proxy URL paths"
    );

    let live = service.read_claude_live().expect("read taken-over live");
    let base_url = live
        .get("env")
        .and_then(|env| env.get("ANTHROPIC_BASE_URL"))
        .and_then(|value| value.as_str())
        .expect("taken-over base url");
    assert_eq!(base_url, format!("http://127.0.0.1:{}", info.port));
    assert!(
        !base_url.contains(":0"),
        "takeover must never write an unresolved :0 port"
    );

    service
        .stop_with_restore()
        .await
        .expect("stop proxy and restore live config");
}

#[test]
#[serial]
fn codex_custom_provider_live_write_preserves_oauth_auth_json() {
    let _home = TempHome::new();
    crate::settings::reload_settings().expect("reload settings");
    crate::settings::update_settings(crate::settings::AppSettings {
        preserve_codex_official_auth_on_switch: true,
        ..Default::default()
    })
    .expect("enable Codex official auth preservation");

    let db = Arc::new(Database::memory().expect("init db"));
    let service = ProxyService::new(db);
    let oauth_auth = json!({
        "auth_mode": "chatgpt",
        "tokens": {
            "id_token": "oauth-id",
            "access_token": "oauth-access"
        }
    });
    crate::codex_config::write_codex_live_atomic(
        &oauth_auth,
        Some(
            r#"model_provider = "openai"
model = "gpt-5-codex"
"#,
        ),
    )
    .expect("seed live OAuth auth");

    let mut provider = Provider::with_id(
        "rightcode".to_string(),
        "RightCode".to_string(),
        json!({
            "auth": {
                "OPENAI_API_KEY": "rightcode-key"
            },
            "config": r#"model_provider = "rightcode"
model = "gpt-5-codex"

[model_providers.rightcode]
name = "RightCode"
base_url = "https://rightcode.example/v1"
wire_api = "responses"
"#
        }),
        None,
    );
    provider.category = Some("custom".to_string());
    let takeover_settings = json!({
        "auth": {
            "OPENAI_API_KEY": PROXY_TOKEN_PLACEHOLDER
        },
        "config": r#"model_provider = "rightcode"
model = "gpt-5-codex"

[model_providers.rightcode]
name = "RightCode"
base_url = "http://127.0.0.1:15721/v1"
wire_api = "responses"
"#
    });

    service
        .write_codex_live_for_provider(&takeover_settings, Some(&provider))
        .expect("write provider-driven Codex live config");

    let live_auth: Value =
        crate::config::read_json_file(&crate::codex_config::get_codex_auth_path())
            .expect("read live auth");
    assert_eq!(
        live_auth, oauth_auth,
        "third-party Codex proxy writes must not overwrite ChatGPT OAuth login state"
    );

    let live_config = std::fs::read_to_string(crate::codex_config::get_codex_config_path())
        .expect("read live config");
    assert!(
        live_config.contains("experimental_bearer_token"),
        "proxy placeholder should move into config.toml instead of auth.json"
    );
    assert!(
        live_config.contains(PROXY_TOKEN_PLACEHOLDER),
        "live config should carry the proxy placeholder token"
    );

    crate::settings::update_settings(crate::settings::AppSettings::default())
        .expect("reset settings");
}

#[tokio::test]
#[serial]
async fn codex_takeover_preserves_oauth_auth_json_when_preserve_enabled() {
    let _home = TempHome::new();
    crate::settings::reload_settings().expect("reload settings");
    crate::settings::update_settings(crate::settings::AppSettings {
        preserve_codex_official_auth_on_switch: true,
        ..Default::default()
    })
    .expect("enable Codex official auth preservation");

    let db = Arc::new(Database::memory().expect("init db"));
    let service = ProxyService::new(db.clone());
    let oauth_auth = json!({
        "auth_mode": "chatgpt",
        "tokens": {
            "id_token": "oauth-id",
            "access_token": "oauth-access"
        }
    });
    let deepseek_live_config = r#"model_provider = "deepseek"
model = "deepseek-v4-flash"

[model_providers.deepseek]
name = "DeepSeek"
base_url = "https://api.deepseek.com/v1"
wire_api = "responses"
experimental_bearer_token = "deepseek-key"
"#;
    crate::codex_config::write_codex_live_atomic(&oauth_auth, Some(deepseek_live_config))
        .expect("seed live OAuth auth with DeepSeek config");

    let mut provider = Provider::with_id(
        "deepseek".to_string(),
        "DeepSeek".to_string(),
        json!({
            "auth": {
                "OPENAI_API_KEY": "deepseek-key"
            },
            "config": r#"model_provider = "deepseek"
model = "deepseek-v4-flash"

[model_providers.deepseek]
name = "DeepSeek"
base_url = "https://api.deepseek.com/v1"
wire_api = "responses"
"#
        }),
        None,
    );
    provider.category = Some("cn_official".to_string());
    db.save_provider("codex", &provider)
        .expect("save DeepSeek provider");
    db.set_current_provider("codex", "deepseek")
        .expect("set current provider");
    crate::settings::set_current_provider(&AppType::Codex, Some("deepseek"))
        .expect("set local current provider");

    service
        .takeover_live_config_strict(&AppType::Codex)
        .await
        .expect("take over Codex live config");

    let live_auth: Value =
        crate::config::read_json_file(&crate::codex_config::get_codex_auth_path())
            .expect("read live auth");
    assert_eq!(
        live_auth, oauth_auth,
        "Codex takeover should not overwrite ChatGPT OAuth auth when preservation is enabled"
    );

    let live_config = std::fs::read_to_string(crate::codex_config::get_codex_config_path())
        .expect("read live config");
    assert!(
        live_config.contains(PROXY_TOKEN_PLACEHOLDER),
        "takeover placeholder should move into config.toml"
    );
    assert!(
        service.detect_takeover_in_live_config_for_app(&AppType::Codex),
        "Codex takeover detection should recognize config.toml placeholders"
    );

    crate::settings::update_settings(crate::settings::AppSettings::default())
        .expect("reset settings");
}

#[tokio::test]
#[serial]
async fn codex_takeover_preserves_oauth_auth_json_even_when_provider_category_is_official() {
    let _home = TempHome::new();
    crate::settings::reload_settings().expect("reload settings");
    crate::settings::update_settings(crate::settings::AppSettings {
        preserve_codex_official_auth_on_switch: true,
        ..Default::default()
    })
    .expect("enable Codex official auth preservation");

    let db = Arc::new(Database::memory().expect("init db"));
    let service = ProxyService::new(db.clone());
    let oauth_auth = json!({
        "auth_mode": "chatgpt",
        "tokens": {
            "id_token": "oauth-id",
            "access_token": "oauth-access"
        }
    });
    let deepseek_live_config = r#"model_provider = "deepseek"
model = "deepseek-v4-flash"

[model_providers.deepseek]
name = "DeepSeek"
base_url = "https://api.deepseek.com/v1"
wire_api = "responses"
experimental_bearer_token = "deepseek-key"
"#;
    crate::codex_config::write_codex_live_atomic(&oauth_auth, Some(deepseek_live_config))
        .expect("seed live OAuth auth with DeepSeek config");

    let mut provider = Provider::with_id(
        "deepseek".to_string(),
        "DeepSeek".to_string(),
        json!({
            "auth": {
                "OPENAI_API_KEY": "deepseek-key"
            },
            "config": r#"model_provider = "deepseek"
model = "deepseek-v4-flash"

[model_providers.deepseek]
name = "DeepSeek"
base_url = "https://api.deepseek.com/v1"
wire_api = "responses"
"#
        }),
        None,
    );
    provider.category = Some("official".to_string());
    db.save_provider("codex", &provider)
        .expect("save misclassified DeepSeek provider");
    db.set_current_provider("codex", "deepseek")
        .expect("set current provider");
    crate::settings::set_current_provider(&AppType::Codex, Some("deepseek"))
        .expect("set local current provider");

    service
        .takeover_live_config_strict(&AppType::Codex)
        .await
        .expect("take over Codex live config");

    let live_auth: Value =
        crate::config::read_json_file(&crate::codex_config::get_codex_auth_path())
            .expect("read live auth");
    assert_eq!(
        live_auth, oauth_auth,
        "Codex takeover must not rewrite auth.json when preservation is enabled, even if provider category is stale or misclassified"
    );

    let live_config = std::fs::read_to_string(crate::codex_config::get_codex_config_path())
        .expect("read live config");
    assert!(
        live_config.contains(PROXY_TOKEN_PLACEHOLDER),
        "takeover placeholder should move into config.toml"
    );

    crate::settings::update_settings(crate::settings::AppSettings::default())
        .expect("reset settings");
}

#[tokio::test]
#[serial]
async fn codex_set_takeover_for_app_preserves_oauth_auth_json_when_preserve_enabled() {
    let _home = TempHome::new();
    crate::settings::reload_settings().expect("reload settings");
    crate::settings::update_settings(crate::settings::AppSettings {
        preserve_codex_official_auth_on_switch: true,
        ..Default::default()
    })
    .expect("enable Codex official auth preservation");

    let db = Arc::new(Database::memory().expect("init db"));
    use_ephemeral_proxy_port(&db).await;
    let service = ProxyService::new(db.clone());
    let oauth_auth = json!({
        "auth_mode": "chatgpt",
        "tokens": {
            "id_token": "oauth-id",
            "access_token": "oauth-access"
        }
    });
    let deepseek_live_config = r#"model_provider = "deepseek"
model = "deepseek-v4-flash"

[model_providers.deepseek]
name = "DeepSeek"
base_url = "https://api.deepseek.com/v1"
wire_api = "responses"
experimental_bearer_token = "deepseek-key"
"#;
    crate::codex_config::write_codex_live_atomic(&oauth_auth, Some(deepseek_live_config))
        .expect("seed live OAuth auth with DeepSeek config");

    let mut provider = Provider::with_id(
        "deepseek".to_string(),
        "DeepSeek".to_string(),
        json!({
            "auth": {
                "OPENAI_API_KEY": "deepseek-key"
            },
            "config": r#"model_provider = "deepseek"
model = "deepseek-v4-flash"

[model_providers.deepseek]
name = "DeepSeek"
base_url = "https://api.deepseek.com/v1"
wire_api = "responses"
"#
        }),
        None,
    );
    provider.category = Some("official".to_string());
    db.save_provider("codex", &provider)
        .expect("save misclassified DeepSeek provider");
    db.set_current_provider("codex", "deepseek")
        .expect("set current provider");
    crate::settings::set_current_provider(&AppType::Codex, Some("deepseek"))
        .expect("set local current provider");

    service
        .set_takeover_for_app("codex", true)
        .await
        .expect("enable Codex takeover");

    let live_auth: Value =
        crate::config::read_json_file(&crate::codex_config::get_codex_auth_path())
            .expect("read live auth");
    assert_eq!(
        live_auth, oauth_auth,
        "the public takeover command path must not rewrite auth.json when preservation is enabled"
    );

    service
        .set_takeover_for_app("codex", false)
        .await
        .expect("disable Codex takeover");
    crate::settings::update_settings(crate::settings::AppSettings::default())
        .expect("reset settings");
}

#[tokio::test]
#[serial]
async fn codex_sync_current_to_live_during_takeover_preserves_oauth_auth_json() {
    let _home = TempHome::new();
    crate::settings::reload_settings().expect("reload settings");
    crate::settings::update_settings(crate::settings::AppSettings {
        preserve_codex_official_auth_on_switch: true,
        ..Default::default()
    })
    .expect("enable Codex official auth preservation");

    let db = Arc::new(Database::memory().expect("init db"));
    use_ephemeral_proxy_port(&db).await;
    let state = crate::store::AppState::new(db.clone());
    let oauth_auth = json!({
        "auth_mode": "chatgpt",
        "tokens": {
            "id_token": "oauth-id",
            "access_token": "oauth-access"
        }
    });
    let deepseek_live_config = r#"model_provider = "deepseek"
model = "deepseek-v4-flash"

[model_providers.deepseek]
name = "DeepSeek"
base_url = "https://api.deepseek.com/v1"
wire_api = "responses"
experimental_bearer_token = "deepseek-key"
"#;
    crate::codex_config::write_codex_live_atomic(&oauth_auth, Some(deepseek_live_config))
        .expect("seed live OAuth auth with DeepSeek config");

    let mut provider = Provider::with_id(
        "deepseek".to_string(),
        "DeepSeek".to_string(),
        json!({
            "auth": {
                "OPENAI_API_KEY": "deepseek-key"
            },
            "config": r#"model_provider = "deepseek"
model = "deepseek-v4-flash"

[model_providers.deepseek]
name = "DeepSeek"
base_url = "https://api.deepseek.com/v1"
wire_api = "responses"
"#
        }),
        None,
    );
    provider.category = Some("official".to_string());
    db.save_provider("codex", &provider)
        .expect("save misclassified DeepSeek provider");
    db.set_current_provider("codex", "deepseek")
        .expect("set current provider");
    crate::settings::set_current_provider(&AppType::Codex, Some("deepseek"))
        .expect("set local current provider");

    state
        .proxy_service
        .set_takeover_for_app("codex", true)
        .await
        .expect("enable Codex takeover");

    crate::services::provider::ProviderService::sync_current_to_live(&state)
        .expect("sync current providers while Codex is taken over");

    let live_auth: Value =
        crate::config::read_json_file(&crate::codex_config::get_codex_auth_path())
            .expect("read live auth");
    assert_eq!(
        live_auth, oauth_auth,
        "post-change provider sync must not rewrite Codex auth.json during takeover"
    );

    let backup = db
        .get_live_backup("codex")
        .await
        .expect("get live backup")
        .expect("backup exists");
    let backup_value: Value = serde_json::from_str(&backup.original_config).expect("parse backup");
    assert_eq!(
        backup_value.get("auth"),
        Some(&oauth_auth),
        "provider-derived takeover backup should preserve official OAuth auth"
    );
    let backup_config = backup_value
        .get("config")
        .and_then(|value| value.as_str())
        .expect("backup config");
    let parsed_backup: toml::Value = toml::from_str(backup_config).expect("parse backup config");
    assert!(
        parsed_backup
            .get("model_providers")
            .and_then(|providers| providers.get("deepseek"))
            .and_then(|provider| provider.get("experimental_bearer_token"))
            .and_then(|token| token.as_str())
            == Some("deepseek-key"),
        "DeepSeek restore backup should retain the published direct bearer authentication"
    );
    assert!(
        parsed_backup
            .get("model_providers")
            .and_then(|providers| providers.get("deepseek"))
            .and_then(|provider| provider.get("auth"))
            .is_none(),
        "native DeepSeek auth should not be rewritten as command auth"
    );

    state
        .proxy_service
        .set_takeover_for_app("codex", false)
        .await
        .expect("disable Codex takeover");
    let restored_auth: Value =
        crate::config::read_json_file(&crate::codex_config::get_codex_auth_path())
            .expect("read restored auth");
    assert_eq!(
        restored_auth, oauth_auth,
        "turning takeover off should restore the preserved official OAuth auth"
    );

    crate::settings::update_settings(crate::settings::AppSettings::default())
        .expect("reset settings");
}

#[tokio::test]
#[serial]
async fn codex_sync_current_to_live_during_takeover_activation_keeps_proxy_live_config() {
    let _home = TempHome::new();
    crate::settings::reload_settings().expect("reload settings");
    crate::settings::update_settings(crate::settings::AppSettings {
        preserve_codex_official_auth_on_switch: true,
        ..Default::default()
    })
    .expect("enable Codex official auth preservation");

    let db = Arc::new(Database::memory().expect("init db"));
    let state = crate::store::AppState::new(db.clone());
    let oauth_auth = json!({
        "auth_mode": "chatgpt",
        "tokens": {
            "id_token": "oauth-id",
            "access_token": "oauth-access"
        }
    });
    let deepseek_live_config = r#"model_provider = "deepseek"
model = "deepseek-v4-flash"

[model_providers.deepseek]
name = "DeepSeek"
base_url = "https://api.deepseek.com/v1"
wire_api = "responses"
experimental_bearer_token = "deepseek-key"
"#;
    crate::codex_config::write_codex_live_atomic(&oauth_auth, Some(deepseek_live_config))
        .expect("seed live OAuth auth with DeepSeek config");

    let mut provider = Provider::with_id(
        "deepseek".to_string(),
        "DeepSeek".to_string(),
        json!({
            "auth": {
                "OPENAI_API_KEY": "deepseek-key"
            },
            "config": r#"model_provider = "deepseek"
model = "deepseek-v4-flash"

[model_providers.deepseek]
name = "DeepSeek"
base_url = "https://api.deepseek.com/v1"
wire_api = "responses"
"#
        }),
        None,
    );
    provider.category = Some("official".to_string());
    db.save_provider("codex", &provider)
        .expect("save misclassified DeepSeek provider");
    db.set_current_provider("codex", "deepseek")
        .expect("set current provider");
    crate::settings::set_current_provider(&AppType::Codex, Some("deepseek"))
        .expect("set local current provider");

    state
        .proxy_service
        .backup_live_config_strict(&AppType::Codex)
        .await
        .expect("backup Codex live config");
    state
        .proxy_service
        .takeover_live_config_strict(&AppType::Codex)
        .await
        .expect("take over Codex live config");
    assert!(
        !db.get_proxy_config_for_app("codex")
            .await
            .expect("get Codex proxy config")
            .enabled,
        "this reproduces the activation window before set_takeover_for_app marks enabled=true"
    );

    crate::services::provider::ProviderService::sync_current_to_live(&state)
        .expect("sync current providers during takeover activation");

    let live_auth: Value =
        crate::config::read_json_file(&crate::codex_config::get_codex_auth_path())
            .expect("read live auth");
    assert_eq!(
        live_auth, oauth_auth,
        "activation-time provider sync must not rewrite Codex OAuth auth.json"
    );

    let live_config = std::fs::read_to_string(crate::codex_config::get_codex_config_path())
        .expect("read live config");
    assert!(
        live_config.contains(PROXY_TOKEN_PLACEHOLDER),
        "activation-time provider sync must keep the proxy bearer placeholder"
    );
    assert!(
        live_config.contains("http://127.0.0.1"),
        "activation-time provider sync must keep the local proxy base_url"
    );
    assert!(
        state
            .proxy_service
            .detect_takeover_in_live_config_for_app(&AppType::Codex),
        "Codex live config should still be detected as taken over"
    );

    crate::settings::update_settings(crate::settings::AppSettings::default())
        .expect("reset settings");
}

#[tokio::test]
#[serial]
async fn codex_set_takeover_rebuilds_stale_enabled_state_without_overwriting_backup() {
    let _home = TempHome::new();
    crate::settings::reload_settings().expect("reload settings");
    crate::settings::update_settings(crate::settings::AppSettings {
        preserve_codex_official_auth_on_switch: true,
        ..Default::default()
    })
    .expect("enable Codex official auth preservation");

    let db = Arc::new(Database::memory().expect("init db"));
    use_ephemeral_proxy_port(&db).await;
    let service = ProxyService::new(db.clone());
    let oauth_auth = json!({
        "auth_mode": "chatgpt",
        "tokens": {
            "id_token": "oauth-id",
            "access_token": "oauth-access"
        }
    });
    let original_deepseek_config = r#"model_provider = "deepseek"
model = "deepseek-v4-flash"

[model_providers.deepseek]
name = "DeepSeek"
base_url = "https://api.deepseek.com/v1"
wire_api = "responses"
experimental_bearer_token = "deepseek-key"
"#;
    let stale_live_config = r#"model_provider = "deepseek"
model = "deepseek-v4-flash"

[model_providers.deepseek]
name = "DeepSeek"
base_url = "https://api.deepseek.com/v1"
wire_api = "responses"
experimental_bearer_token = "PROXY_MANAGED"
"#;
    crate::codex_config::write_codex_live_atomic(&oauth_auth, Some(stale_live_config))
        .expect("seed stale Codex live config");

    let mut provider = Provider::with_id(
        "deepseek".to_string(),
        "DeepSeek".to_string(),
        json!({
            "auth": {
                "OPENAI_API_KEY": "deepseek-key"
            },
            "config": r#"model_provider = "deepseek"
model = "deepseek-v4-flash"

[model_providers.deepseek]
name = "DeepSeek"
base_url = "https://api.deepseek.com/v1"
wire_api = "responses"
"#
        }),
        None,
    );
    provider.category = Some("official".to_string());
    db.save_provider("codex", &provider)
        .expect("save misclassified DeepSeek provider");
    db.set_current_provider("codex", "deepseek")
        .expect("set current provider");
    crate::settings::set_current_provider(&AppType::Codex, Some("deepseek"))
        .expect("set local current provider");
    db.save_live_backup(
        "codex",
        &serde_json::to_string(&json!({
            "auth": oauth_auth,
            "config": original_deepseek_config
        }))
        .expect("serialize original backup"),
    )
    .await
    .expect("seed original live backup");
    let mut proxy_config = db
        .get_proxy_config_for_app("codex")
        .await
        .expect("get Codex proxy config");
    proxy_config.enabled = true;
    db.update_proxy_config_for_app(proxy_config)
        .await
        .expect("mark Codex takeover enabled");

    service
        .set_takeover_for_app("codex", true)
        .await
        .expect("rebuild Codex takeover");

    let live_auth: Value =
        crate::config::read_json_file(&crate::codex_config::get_codex_auth_path())
            .expect("read live auth");
    assert_eq!(
        live_auth, oauth_auth,
        "repairing stale takeover must restore the preserved OAuth auth from backup"
    );

    let live_config = std::fs::read_to_string(crate::codex_config::get_codex_config_path())
        .expect("read live config");
    let expected_base_url = running_codex_base_url(&service).await;
    assert!(
        live_config.contains(&expected_base_url),
        "stale enabled takeover must be rebuilt to the current proxy base_url"
    );
    assert!(
        live_config.contains(PROXY_TOKEN_PLACEHOLDER),
        "rebuilt takeover should keep the proxy bearer placeholder"
    );
    assert!(
        service
            .live_takeover_matches_current_proxy(&AppType::Codex)
            .await
            .expect("detect rebuilt Codex takeover"),
        "rebuilt Codex live config should match the active proxy address"
    );

    let backup = db
        .get_live_backup("codex")
        .await
        .expect("get Codex live backup")
        .expect("backup exists");
    let backup_value: Value = serde_json::from_str(&backup.original_config).expect("parse backup");
    assert_eq!(
        backup_value.get("auth"),
        Some(&oauth_auth),
        "rebuilding stale takeover must not overwrite the original OAuth backup"
    );
    assert!(
        backup_value
            .get("config")
            .and_then(|value| value.as_str())
            .is_some_and(
                |config| config.contains("deepseek-key") && !config.contains("http://127.0.0.1")
            ),
        "backup should remain the restorable DeepSeek config, not the proxy config"
    );

    service
        .set_takeover_for_app("codex", false)
        .await
        .expect("disable Codex takeover");
    crate::settings::update_settings(crate::settings::AppSettings::default())
        .expect("reset settings");
}

#[tokio::test]
#[serial]
async fn codex_takeover_ignores_legacy_preserve_opt_out() {
    let _home = TempHome::new();
    crate::settings::reload_settings().expect("reload settings");
    crate::settings::update_settings(crate::settings::AppSettings {
        preserve_codex_official_auth_on_switch: false,
        ..Default::default()
    })
    .expect("store legacy Codex auth preservation opt-out");

    let db = Arc::new(Database::memory().expect("init db"));
    let service = ProxyService::new(db.clone());
    let oauth_auth = json!({
        "auth_mode": "chatgpt",
        "tokens": {
            "id_token": "oauth-id",
            "access_token": "oauth-access"
        }
    });
    let deepseek_live_config = r#"model_provider = "deepseek"
model = "deepseek-v4-flash"

[model_providers.deepseek]
name = "DeepSeek"
base_url = "https://api.deepseek.com/v1"
wire_api = "responses"
"#;
    crate::codex_config::write_codex_live_atomic(&oauth_auth, Some(deepseek_live_config))
        .expect("seed live OAuth auth with DeepSeek config");

    let mut provider = Provider::with_id(
        "deepseek".to_string(),
        "DeepSeek".to_string(),
        json!({
            "auth": {
                "OPENAI_API_KEY": "deepseek-key"
            },
            "config": r#"model_provider = "deepseek"
model = "deepseek-v4-flash"

[model_providers.deepseek]
name = "DeepSeek"
base_url = "https://api.deepseek.com/v1"
wire_api = "responses"
"#
        }),
        None,
    );
    provider.category = Some("cn_official".to_string());
    db.save_provider("codex", &provider)
        .expect("save DeepSeek provider");
    db.set_current_provider("codex", "deepseek")
        .expect("set current provider");
    crate::settings::set_current_provider(&AppType::Codex, Some("deepseek"))
        .expect("set local current provider");

    service
        .takeover_live_config_strict(&AppType::Codex)
        .await
        .expect("take over Codex live config");

    let live_auth: Value =
        crate::config::read_json_file(&crate::codex_config::get_codex_auth_path())
            .expect("read live auth");
    assert_eq!(
        live_auth, oauth_auth,
        "legacy opt-out must no longer allow proxy takeover to overwrite OAuth auth.json"
    );

    let live_config = std::fs::read_to_string(crate::codex_config::get_codex_config_path())
        .expect("read live config");
    assert!(
        live_config.contains(PROXY_TOKEN_PLACEHOLDER),
        "proxy takeover should keep its local placeholder in config.toml"
    );

    crate::settings::update_settings(crate::settings::AppSettings::default())
        .expect("reset settings");
}

#[test]
#[serial]
fn codex_takeover_cleanup_removes_config_placeholder_without_touching_oauth_auth() {
    let _home = TempHome::new();
    crate::settings::reload_settings().expect("reload settings");

    let db = Arc::new(Database::memory().expect("init db"));
    let service = ProxyService::new(db);
    let oauth_auth = json!({
        "auth_mode": "chatgpt",
        "tokens": {
            "id_token": "oauth-id",
            "access_token": "oauth-access"
        }
    });
    crate::codex_config::write_codex_live_atomic(
        &oauth_auth,
        Some(
            r#"model_provider = "deepseek"
model = "deepseek-v4-flash"

[model_providers.deepseek]
name = "DeepSeek"
base_url = "http://127.0.0.1:15721/v1"
wire_api = "responses"
experimental_bearer_token = "PROXY_MANAGED"
"#,
        ),
    )
    .expect("seed taken-over Codex live config");

    assert!(
        service.detect_takeover_in_live_config_for_app(&AppType::Codex),
        "config.toml placeholder should be detected before cleanup"
    );

    service
        .cleanup_codex_takeover_placeholders_in_live()
        .expect("cleanup Codex takeover placeholders");

    let live_auth: Value =
        crate::config::read_json_file(&crate::codex_config::get_codex_auth_path())
            .expect("read live auth");
    assert_eq!(
        live_auth, oauth_auth,
        "cleanup should preserve ChatGPT OAuth auth"
    );

    let live_config = std::fs::read_to_string(crate::codex_config::get_codex_config_path())
        .expect("read live config");
    assert!(
        !live_config.contains(PROXY_TOKEN_PLACEHOLDER),
        "cleanup should remove config.toml proxy bearer placeholder"
    );
    assert!(
        !live_config.contains("http://127.0.0.1:15721"),
        "cleanup should remove local proxy base_url"
    );
}

#[test]
#[serial]
fn codex_custom_provider_live_write_ignores_legacy_preserve_opt_out() {
    let _home = TempHome::new();
    crate::settings::reload_settings().expect("reload settings");
    crate::settings::update_settings(crate::settings::AppSettings {
        preserve_codex_official_auth_on_switch: false,
        ..Default::default()
    })
    .expect("store legacy Codex auth preservation opt-out");

    let db = Arc::new(Database::memory().expect("init db"));
    let service = ProxyService::new(db);
    let oauth_auth = json!({
        "auth_mode": "chatgpt",
        "tokens": {
            "id_token": "oauth-id",
            "access_token": "oauth-access"
        }
    });
    crate::codex_config::write_codex_live_atomic(
        &oauth_auth,
        Some(
            r#"model_provider = "openai"
model = "gpt-5-codex"
"#,
        ),
    )
    .expect("seed live OAuth auth");

    let mut provider = Provider::with_id(
        "rightcode".to_string(),
        "RightCode".to_string(),
        json!({
            "auth": {
                "OPENAI_API_KEY": "rightcode-key"
            },
            "config": r#"model_provider = "rightcode"
model = "gpt-5-codex"

[model_providers.rightcode]
name = "RightCode"
base_url = "https://rightcode.example/v1"
wire_api = "responses"
"#
        }),
        None,
    );
    provider.category = Some("custom".to_string());
    let takeover_auth = json!({
        "OPENAI_API_KEY": PROXY_TOKEN_PLACEHOLDER
    });
    let takeover_settings = json!({
        "auth": takeover_auth,
        "config": r#"model_provider = "rightcode"
model = "gpt-5-codex"

[model_providers.rightcode]
name = "RightCode"
base_url = "http://127.0.0.1:15721/v1"
wire_api = "responses"
"#
    });

    service
        .write_codex_live_for_provider(&takeover_settings, Some(&provider))
        .expect("write provider-driven Codex live config");

    let live_auth: Value =
        crate::config::read_json_file(&crate::codex_config::get_codex_auth_path())
            .expect("read live auth");
    assert_eq!(
        live_auth, oauth_auth,
        "legacy opt-out must not let third-party providers overwrite OAuth auth.json"
    );

    let live_config = std::fs::read_to_string(crate::codex_config::get_codex_config_path())
        .expect("read live config");
    assert!(
        live_config.contains(PROXY_TOKEN_PLACEHOLDER),
        "proxy takeover should keep its local placeholder in config.toml"
    );

    crate::settings::update_settings(crate::settings::AppSettings::default())
        .expect("reset settings");
}

#[test]
fn update_toml_base_url_updates_active_model_provider_base_url() {
    let input = r#"
model_provider = "any"
model = "gpt-5.1-codex"
disable_response_storage = true

[model_providers.any]
name = "any"
base_url = "https://anyrouter.top/v1"
wire_api = "responses"
requires_openai_auth = true
"#;

    let new_url = "http://127.0.0.1:5000/v1";
    let output = ProxyService::update_toml_base_url(input, new_url);

    let parsed: toml::Value = toml::from_str(&output).expect("updated config should be valid TOML");

    let base_url = parsed
        .get("model_providers")
        .and_then(|v| v.get("any"))
        .and_then(|v| v.get("base_url"))
        .and_then(|v| v.as_str())
        .expect("model_providers.any.base_url should exist");

    assert_eq!(base_url, new_url);
    assert!(
        parsed.get("base_url").is_none(),
        "should not write top-level base_url"
    );

    let wire_api = parsed
        .get("model_providers")
        .and_then(|v| v.get("any"))
        .and_then(|v| v.get("wire_api"))
        .and_then(|v| v.as_str())
        .expect("model_providers.any.wire_api should exist");
    assert_eq!(wire_api, "responses");
}

#[test]
fn apply_codex_proxy_toml_config_forces_local_responses_wire_api() {
    let input = r#"
model_provider = "chat_only"
model = "gpt-5.1-codex"

[model_providers.chat_only]
name = "Chat Only"
base_url = "https://chat-only.example/v1"
wire_api = "chat"
"#;

    let proxy_url = "http://127.0.0.1:5000/v1";
    let output = ProxyService::apply_codex_proxy_toml_config_for_provider(input, proxy_url, None);
    let parsed: toml::Value = toml::from_str(&output).expect("updated config should be valid TOML");

    let provider = parsed
        .get("model_providers")
        .and_then(|v| v.get("chat_only"))
        .expect("model_providers.chat_only should exist");

    assert_eq!(
        provider.get("base_url").and_then(|v| v.as_str()),
        Some(proxy_url)
    );
    assert_eq!(
        provider.get("wire_api").and_then(|v| v.as_str()),
        Some("responses")
    );
}

#[test]
fn apply_codex_proxy_toml_config_keeps_upstream_model_for_chat_provider() {
    let input = r#"
model_provider = "deepseek"
model = "deepseek-v4-flash"

[model_providers.deepseek]
name = "DeepSeek"
base_url = "https://api.deepseek.com/v1"
wire_api = "responses"
"#;
    let mut provider = Provider::with_id(
        "deepseek".to_string(),
        "DeepSeek".to_string(),
        json!({
            "config": input
        }),
        None,
    );
    provider.meta = Some(ProviderMeta {
        api_format: Some("openai_chat".to_string()),
        ..Default::default()
    });

    let proxy_url = "http://127.0.0.1:5000/v1";
    let output =
        ProxyService::apply_codex_proxy_toml_config_for_provider(input, proxy_url, Some(&provider));
    let parsed: toml::Value = toml::from_str(&output).expect("updated config should be valid TOML");

    assert_eq!(
        parsed.get("model").and_then(|v| v.as_str()),
        Some("deepseek-v4-flash")
    );
    assert_eq!(
        parsed
            .get("model_providers")
            .and_then(|v| v.get("deepseek"))
            .and_then(|v| v.get("base_url"))
            .and_then(|v| v.as_str()),
        Some(proxy_url)
    );
}

#[test]
fn apply_codex_proxy_toml_config_preserves_model_for_responses_provider() {
    let input = r#"
model_provider = "responses"
model = "upstream-responses-model"

[model_providers.responses]
name = "Responses"
base_url = "https://responses.example/v1"
wire_api = "responses"
"#;
    let mut provider = Provider::with_id(
        "responses".to_string(),
        "Responses".to_string(),
        json!({
            "config": input
        }),
        None,
    );
    provider.meta = Some(ProviderMeta {
        api_format: Some("openai_responses".to_string()),
        ..Default::default()
    });

    let output = ProxyService::apply_codex_proxy_toml_config_for_provider(
        input,
        "http://127.0.0.1:5000/v1",
        Some(&provider),
    );
    let parsed: toml::Value = toml::from_str(&output).expect("updated config should be valid TOML");

    assert_eq!(
        parsed.get("model").and_then(|v| v.as_str()),
        Some("upstream-responses-model")
    );
}

#[test]
fn apply_codex_proxy_toml_config_restores_upstream_model_for_responses_provider() {
    let input = r#"
model_provider = "responses"
model = "gpt-5.4"

[model_providers.responses]
name = "Responses"
base_url = "http://127.0.0.1:5000/v1"
wire_api = "responses"
"#;
    let mut provider = Provider::with_id(
        "responses".to_string(),
        "Responses".to_string(),
        json!({
            "config": r#"model_provider = "responses"
model = "upstream-responses-model"

[model_providers.responses]
name = "Responses"
base_url = "https://responses.example/v1"
wire_api = "responses"
"#
        }),
        None,
    );
    provider.meta = Some(ProviderMeta {
        api_format: Some("openai_responses".to_string()),
        ..Default::default()
    });

    let output = ProxyService::apply_codex_proxy_toml_config_for_provider(
        input,
        "http://127.0.0.1:5000/v1",
        Some(&provider),
    );
    let parsed: toml::Value = toml::from_str(&output).expect("updated config should be valid TOML");

    assert_eq!(
        parsed.get("model").and_then(|v| v.as_str()),
        Some("upstream-responses-model")
    );
}

#[test]
fn update_toml_base_url_falls_back_to_top_level_base_url() {
    let input = r#"
model = "gpt-5.1-codex"
"#;

    let new_url = "http://127.0.0.1:5000/v1";
    let output = ProxyService::update_toml_base_url(input, new_url);

    let parsed: toml::Value = toml::from_str(&output).expect("updated config should be valid TOML");

    let base_url = parsed
        .get("base_url")
        .and_then(|v| v.as_str())
        .expect("base_url should exist");

    assert_eq!(base_url, new_url);
}

#[tokio::test]
#[serial]
async fn sync_claude_token_does_not_add_anthropic_api_key() {
    let _home = TempHome::new();
    crate::settings::reload_settings().expect("reload settings");

    let db = Arc::new(Database::memory().expect("init db"));
    let service = ProxyService::new(db.clone());

    let provider = Provider::with_id(
        "p1".to_string(),
        "P1".to_string(),
        json!({
            "env": {
                "ANTHROPIC_BASE_URL": "https://api.anthropic.com",
                "ANTHROPIC_AUTH_TOKEN": "stale"
            }
        }),
        None,
    );
    db.save_provider("claude", &provider)
        .expect("save provider");
    db.set_current_provider("claude", "p1")
        .expect("set current provider");

    let live_config = json!({
        "env": {
            "ANTHROPIC_AUTH_TOKEN": "fresh"
        }
    });

    service
        .sync_live_config_to_provider(&AppType::Claude, &live_config)
        .await
        .expect("sync");

    let updated = db
        .get_provider_by_id("p1", "claude")
        .expect("get provider")
        .expect("provider exists");
    let env = updated
        .settings_config
        .get("env")
        .and_then(|v| v.as_object())
        .expect("env object");

    assert_eq!(
        env.get("ANTHROPIC_AUTH_TOKEN").and_then(|v| v.as_str()),
        Some("fresh")
    );
    assert!(
        !env.contains_key("ANTHROPIC_API_KEY"),
        "should not add ANTHROPIC_API_KEY when absent"
    );
}

#[tokio::test]
#[serial]
async fn sync_claude_token_respects_existing_api_key_field() {
    let _home = TempHome::new();
    crate::settings::reload_settings().expect("reload settings");

    let db = Arc::new(Database::memory().expect("init db"));
    let service = ProxyService::new(db.clone());

    let provider = Provider::with_id(
        "p1".to_string(),
        "P1".to_string(),
        json!({
            "env": {
                "ANTHROPIC_BASE_URL": "https://api.anthropic.com",
                "ANTHROPIC_API_KEY": "stale"
            }
        }),
        None,
    );
    db.save_provider("claude", &provider)
        .expect("save provider");
    db.set_current_provider("claude", "p1")
        .expect("set current provider");

    let live_config = json!({
        "env": {
            "ANTHROPIC_AUTH_TOKEN": "fresh"
        }
    });

    service
        .sync_live_config_to_provider(&AppType::Claude, &live_config)
        .await
        .expect("sync");

    let updated = db
        .get_provider_by_id("p1", "claude")
        .expect("get provider")
        .expect("provider exists");
    let env = updated
        .settings_config
        .get("env")
        .and_then(|v| v.as_object())
        .expect("env object");

    assert_eq!(
        env.get("ANTHROPIC_API_KEY").and_then(|v| v.as_str()),
        Some("fresh")
    );
    assert!(
        !env.contains_key("ANTHROPIC_AUTH_TOKEN"),
        "should not add ANTHROPIC_AUTH_TOKEN when absent"
    );
}

#[tokio::test]
#[serial]
async fn switch_proxy_target_updates_live_backup_when_taken_over() {
    let _home = TempHome::new();
    crate::settings::reload_settings().expect("reload settings");

    let db = Arc::new(Database::memory().expect("init db"));
    let service = ProxyService::new(db.clone());

    let provider_a = Provider::with_id(
        "a".to_string(),
        "A".to_string(),
        json!({
            "env": {
                "ANTHROPIC_API_KEY": "a-key"
            }
        }),
        None,
    );
    let provider_b = Provider::with_id(
        "b".to_string(),
        "B".to_string(),
        json!({
            "env": {
                "ANTHROPIC_API_KEY": "b-key"
            }
        }),
        None,
    );
    db.save_provider("claude", &provider_a)
        .expect("save provider a");
    db.save_provider("claude", &provider_b)
        .expect("save provider b");
    db.set_current_provider("claude", "a")
        .expect("set current provider");

    // 模拟"已接管"状态：存在 Live 备份（内容不重要，会被热切换更新）
    db.save_live_backup("claude", "{\"env\":{}}")
        .await
        .expect("seed live backup");

    service
        .switch_proxy_target("claude", "b")
        .await
        .expect("switch proxy target");

    // 断言：本地 settings 的 current provider 已同步
    assert_eq!(
        crate::settings::get_current_provider(&AppType::Claude).as_deref(),
        Some("b")
    );

    // 断言：Live 备份已更新为目标供应商配置（用于 stop_with_restore 恢复）
    let backup = db
        .get_live_backup("claude")
        .await
        .expect("get live backup")
        .expect("backup exists");
    let expected = serde_json::to_string(&provider_b.settings_config).expect("serialize");
    assert_eq!(backup.original_config, expected);
}

#[tokio::test]
#[serial]
async fn hot_switch_provider_updates_claude_live_while_preserving_takeover_fields() {
    let _home = TempHome::new();
    crate::settings::reload_settings().expect("reload settings");

    let db = Arc::new(Database::memory().expect("init db"));
    let service = ProxyService::new(db.clone());

    let provider_a = Provider::with_id(
        "a".to_string(),
        "A".to_string(),
        json!({
            "env": {
                "ANTHROPIC_API_KEY": "a-key",
                "ANTHROPIC_BASE_URL": "https://api.a.example",
                "ANTHROPIC_MODEL": "claude-old"
            },
            "permissions": { "allow": ["Bash"] }
        }),
        None,
    );
    let provider_b = Provider::with_id(
        "b".to_string(),
        "B".to_string(),
        json!({
            "env": {
                "ANTHROPIC_API_KEY": "b-key",
                "ANTHROPIC_BASE_URL": "https://api.b.example",
                "ANTHROPIC_MODEL": "claude-new",
                "ANTHROPIC_DEFAULT_HAIKU_MODEL": "deepseek-v4-flash",
                "ANTHROPIC_DEFAULT_HAIKU_MODEL_NAME": "DeepSeek V4 Flash",
                "ANTHROPIC_DEFAULT_SONNET_MODEL": "deepseek-v4-pro[1M]",
                "ANTHROPIC_DEFAULT_SONNET_MODEL_NAME": "DeepSeek V4 Pro",
                "ANTHROPIC_DEFAULT_OPUS_MODEL": "deepseek-v4-ultra [1m]"
            },
            "permissions": { "allow": ["Read"] }
        }),
        None,
    );

    db.save_provider("claude", &provider_a)
        .expect("save provider a");
    db.save_provider("claude", &provider_b)
        .expect("save provider b");
    db.set_current_provider("claude", "a")
        .expect("set current provider");
    crate::settings::set_current_provider(&AppType::Claude, Some("a"))
        .expect("set local current provider");
    db.save_live_backup(
        "claude",
        &serde_json::to_string(&provider_a.settings_config).expect("serialize provider a"),
    )
    .await
    .expect("seed live backup");
    service
        .write_claude_live(&json!({
            "env": {
                "ANTHROPIC_BASE_URL": "http://127.0.0.1:15721",
                "ANTHROPIC_API_KEY": PROXY_TOKEN_PLACEHOLDER,
                "ANTHROPIC_MODEL": "stale-model",
                "ANTHROPIC_DEFAULT_SONNET_MODEL_NAME": "Stale Sonnet"
            },
            "permissions": { "allow": ["Bash"] }
        }))
        .expect("seed taken-over live file");

    service
        .hot_switch_provider("claude", "b")
        .await
        .expect("hot switch provider");

    let live = service.read_claude_live().expect("read live config");
    assert_eq!(
        live.get("permissions"),
        provider_b.settings_config.get("permissions"),
        "provider-derived live settings should be refreshed"
    );
    assert_eq!(
        live.get("env")
            .and_then(|env| env.get("ANTHROPIC_API_KEY"))
            .and_then(|v| v.as_str()),
        Some(PROXY_TOKEN_PLACEHOLDER),
        "takeover token placeholder should be preserved"
    );
    assert_eq!(
        live.get("env")
            .and_then(|env| env.get("ANTHROPIC_BASE_URL"))
            .and_then(|v| v.as_str()),
        Some("http://127.0.0.1:15721"),
        "takeover proxy URL should remain active"
    );
    assert!(
        live.get("env")
            .and_then(|env| env.get("ANTHROPIC_MODEL"))
            .is_none(),
        "fallback model override should be removed in takeover mode"
    );
    let live_env = live
        .get("env")
        .and_then(|env| env.as_object())
        .expect("live env");
    assert_eq!(
        live_env
            .get("ANTHROPIC_DEFAULT_HAIKU_MODEL")
            .and_then(|v| v.as_str()),
        Some("claude-haiku-4-5"),
        "takeover mode should expose a stable Haiku role model"
    );
    assert_eq!(
        live_env
            .get("ANTHROPIC_DEFAULT_HAIKU_MODEL_NAME")
            .and_then(|v| v.as_str()),
        Some("DeepSeek V4 Flash"),
        "model menu should show the current provider Haiku display name"
    );
    assert_eq!(
        live_env
            .get("ANTHROPIC_DEFAULT_SONNET_MODEL")
            .and_then(|v| v.as_str()),
        Some("claude-sonnet-4-6[1M]"),
        "Sonnet role should carry the local 1M declaration for Claude Code"
    );
    assert_eq!(
        live_env
            .get("ANTHROPIC_DEFAULT_SONNET_MODEL_NAME")
            .and_then(|v| v.as_str()),
        Some("DeepSeek V4 Pro"),
        "stale model display names should be replaced during hot switch"
    );
    assert_eq!(
        live_env
            .get("ANTHROPIC_DEFAULT_OPUS_MODEL")
            .and_then(|v| v.as_str()),
        Some("claude-opus-4-8[1M]"),
        "Opus role should preserve the current provider 1M capability marker"
    );
    assert_eq!(
        live_env
            .get("ANTHROPIC_DEFAULT_OPUS_MODEL_NAME")
            .and_then(|v| v.as_str()),
        Some("deepseek-v4-ultra"),
        "implicit display names should strip the local 1M marker"
    );

    let backup = db
        .get_live_backup("claude")
        .await
        .expect("get live backup")
        .expect("backup exists");
    let expected = serde_json::to_string(&provider_b.settings_config).expect("serialize");
    assert_eq!(backup.original_config, expected);
}

#[tokio::test]
#[serial]
async fn hot_switch_provider_serializes_same_app_switches() {
    use tokio::time::{sleep, Duration};

    let _home = TempHome::new();
    crate::settings::reload_settings().expect("reload settings");

    let db = Arc::new(Database::memory().expect("init db"));
    let service = ProxyService::new(db.clone());

    let provider_a = Provider::with_id(
        "a".to_string(),
        "A".to_string(),
        json!({ "env": { "ANTHROPIC_API_KEY": "a-key" } }),
        None,
    );
    let provider_b = Provider::with_id(
        "b".to_string(),
        "B".to_string(),
        json!({ "env": { "ANTHROPIC_API_KEY": "b-key" } }),
        None,
    );
    let provider_c = Provider::with_id(
        "c".to_string(),
        "C".to_string(),
        json!({ "env": { "ANTHROPIC_API_KEY": "c-key" } }),
        None,
    );

    db.save_provider("claude", &provider_a)
        .expect("save provider a");
    db.save_provider("claude", &provider_b)
        .expect("save provider b");
    db.save_provider("claude", &provider_c)
        .expect("save provider c");
    db.set_current_provider("claude", "a")
        .expect("set current provider");
    crate::settings::set_current_provider(&AppType::Claude, Some("a"))
        .expect("set local current provider");
    db.save_live_backup("claude", "{\"env\":{}}")
        .await
        .expect("seed live backup");

    let guard = service.lock_switch_for_test("claude").await;
    let service_for_b = service.clone();
    let service_for_c = service.clone();

    let switch_b = tokio::spawn(async move {
        service_for_b
            .hot_switch_provider("claude", "b")
            .await
            .expect("switch to b")
    });
    sleep(Duration::from_millis(20)).await;
    let switch_c = tokio::spawn(async move {
        service_for_c
            .hot_switch_provider("claude", "c")
            .await
            .expect("switch to c")
    });

    sleep(Duration::from_millis(20)).await;
    drop(guard);

    let outcome_b = switch_b.await.expect("join switch b");
    let outcome_c = switch_c.await.expect("join switch c");
    assert!(outcome_b.logical_target_changed);
    assert!(outcome_c.logical_target_changed);

    assert_eq!(
        crate::settings::get_effective_current_provider(&db, &AppType::Claude)
            .expect("effective current"),
        Some("c".to_string())
    );
    assert_eq!(
        crate::settings::get_current_provider(&AppType::Claude).as_deref(),
        Some("c")
    );
    assert_eq!(
        db.get_current_provider("claude").expect("db current"),
        Some("c".to_string())
    );

    let backup = db
        .get_live_backup("claude")
        .await
        .expect("get live backup")
        .expect("backup exists");
    let expected = serde_json::to_string(&provider_c.settings_config).expect("serialize");
    assert_eq!(backup.original_config, expected);
}

#[tokio::test]
#[serial]
async fn restore_waits_for_hot_switch_and_restores_latest_backup() {
    use tokio::time::{sleep, Duration};

    let _home = TempHome::new();
    crate::settings::reload_settings().expect("reload settings");

    let db = Arc::new(Database::memory().expect("init db"));
    let service = ProxyService::new(db.clone());

    let provider_a = Provider::with_id(
        "a".to_string(),
        "A".to_string(),
        json!({ "env": { "ANTHROPIC_API_KEY": "a-key" } }),
        None,
    );
    let provider_b = Provider::with_id(
        "b".to_string(),
        "B".to_string(),
        json!({ "env": { "ANTHROPIC_API_KEY": "b-key" } }),
        None,
    );

    db.save_provider("claude", &provider_a)
        .expect("save provider a");
    db.save_provider("claude", &provider_b)
        .expect("save provider b");
    db.set_current_provider("claude", "a")
        .expect("set current provider");
    crate::settings::set_current_provider(&AppType::Claude, Some("a"))
        .expect("set local current provider");
    db.save_live_backup(
        "claude",
        &serde_json::to_string(&provider_a.settings_config).expect("serialize provider a"),
    )
    .await
    .expect("seed live backup");
    service
        .write_claude_live(&json!({ "env": { "ANTHROPIC_API_KEY": "stale" } }))
        .expect("seed live file");

    let guard = service.lock_switch_for_test("claude").await;
    let service_for_switch = service.clone();
    let service_for_restore = service.clone();

    let switch_to_b = tokio::spawn(async move {
        service_for_switch
            .hot_switch_provider("claude", "b")
            .await
            .expect("switch to b")
    });
    sleep(Duration::from_millis(20)).await;
    let restore = tokio::spawn(async move {
        service_for_restore
            .restore_live_config_for_app_with_fallback(&AppType::Claude)
            .await
            .expect("restore claude live")
    });

    sleep(Duration::from_millis(20)).await;
    drop(guard);

    let outcome = switch_to_b.await.expect("join switch");
    restore.await.expect("join restore");
    assert!(outcome.logical_target_changed);

    assert_eq!(
        crate::settings::get_effective_current_provider(&db, &AppType::Claude)
            .expect("effective current"),
        Some("b".to_string())
    );

    let backup = db
        .get_live_backup("claude")
        .await
        .expect("get live backup")
        .expect("backup exists");
    let expected = serde_json::to_string(&provider_b.settings_config).expect("serialize");
    assert_eq!(backup.original_config, expected);
    assert_eq!(
        service.read_claude_live().expect("read live"),
        provider_b.settings_config
    );
}

#[tokio::test]
#[serial]
async fn update_live_backup_from_provider_applies_claude_common_config() {
    let _home = TempHome::new();
    crate::settings::reload_settings().expect("reload settings");

    let db = Arc::new(Database::memory().expect("init db"));
    db.set_config_snippet(
        "claude",
        Some(
            serde_json::json!({
                "includeCoAuthoredBy": false
            })
            .to_string(),
        ),
    )
    .expect("set common config snippet");

    let service = ProxyService::new(db.clone());

    let mut provider = Provider::with_id(
        "p1".to_string(),
        "P1".to_string(),
        json!({
            "env": {
                "ANTHROPIC_AUTH_TOKEN": "token",
                "ANTHROPIC_BASE_URL": "https://claude.example"
            }
        }),
        None,
    );
    provider.meta = Some(ProviderMeta {
        common_config_enabled: Some(true),
        ..Default::default()
    });

    service
        .update_live_backup_from_provider("claude", &provider)
        .await
        .expect("update live backup");

    let backup = db
        .get_live_backup("claude")
        .await
        .expect("get live backup")
        .expect("backup exists");
    let stored: Value = serde_json::from_str(&backup.original_config).expect("parse backup json");

    assert_eq!(
        stored.get("includeCoAuthoredBy").and_then(|v| v.as_bool()),
        Some(false),
        "common config should be applied into Claude restore backup"
    );
}

#[tokio::test]
#[serial]
async fn update_live_backup_from_provider_applies_codex_common_config() {
    let _home = TempHome::new();
    crate::settings::reload_settings().expect("reload settings");

    let db = Arc::new(Database::memory().expect("init db"));
    db.set_config_snippet(
        "codex",
        Some("disable_response_storage = true\n".to_string()),
    )
    .expect("set common config snippet");

    let service = ProxyService::new(db.clone());

    let mut provider = Provider::with_id(
        "p1".to_string(),
        "P1".to_string(),
        json!({
            "auth": {
                "OPENAI_API_KEY": "token"
            },
            "config": r#"model_provider = "any"
model = "gpt-5"

[model_providers.any]
base_url = "https://codex.example/v1"
"#
        }),
        None,
    );
    provider.meta = Some(ProviderMeta {
        common_config_enabled: Some(true),
        ..Default::default()
    });

    service
        .update_live_backup_from_provider("codex", &provider)
        .await
        .expect("update live backup");

    let backup = db
        .get_live_backup("codex")
        .await
        .expect("get live backup")
        .expect("backup exists");
    let stored: Value = serde_json::from_str(&backup.original_config).expect("parse backup json");
    let config = stored
        .get("config")
        .and_then(|v| v.as_str())
        .expect("config string");

    assert!(
        config.contains("disable_response_storage = true"),
        "common config should be applied into Codex restore backup"
    );
}

#[tokio::test]
#[serial]
async fn update_live_backup_from_provider_preserves_codex_mcp_servers() {
    let _home = TempHome::new();
    crate::settings::reload_settings().expect("reload settings");

    let db = Arc::new(Database::memory().expect("init db"));
    let service = ProxyService::new(db.clone());

    db.save_live_backup(
        "codex",
        &serde_json::to_string(&json!({
            "auth": {
                "OPENAI_API_KEY": "old-token"
            },
            "config": r#"model_provider = "any"
model = "gpt-4"

[model_providers.any]
base_url = "https://old.example/v1"

[mcp_servers.echo]
command = "npx"
args = ["echo-server"]
"#
        }))
        .expect("serialize seed backup"),
    )
    .await
    .expect("seed live backup");

    let provider = Provider::with_id(
        "p2".to_string(),
        "P2".to_string(),
        json!({
            "auth": {
                "OPENAI_API_KEY": "new-token"
            },
            "config": r#"model_provider = "any"
model = "gpt-5"

[model_providers.any]
base_url = "https://new.example/v1"
"#
        }),
        None,
    );

    service
        .update_live_backup_from_provider("codex", &provider)
        .await
        .expect("update live backup");

    let backup = db
        .get_live_backup("codex")
        .await
        .expect("get live backup")
        .expect("backup exists");
    let stored: Value = serde_json::from_str(&backup.original_config).expect("parse backup json");
    let config = stored
        .get("config")
        .and_then(|v| v.as_str())
        .expect("config string");

    assert!(
        config.contains("[mcp_servers.echo]"),
        "existing Codex MCP section should survive proxy hot-switch backup update"
    );
    assert!(
        config.contains("https://new.example/v1"),
        "provider-specific base_url should still update to the new provider"
    );
}

/// 接管中的 live 备份 auth 没有 ChatGPT 登录材料时，
/// `preserve_codex_oauth_auth_in_backup` 会直接早退、不重写 config。
/// 此时若热切换到旧版本创建的无 key 第三方 provider（存储配置仍带
/// `requires_openai_auth`/`env_key`），未消毒的 config 会进备份，接管释放
/// 时被 `write_codex_live_verbatim` 原样写回 live，Codex 就会把 ChatGPT
/// 登录态 / shell 环境变量里的 OPENAI_API_KEY 发给第三方 base_url。
#[tokio::test]
#[serial]
async fn hot_switch_codex_backup_strips_openai_auth_fields_without_oauth_backup_auth() {
    let _home = TempHome::new();
    crate::settings::reload_settings().expect("reload settings");

    let db = Arc::new(Database::memory().expect("init db"));
    let service = ProxyService::new(db.clone());

    let current = Provider::with_id(
        "current".to_string(),
        "RightCode".to_string(),
        json!({
            "auth": {
                "OPENAI_API_KEY": "rightcode-key"
            },
            "config": r#"model_provider = "rightcode"
model = "gpt-5.4"

[model_providers.rightcode]
name = "RightCode"
base_url = "https://rightcode.example/v1"
wire_api = "responses"
"#
        }),
        None,
    );
    // 旧版本模板：无 key，且带着 requires_openai_auth / env_key。
    let mut legacy_keyless = Provider::with_id(
        "legacy".to_string(),
        "Legacy Vendor".to_string(),
        json!({
            "auth": {
                "OPENAI_API_KEY": ""
            },
            "config": r#"model_provider = "custom"
model = "gpt-5.5"

[model_providers.custom]
name = "Legacy Vendor"
base_url = "https://legacy.example/v1"
env_key = "OPENAI_API_KEY"
wire_api = "responses"
requires_openai_auth = true
"#
        }),
        None,
    );
    legacy_keyless.category = Some("third_party".to_string());

    db.save_provider("codex", &current)
        .expect("save current provider");
    db.save_provider("codex", &legacy_keyless)
        .expect("save legacy provider");
    db.set_current_provider("codex", "current")
        .expect("set current provider");
    crate::settings::set_current_provider(&AppType::Codex, Some("current"))
        .expect("set local current provider");

    // 现存备份的 auth 只有 API key，没有 ChatGPT OAuth 材料 —— 早退分支。
    db.save_live_backup(
        "codex",
        &serde_json::to_string(&current.settings_config).expect("serialize current provider"),
    )
    .await
    .expect("seed live backup");
    assert!(
        !crate::codex_config::codex_auth_has_oauth_login_material(
            current.settings_config.get("auth").expect("provider auth")
        ),
        "this regression only reproduces when the backup auth carries no ChatGPT login"
    );

    service
        .write_codex_live(&json!({
            "auth": {
                "OPENAI_API_KEY": PROXY_TOKEN_PLACEHOLDER
            },
            "config": r#"model_provider = "rightcode"
model = "gpt-5.4"

[model_providers.rightcode]
name = "RightCode"
base_url = "http://127.0.0.1:15721/v1"
wire_api = "responses"
"#
        }))
        .expect("seed taken-over Codex live config");

    service
        .hot_switch_provider("codex", "legacy")
        .await
        .expect("hot switch Codex provider");

    let backup = db
        .get_live_backup("codex")
        .await
        .expect("get live backup")
        .expect("backup exists");
    let stored: Value = serde_json::from_str(&backup.original_config).expect("parse backup json");
    let backup_config = stored
        .get("config")
        .and_then(|v| v.as_str())
        .expect("backup config string");

    assert!(
        !backup_config.contains("requires_openai_auth"),
        "restore backup must not route third-party auth to the ChatGPT login: {backup_config}"
    );
    assert!(
        !backup_config.contains("env_key"),
        "restore backup must not pull third-party credentials from the environment: {backup_config}"
    );

    service
        .restore_live_config_for_app_with_fallback(&AppType::Codex)
        .await
        .expect("restore Codex live config");

    let live = service.read_codex_live().expect("read Codex live config");
    let live_config = live
        .get("config")
        .and_then(|v| v.as_str())
        .expect("live config string");
    assert!(
        !live_config.contains("requires_openai_auth") && !live_config.contains("env_key"),
        "releasing the takeover must not restore OpenAI-auth routing for a third-party provider: {live_config}"
    );

    let parsed_live: toml::Value = toml::from_str(live_config).expect("parse live config");
    assert_eq!(
        parsed_live
            .get("model_providers")
            .and_then(|v| v.get("custom"))
            .and_then(|v| v.get("base_url"))
            .and_then(|v| v.as_str()),
        Some("https://legacy.example/v1"),
        "unrelated provider fields must survive the sanitize"
    );
}

#[tokio::test]
#[serial]
async fn hot_switch_codex_provider_preserves_provider_model_provider_in_backup_and_restore() {
    let _home = TempHome::new();
    crate::settings::reload_settings().expect("reload settings");

    let db = Arc::new(Database::memory().expect("init db"));
    let service = ProxyService::new(db.clone());

    let provider_a = Provider::with_id(
        "a".to_string(),
        "RightCode".to_string(),
        json!({
            "auth": {
                "OPENAI_API_KEY": "rightcode-key"
            },
            "config": r#"model_provider = "rightcode"
model = "gpt-5.4"

[model_providers.rightcode]
name = "RightCode"
base_url = "https://rightcode.example/v1"
wire_api = "responses"
requires_openai_auth = true
"#
        }),
        None,
    );
    let provider_b = Provider::with_id(
        "b".to_string(),
        "AiHubMix".to_string(),
        json!({
            "auth": {
                "OPENAI_API_KEY": "aihubmix-key"
            },
            "config": r#"model_provider = "aihubmix"
model = "gpt-5.4"

[model_providers.aihubmix]
name = "AiHubMix"
base_url = "https://aihubmix.example/v1"
wire_api = "responses"
requires_openai_auth = true
"#
        }),
        None,
    );

    db.save_provider("codex", &provider_a)
        .expect("save provider a");
    db.save_provider("codex", &provider_b)
        .expect("save provider b");
    db.set_current_provider("codex", "a")
        .expect("set current provider");
    crate::settings::set_current_provider(&AppType::Codex, Some("a"))
        .expect("set local current provider");
    db.save_live_backup(
        "codex",
        &serde_json::to_string(&provider_a.settings_config).expect("serialize provider a"),
    )
    .await
    .expect("seed live backup");
    service
        .write_codex_live(&json!({
            "auth": {
                "OPENAI_API_KEY": PROXY_TOKEN_PLACEHOLDER
            },
            "config": r#"model_provider = "rightcode"
model = "gpt-5.4"

[model_providers.rightcode]
name = "RightCode"
base_url = "http://127.0.0.1:15721/v1"
wire_api = "responses"
requires_openai_auth = true
"#
        }))
        .expect("seed taken-over Codex live config");

    service
        .hot_switch_provider("codex", "b")
        .await
        .expect("hot switch Codex provider");

    let backup = db
        .get_live_backup("codex")
        .await
        .expect("get live backup")
        .expect("backup exists");
    let stored: Value = serde_json::from_str(&backup.original_config).expect("parse backup json");
    let backup_config = stored
        .get("config")
        .and_then(|v| v.as_str())
        .expect("backup config string");
    let parsed_backup: toml::Value = toml::from_str(backup_config).expect("parse backup config");
    assert_eq!(
        parsed_backup.get("model_provider").and_then(|v| v.as_str()),
        Some("aihubmix"),
        "provider-derived restore backup should preserve the provider's model_provider"
    );
    let backup_model_providers = parsed_backup
        .get("model_providers")
        .and_then(|v| v.as_table())
        .expect("backup model_providers");
    assert!(backup_model_providers.get("custom").is_none());
    assert_eq!(
        backup_model_providers
            .get("aihubmix")
            .and_then(|v| v.get("base_url"))
            .and_then(|v| v.as_str()),
        Some("https://aihubmix.example/v1"),
        "provider id should point at the hot-switched provider endpoint"
    );

    let live = service.read_codex_live().expect("read Codex live config");
    let live_config = live
        .get("config")
        .and_then(|v| v.as_str())
        .expect("live config string");
    let parsed_live: toml::Value = toml::from_str(live_config).expect("parse live config");
    assert_eq!(
        parsed_live.get("model_provider").and_then(|v| v.as_str()),
        Some("aihubmix"),
        "hot-switched Codex live config should expose the selected provider"
    );
    assert_eq!(
        parsed_live
            .get("model_providers")
            .and_then(|v| v.get("aihubmix"))
            .and_then(|v| v.get("name"))
            .and_then(|v| v.as_str()),
        Some("AiHubMix"),
        "Codex app provider label should follow the selected provider"
    );
    assert_eq!(
        parsed_live
            .get("model_providers")
            .and_then(|v| v.get("aihubmix"))
            .and_then(|v| v.get("base_url"))
            .and_then(|v| v.as_str()),
        Some("http://127.0.0.1:15721/v1"),
        "taken-over live config should stay pointed at the local proxy"
    );

    service
        .restore_live_config_for_app_with_fallback(&AppType::Codex)
        .await
        .expect("restore Codex live config");

    let live = service.read_codex_live().expect("read Codex live config");
    let live_config = live
        .get("config")
        .and_then(|v| v.as_str())
        .expect("live config string");
    let parsed_live: toml::Value = toml::from_str(live_config).expect("parse live config");
    assert_eq!(
        parsed_live.get("model_provider").and_then(|v| v.as_str()),
        Some("aihubmix"),
        "restored Codex live config should preserve the provider's model_provider"
    );
    assert_eq!(
        live.get("auth")
            .and_then(|auth| auth.get("OPENAI_API_KEY"))
            .and_then(|v| v.as_str()),
        Some("aihubmix-key"),
        "restore should still use the hot-switched provider auth"
    );
}

#[tokio::test]
#[serial]
async fn hot_switch_codex_chat_provider_updates_live_provider_display() {
    let _home = TempHome::new();
    crate::settings::reload_settings().expect("reload settings");

    let db = Arc::new(Database::memory().expect("init db"));
    let service = ProxyService::new(db.clone());

    let provider_a = Provider::with_id(
        "a".to_string(),
        "Responses".to_string(),
        json!({
            "auth": {
                "OPENAI_API_KEY": "responses-key"
            },
            "config": r#"model_provider = "stable"
model = "responses-model"

[model_providers.stable]
name = "Stable"
base_url = "https://responses.example/v1"
wire_api = "responses"
requires_openai_auth = true
"#
        }),
        None,
    );
    let mut provider_b = Provider::with_id(
        "b".to_string(),
        "DeepSeek".to_string(),
        json!({
            "auth": {
                "OPENAI_API_KEY": "deepseek-key"
            },
            "config": r#"model_provider = "deepseek"
model = "deepseek-v4-flash"

[model_providers.deepseek]
name = "DeepSeek"
base_url = "https://api.deepseek.com/v1"
wire_api = "responses"
requires_openai_auth = true
"#
        }),
        None,
    );
    provider_b.meta = Some(ProviderMeta {
        api_format: Some("openai_chat".to_string()),
        ..Default::default()
    });

    db.save_provider("codex", &provider_a)
        .expect("save provider a");
    db.save_provider("codex", &provider_b)
        .expect("save provider b");
    db.set_current_provider("codex", "a")
        .expect("set current provider");
    crate::settings::set_current_provider(&AppType::Codex, Some("a"))
        .expect("set local current provider");
    db.save_live_backup(
        "codex",
        &serde_json::to_string(&provider_a.settings_config).expect("serialize provider a"),
    )
    .await
    .expect("seed live backup");
    service
        .write_codex_live(&json!({
            "auth": {
                "OPENAI_API_KEY": PROXY_TOKEN_PLACEHOLDER
            },
            "config": r#"model_provider = "stable"
model = "responses-model"

[model_providers.stable]
name = "Stable"
base_url = "http://127.0.0.1:15721/v1"
wire_api = "responses"
requires_openai_auth = true
"#
        }))
        .expect("seed taken-over Codex live config");

    service
        .hot_switch_provider("codex", "b")
        .await
        .expect("hot switch Codex provider");

    let live = service.read_codex_live().expect("read Codex live config");
    let live_config = live
        .get("config")
        .and_then(|v| v.as_str())
        .expect("live config string");
    let parsed_live: toml::Value = toml::from_str(live_config).expect("parse live config");

    assert_eq!(
        parsed_live.get("model_provider").and_then(|v| v.as_str()),
        Some("deepseek")
    );
    assert_eq!(
        parsed_live
            .get("model_providers")
            .and_then(|v| v.get("deepseek"))
            .and_then(|v| v.get("name"))
            .and_then(|v| v.as_str()),
        Some("DeepSeek")
    );
    assert_eq!(
        parsed_live
            .get("model_providers")
            .and_then(|v| v.get("deepseek"))
            .and_then(|v| v.get("base_url"))
            .and_then(|v| v.as_str()),
        Some("http://127.0.0.1:15721/v1")
    );
    assert_eq!(
        parsed_live.get("model").and_then(|v| v.as_str()),
        Some("deepseek-v4-flash")
    );
    assert_eq!(
        live.get("auth")
            .and_then(|auth| auth.get("OPENAI_API_KEY"))
            .and_then(|v| v.as_str()),
        Some(PROXY_TOKEN_PLACEHOLDER)
    );
}

#[tokio::test]
#[serial]
async fn update_live_backup_from_provider_keeps_only_existing_codex_mcp_entries() {
    let _home = TempHome::new();
    crate::settings::reload_settings().expect("reload settings");

    let db = Arc::new(Database::memory().expect("init db"));
    let service = ProxyService::new(db.clone());

    db.save_live_backup(
        "codex",
        &serde_json::to_string(&json!({
            "auth": {
                "OPENAI_API_KEY": "old-token"
            },
            "config": r#"[mcp_servers.shared]
command = "old-command"

[mcp_servers.legacy]
command = "legacy-command"
"#
        }))
        .expect("serialize seed backup"),
    )
    .await
    .expect("seed live backup");

    let provider = Provider::with_id(
        "p2".to_string(),
        "P2".to_string(),
        json!({
            "auth": {
                "OPENAI_API_KEY": "new-token"
            },
            "config": r#"[mcp_servers.shared]
command = "new-command"

[mcp_servers.latest]
command = "latest-command"
"#
        }),
        None,
    );

    service
        .update_live_backup_from_provider("codex", &provider)
        .await
        .expect("update live backup");

    let backup = db
        .get_live_backup("codex")
        .await
        .expect("get live backup")
        .expect("backup exists");
    let stored: Value = serde_json::from_str(&backup.original_config).expect("parse backup json");
    let config = stored
        .get("config")
        .and_then(|v| v.as_str())
        .expect("config string");
    let parsed: toml::Value = toml::from_str(config).expect("parse merged codex config");

    let mcp_servers = parsed
        .get("mcp_servers")
        .expect("mcp_servers should be present");
    assert_eq!(
        mcp_servers
            .get("shared")
            .and_then(|v| v.get("command"))
            .and_then(|v| v.as_str()),
        Some("old-command"),
        "the independently managed backup definition should win on conflict"
    );
    assert_eq!(
        mcp_servers
            .get("legacy")
            .and_then(|v| v.get("command"))
            .and_then(|v| v.as_str()),
        Some("legacy-command"),
        "backup-only MCP entries should still be preserved"
    );
    assert_eq!(
        mcp_servers
            .get("latest")
            .and_then(|v| v.get("command"))
            .and_then(|v| v.as_str()),
        None,
        "MCP entries embedded in a provider snapshot must not enter the restore backup"
    );
}

#[tokio::test]
#[serial]
async fn provider_switch_with_restored_codex_backup_refreshes_catalog_and_common_config() {
    let _home = TempHome::new();
    crate::settings::reload_settings().expect("reload settings");
    seed_codex_model_template();

    let db = Arc::new(Database::memory().expect("init db"));
    let state = crate::store::AppState::new(db.clone());

    db.set_config_snippet(
        "codex",
        Some(
            r#"[features]
disable_response_storage = true
"#
            .to_string(),
        ),
    )
    .expect("set common config snippet");

    let proxy_config = ProxyConfig {
        listen_port: 0,
        ..Default::default()
    };
    db.update_proxy_config(proxy_config)
        .await
        .expect("set test proxy config");
    state
        .proxy_service
        .start()
        .await
        .expect("start proxy server");

    let config_a = r#"model_provider = "provider-a"
model = "model-a"

[model_providers.provider-a]
name = "ProviderA"
base_url = "https://provider-a.example/v1"
wire_api = "responses"
requires_openai_auth = true
"#;
    let config_b = r#"model_provider = "provider-b"
model = "model-b"

[model_providers.provider-b]
name = "ProviderB"
base_url = "https://provider-b.example/v1"
wire_api = "responses"
requires_openai_auth = true
"#;

    let provider_a = Provider::with_id(
        "a".to_string(),
        "ProviderA".to_string(),
        serde_json::json!({
            "auth": { "OPENAI_API_KEY": "key-a" },
            "config": config_a,
            "modelCatalog": { "models": [{ "model": "model-a" }] }
        }),
        None,
    );
    let mut provider_b = Provider::with_id(
        "b".to_string(),
        "ProviderB".to_string(),
        serde_json::json!({
            "auth": { "OPENAI_API_KEY": "key-b" },
            "config": config_b,
            "modelCatalog": { "models": [{ "model": "model-b" }] }
        }),
        None,
    );
    provider_b.meta = Some(ProviderMeta {
        common_config_enabled: Some(true),
        ..Default::default()
    });

    db.save_provider("codex", &provider_a)
        .expect("save provider a");
    db.save_provider("codex", &provider_b)
        .expect("save provider b");
    db.set_current_provider("codex", "a")
        .expect("set current provider a");
    crate::settings::set_current_provider(&AppType::Codex, Some("a"))
        .expect("set local current provider a");

    state
        .proxy_service
        .write_codex_live_for_provider(&provider_a.settings_config, Some(&provider_a))
        .expect("seed live codex config");
    assert!(
        !state
            .proxy_service
            .detect_takeover_in_live_config_for_app(&AppType::Codex),
        "seeded live config should not be proxy-taken-over"
    );

    db.save_live_backup(
        "codex",
        &serde_json::to_string(&provider_a.settings_config).expect("serialize backup"),
    )
    .await
    .expect("seed restored backup");

    crate::services::provider::ProviderService::switch(&state, AppType::Codex, "b")
        .expect("provider switch to provider b");
    state.proxy_service.stop().await.expect("stop proxy server");

    let catalog_path = crate::codex_config::get_codex_model_catalog_path();
    assert!(
        catalog_path.exists(),
        "agentswitch-model-catalog.json must be created on provider switch"
    );
    let catalog_text = std::fs::read_to_string(&catalog_path).expect("read catalog json");
    let catalog: serde_json::Value =
        serde_json::from_str(&catalog_text).expect("parse catalog json");
    let slugs: Vec<&str> = catalog
        .get("models")
        .and_then(|m| m.as_array())
        .map(|arr| {
            arr.iter()
                .filter_map(|e| e.get("slug").and_then(|s| s.as_str()))
                .collect()
        })
        .unwrap_or_default();
    assert!(
        slugs.contains(&"model-b"),
        "catalog must contain provider B's model after switch; got: {slugs:?}"
    );
    assert!(
        !slugs.contains(&"model-a"),
        "catalog must not contain stale provider A model after switch; got: {slugs:?}"
    );

    let config_path = crate::codex_config::get_codex_config_path();
    let config_text = std::fs::read_to_string(&config_path).expect("read config.toml");
    assert!(
        config_text.contains("model_catalog_json"),
        "config.toml must reference model_catalog_json after switch"
    );
    assert!(
        config_text.contains("[features]"),
        "config.toml must keep common config after switch"
    );
    assert!(
        config_text.contains("disable_response_storage = true"),
        "config.toml must include common config content after switch"
    );
}

#[tokio::test]
#[serial]
async fn provider_switch_with_restored_codex_backup_propagates_catalog_write_errors() {
    let _home = TempHome::new();
    crate::settings::reload_settings().expect("reload settings");
    seed_codex_model_template();

    let db = Arc::new(Database::memory().expect("init db"));
    let state = crate::store::AppState::new(db.clone());

    let proxy_config = ProxyConfig {
        listen_port: 0,
        ..Default::default()
    };
    db.update_proxy_config(proxy_config)
        .await
        .expect("set test proxy config");
    state
        .proxy_service
        .start()
        .await
        .expect("start proxy server");

    let config_a = r#"model_provider = "provider-a"
model = "model-a"

[model_providers.provider-a]
name = "ProviderA"
base_url = "https://provider-a.example/v1"
wire_api = "responses"
requires_openai_auth = true
"#;
    let config_b = r#"model_provider = "provider-b"
model = "model-b"

[model_providers.provider-b]
name = "ProviderB"
base_url = "https://provider-b.example/v1"
wire_api = "responses"
requires_openai_auth = true
"#;

    let provider_a = Provider::with_id(
        "a".to_string(),
        "ProviderA".to_string(),
        serde_json::json!({
            "auth": { "OPENAI_API_KEY": "key-a" },
            "config": config_a,
            "modelCatalog": { "models": [{ "model": "model-a" }] }
        }),
        None,
    );
    let provider_b = Provider::with_id(
        "b".to_string(),
        "ProviderB".to_string(),
        serde_json::json!({
            "auth": { "OPENAI_API_KEY": "key-b" },
            "config": config_b,
            "modelCatalog": { "models": [{ "model": "model-b" }] }
        }),
        None,
    );

    db.save_provider("codex", &provider_a)
        .expect("save provider a");
    db.save_provider("codex", &provider_b)
        .expect("save provider b");
    db.set_current_provider("codex", "a")
        .expect("set current provider a");
    crate::settings::set_current_provider(&AppType::Codex, Some("a"))
        .expect("set local current provider a");

    state
        .proxy_service
        .write_codex_live_for_provider(&provider_a.settings_config, Some(&provider_a))
        .expect("seed live codex config");
    assert!(
        !state
            .proxy_service
            .detect_takeover_in_live_config_for_app(&AppType::Codex),
        "seeded live config should not be proxy-taken-over"
    );

    db.save_live_backup(
        "codex",
        &serde_json::to_string(&provider_a.settings_config).expect("serialize backup"),
    )
    .await
    .expect("seed restored backup");

    let catalog_path = crate::codex_config::get_codex_model_catalog_path();
    if catalog_path.exists() {
        std::fs::remove_file(&catalog_path).expect("remove catalog file");
    }
    std::fs::create_dir_all(&catalog_path).expect("turn catalog path into directory");

    let err = crate::services::provider::ProviderService::switch(&state, AppType::Codex, "b")
        .expect_err("provider switch should fail when catalog cannot be written");
    state.proxy_service.stop().await.expect("stop proxy server");

    let message = err.to_string();
    assert!(
        message.contains("写入 Codex 配置失败") || message.contains("原子替换失败"),
        "switch should surface catalog write failure, got: {message}"
    );
}

/// Regression: turning proxy takeover off restores Live from the backup. The
/// backup snapshot is `read_codex_live_settings()` output (`{auth, config}`,
/// never an inline `modelCatalog`). The restore must NOT route the config
/// through catalog projection, which would see no specs and strip the
/// `model_catalog_json` pointer — silently dropping the user's Codex model
/// mapping from Live even though the DB SSOT still holds it.
#[tokio::test]
#[serial]
async fn codex_restore_from_backup_preserves_model_catalog_pointer() {
    let _home = TempHome::new();
    crate::settings::reload_settings().expect("reload settings");

    let db = Arc::new(Database::memory().expect("init db"));
    let service = ProxyService::new(db.clone());

    // Pre-takeover Live state: config.toml points at the Agent Switch-generated
    // catalog file, and that file exists on disk (takeover never touches it).
    let catalog_path = crate::codex_config::get_codex_model_catalog_path();
    if let Some(parent) = catalog_path.parent() {
        std::fs::create_dir_all(parent).expect("create codex dir");
    }
    std::fs::write(
        &catalog_path,
        r#"{"models":[{"slug":"deepseek-v4-flash"}]}"#,
    )
    .expect("seed generated catalog file");

    let pointer = catalog_path.to_string_lossy().replace('\\', "/");
    let backup_config = format!(
        "model_provider = \"custom\"\n\
         model = \"deepseek-v4-flash\"\n\
         model_catalog_json = \"{pointer}\"\n\n\
         [model_providers.custom]\n\
         name = \"DeepSeek\"\n\
         base_url = \"https://api.deepseek.example/v1\"\n\
         wire_api = \"responses\"\n"
    );
    let backup_json = serde_json::to_string(&json!({
        "auth": { "OPENAI_API_KEY": "deepseek-key" },
        "config": backup_config,
    }))
    .expect("serialize backup");
    db.save_live_backup("codex", &backup_json)
        .await
        .expect("seed live backup");

    // Turning takeover off restores Live from this backup.
    service
        .restore_live_config_for_app_with_fallback(&AppType::Codex)
        .await
        .expect("restore codex live from backup");

    let restored = std::fs::read_to_string(crate::codex_config::get_codex_config_path())
        .expect("read restored config.toml");
    assert!(
        restored.contains("model_catalog_json"),
        "restore must preserve the model_catalog_json pointer, got:\n{restored}"
    );
    assert!(
        restored.contains(pointer.as_str()),
        "restored pointer must still reference the Agent Switch-generated catalog file"
    );
}

/// Regression: a hot-switch during takeover rebuilds the backup from the DB
/// provider (`update_live_backup_from_provider`), so the backup carries an
/// inline `modelCatalog` (DB SSOT) but a `config.toml` text WITHOUT a
/// `model_catalog_json` pointer. Restoring that backup must project the
/// inline catalog — (re)generating both the catalog file and the pointer —
/// or the Codex model mapping vanishes from Live after takeover-off.
#[tokio::test]
#[serial]
async fn codex_restore_from_backup_projects_inline_model_catalog() {
    let _home = TempHome::new();
    crate::settings::reload_settings().expect("reload settings");

    let db = Arc::new(Database::memory().expect("init db"));
    let service = ProxyService::new(db.clone());

    // Catalog projection needs a model template; seed `models_cache.json`
    // with the template slug so we don't depend on the `codex` CLI.
    let codex_dir = crate::codex_config::get_codex_config_dir();
    std::fs::create_dir_all(&codex_dir).expect("create codex dir");
    std::fs::write(
        codex_dir.join("models_cache.json"),
        r#"{"models":[{"slug":"gpt-5.5"}]}"#,
    )
    .expect("seed models_cache template");

    // Provider-rebuilt backup shape: inline modelCatalog, pointer-less config.
    let backup_json = serde_json::to_string(&json!({
        "auth": { "OPENAI_API_KEY": "deepseek-key" },
        "config": "model_provider = \"custom\"\nmodel = \"deepseek-v4-flash\"\n\n[model_providers.custom]\nname = \"DeepSeek\"\nbase_url = \"https://api.deepseek.example/v1\"\nwire_api = \"responses\"\n",
        "modelCatalog": {
            "models": [
                { "model": "deepseek-v4-flash", "displayName": "DeepSeek V4 Flash", "contextWindow": 1_000_000 }
            ]
        }
    }))
    .expect("serialize backup");
    db.save_live_backup("codex", &backup_json)
        .await
        .expect("seed live backup");

    service
        .restore_live_config_for_app_with_fallback(&AppType::Codex)
        .await
        .expect("restore codex live from backup");

    let restored = std::fs::read_to_string(crate::codex_config::get_codex_config_path())
        .expect("read restored config.toml");
    let catalog_path = crate::codex_config::get_codex_model_catalog_path();
    assert!(
        restored.contains("model_catalog_json"),
        "restore must (re)generate the model_catalog_json pointer from inline catalog, got:\n{restored}"
    );
    assert!(
        catalog_path.exists(),
        "restore must generate the Agent Switch catalog file on disk"
    );
    let catalog: Value = serde_json::from_str(
        &std::fs::read_to_string(&catalog_path).expect("read generated catalog"),
    )
    .expect("parse generated catalog");
    let slugs: Vec<&str> = catalog
        .get("models")
        .and_then(|m| m.as_array())
        .expect("catalog models")
        .iter()
        .filter_map(|m| m.get("slug").and_then(|s| s.as_str()))
        .collect();
    assert!(
        slugs.contains(&"deepseek-v4-flash"),
        "generated catalog must contain the inline model, got slugs: {slugs:?}"
    );
}

/// Regression: a provider-rebuilt backup can pair an inline `modelCatalog`
/// with EMPTY `auth.json` (`{}`) — the bearer-token / Mobile-compat shape
/// where the API key lives in the config's `experimental_bearer_token`. The
/// empty-auth restore branch deletes `auth.json` and writes config raw; it
/// must still project the inline catalog (decision is orthogonal to auth), or
/// the model mapping vanishes on takeover-off for this provider shape.
#[tokio::test]
#[serial]
async fn codex_restore_empty_auth_backup_still_projects_inline_catalog() {
    let _home = TempHome::new();
    crate::settings::reload_settings().expect("reload settings");

    let db = Arc::new(Database::memory().expect("init db"));
    let service = ProxyService::new(db.clone());

    let codex_dir = crate::codex_config::get_codex_config_dir();
    std::fs::create_dir_all(&codex_dir).expect("create codex dir");
    std::fs::write(
        codex_dir.join("models_cache.json"),
        r#"{"models":[{"slug":"gpt-5.5"}]}"#,
    )
    .expect("seed models_cache template");

    // Empty auth.json + key carried in config.toml's experimental_bearer_token,
    // plus the inline modelCatalog (DB SSOT).
    let backup_json = serde_json::to_string(&json!({
        "auth": {},
        "config": "model_provider = \"custom\"\nmodel = \"deepseek-v4-flash\"\n\n[model_providers.custom]\nname = \"DeepSeek\"\nbase_url = \"https://api.deepseek.example/v1\"\nwire_api = \"responses\"\nexperimental_bearer_token = \"sk-deepseek\"\n",
        "modelCatalog": {
            "models": [ { "model": "deepseek-v4-flash", "displayName": "DeepSeek V4 Flash" } ]
        }
    }))
    .expect("serialize backup");
    db.save_live_backup("codex", &backup_json)
        .await
        .expect("seed live backup");

    service
        .restore_live_config_for_app_with_fallback(&AppType::Codex)
        .await
        .expect("restore codex live from backup");

    let restored = std::fs::read_to_string(crate::codex_config::get_codex_config_path())
        .expect("read restored config.toml");
    assert!(
        restored.contains("model_catalog_json"),
        "empty-auth restore must still project the inline catalog pointer, got:\n{restored}"
    );
    assert!(
        crate::codex_config::get_codex_model_catalog_path().exists(),
        "empty-auth restore must generate the Agent Switch catalog file"
    );
    assert!(
        !crate::codex_config::get_codex_auth_path().exists(),
        "empty-auth restore must delete auth.json rather than write an empty one"
    );
}

/// Regression: when the backup row itself contains the proxy placeholder
/// (a corrupted state where previous start/stop cycles saved the proxy
/// config as the "original Live"), restore must NOT write it back to Live.
/// It should fall through to the SSOT (current provider) path and rebuild
/// Live from the provider DB instead.
#[tokio::test]
#[serial]
async fn restore_falls_through_to_ssot_when_backup_is_proxy_placeholder() {
    let _home = TempHome::new();
    crate::settings::reload_settings().expect("reload settings");

    let db = Arc::new(Database::memory().expect("init db"));
    let service = ProxyService::new(db.clone());

    // Seed DB with a current provider that has a real API key
    let provider = Provider::with_id(
        "p1".to_string(),
        "P1".to_string(),
        json!({
            "env": {
                "ANTHROPIC_BASE_URL": "https://api.minimaxi.com/anthropic",
                "ANTHROPIC_API_KEY": "real-key-from-db"
            }
        }),
        None,
    );
    db.save_provider("claude", &provider)
        .expect("save provider");
    db.set_current_provider("claude", "p1")
        .expect("set current provider");

    // Seed backup with proxy placeholder (the corrupted state)
    let corrupted_backup = serde_json::to_string(&json!({
        "env": {
            "ANTHROPIC_AUTH_TOKEN": PROXY_TOKEN_PLACEHOLDER,
            "ANTHROPIC_BASE_URL": "http://127.0.0.1:15721"
        }
    }))
    .expect("serialize corrupted backup");
    db.save_live_backup("claude", &corrupted_backup)
        .await
        .expect("seed corrupted backup");

    // Seed Live with the same proxy placeholder (matches the corrupted state)
    service
        .write_claude_live(&json!({
            "env": {
                "ANTHROPIC_AUTH_TOKEN": PROXY_TOKEN_PLACEHOLDER,
                "ANTHROPIC_BASE_URL": "http://127.0.0.1:15721"
            }
        }))
        .expect("seed taken-over live file");

    // Restore: must NOT use the corrupted backup
    service
        .restore_live_config_for_app_with_fallback(&AppType::Claude)
        .await
        .expect("restore should succeed via SSOT");

    // The backup should still be the corrupted one (we didn't touch it on this path)
    let backup_after = db
        .get_live_backup("claude")
        .await
        .expect("get backup")
        .expect("backup still exists");
    assert_eq!(
        backup_after.original_config, corrupted_backup,
        "restore must NOT overwrite the corrupted backup"
    );

    // Live should now reflect the SSOT (provider DB), NOT the proxy URL
    let restored_live = service.read_claude_live().expect("read live");
    let restored_url = restored_live
        .get("env")
        .and_then(|env| env.get("ANTHROPIC_BASE_URL"))
        .and_then(|v| v.as_str());
    assert_eq!(
        restored_url,
        Some("https://api.minimaxi.com/anthropic"),
        "Live must be rebuilt from SSOT, not from the corrupted backup"
    );
    let restored_key = restored_live
        .get("env")
        .and_then(|env| env.get("ANTHROPIC_API_KEY"))
        .and_then(|v| v.as_str());
    assert_eq!(
        restored_key,
        Some("real-key-from-db"),
        "Live must carry the real API key from the provider DB"
    );
    assert_ne!(
        restored_live
            .get("env")
            .and_then(|env| env.get("ANTHROPIC_AUTH_TOKEN"))
            .and_then(|v| v.as_str()),
        Some(PROXY_TOKEN_PLACEHOLDER),
        "Live must not still carry the proxy placeholder"
    );
}

/// Regression: when Live is already a proxy placeholder (a corrupted state
/// where previous stop failed to restore), backup must NOT overwrite a
/// previously-good backup with the proxy config. This prevents the bug
/// where stop-then-start cycles permanently corrupt the backup.
#[tokio::test]
#[serial]
async fn backup_skips_when_live_is_already_proxy_placeholder() {
    let _home = TempHome::new();
    crate::settings::reload_settings().expect("reload settings");

    let db = Arc::new(Database::memory().expect("init db"));
    let service = ProxyService::new(db.clone());

    // Seed a GOOD backup (the "real" original Live)
    let good_backup = serde_json::to_string(&json!({
        "env": {
            "ANTHROPIC_BASE_URL": "https://api.minimaxi.com/anthropic",
            "ANTHROPIC_AUTH_TOKEN": "real-token"
        }
    }))
    .expect("serialize good backup");
    db.save_live_backup("claude", &good_backup)
        .await
        .expect("seed good backup");

    // Seed Live with proxy placeholder (the corrupted state)
    service
        .write_claude_live(&json!({
            "env": {
                "ANTHROPIC_AUTH_TOKEN": PROXY_TOKEN_PLACEHOLDER,
                "ANTHROPIC_BASE_URL": "http://127.0.0.1:15721"
            }
        }))
        .expect("seed taken-over live file");

    // Call backup_live_config_strict: must skip
    service
        .backup_live_config_strict(&AppType::Claude)
        .await
        .expect("backup should succeed (no-op when live is placeholder)");

    // The good backup must still be intact
    let backup_after = db
        .get_live_backup("claude")
        .await
        .expect("get backup")
        .expect("backup still exists");
    assert_eq!(
        backup_after.original_config, good_backup,
        "must not overwrite a good backup with a proxy placeholder"
    );
}

/// Regression: when ALL apps have Live=proxy-placeholder (worst-case
/// corrupted state), the bulk `backup_live_configs` path used by
/// `start_with_takeover` must skip every save — instead of overwriting
/// good backups with the proxy config.
#[tokio::test]
#[serial]
async fn bulk_backup_skips_all_when_live_is_proxy_placeholder() {
    let _home = TempHome::new();
    crate::settings::reload_settings().expect("reload settings");

    let db = Arc::new(Database::memory().expect("init db"));
    let service = ProxyService::new(db.clone());

    // Seed good backups for all three apps
    let good_backup = serde_json::to_string(&json!({
        "env": {
            "ANTHROPIC_AUTH_TOKEN": "real-token"
        }
    }))
    .expect("serialize good backup");
    db.save_live_backup("claude", &good_backup)
        .await
        .expect("seed claude backup");

    let codex_good_backup = serde_json::to_string(&json!({
        "auth": { "OPENAI_API_KEY": "real-codex-token" }
    }))
    .expect("serialize codex good backup");
    db.save_live_backup("codex", &codex_good_backup)
        .await
        .expect("seed codex backup");

    let gemini_good_backup = serde_json::to_string(&json!({
        "env": { "GEMINI_API_KEY": "real-gemini-key" }
    }))
    .expect("serialize gemini good backup");
    db.save_live_backup("gemini", &gemini_good_backup)
        .await
        .expect("seed gemini backup");

    // Seed all three Live files with proxy placeholders
    service
        .write_claude_live(&json!({
            "env": {
                "ANTHROPIC_AUTH_TOKEN": PROXY_TOKEN_PLACEHOLDER,
                "ANTHROPIC_BASE_URL": "http://127.0.0.1:15721"
            }
        }))
        .expect("seed claude live");
    let codex_dir = crate::codex_config::get_codex_config_dir();
    std::fs::create_dir_all(&codex_dir).expect("create codex dir");
    std::fs::write(
        crate::codex_config::get_codex_config_path(),
        r#"model_provider = "custom"

[model_providers.custom]
name = "Custom"
base_url = "http://127.0.0.1:15721/v1"
wire_api = "chat"
experimental_bearer_token = "PROXY_MANAGED"
"#,
    )
    .expect("seed codex config.toml");
    std::fs::write(
        crate::codex_config::get_codex_auth_path(),
        r#"{"OPENAI_API_KEY":"PROXY_MANAGED"}"#,
    )
    .expect("seed codex auth.json");
    let gemini_env_path = crate::gemini_config::get_gemini_env_path();
    if let Some(parent) = gemini_env_path.parent() {
        std::fs::create_dir_all(parent).expect("create gemini dir");
    }
    std::fs::write(&gemini_env_path, "GEMINI_API_KEY=PROXY_MANAGED\n").expect("seed gemini env");

    // Call bulk backup: must skip all three apps
    service
        .backup_live_configs()
        .await
        .expect("bulk backup should succeed (no-op when all live are placeholders)");

    // All three good backups must still be intact
    for (app_type, original) in [
        ("claude", good_backup.as_str()),
        ("codex", codex_good_backup.as_str()),
        ("gemini", gemini_good_backup.as_str()),
    ] {
        let backup_after = db
            .get_live_backup(app_type)
            .await
            .expect("get backup")
            .expect("backup still exists");
        assert_eq!(
            backup_after.original_config, original,
            "must not overwrite good backup for {app_type} with proxy placeholder"
        );
    }
}
