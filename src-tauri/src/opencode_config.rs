use crate::config::write_json_file;
use crate::error::AppError;
use crate::provider::OpenCodeProviderConfig;
use crate::settings::get_opencode_override_dir;
use indexmap::IndexMap;
use serde_json::{json, Map, Value};
use std::path::PathBuf;

const STANDARD_OMO_PLUGIN_PREFIXES: [&str; 2] = ["oh-my-openagent", "oh-my-opencode"];
const SLIM_OMO_PLUGIN_PREFIXES: [&str; 1] = ["oh-my-opencode-slim"];

fn matches_plugin_prefix(plugin_name: &str, prefix: &str) -> bool {
    plugin_name == prefix
        || plugin_name
            .strip_prefix(prefix)
            .map(|suffix| suffix.starts_with('@'))
            .unwrap_or(false)
}

fn matches_any_plugin_prefix(plugin_name: &str, prefixes: &[&str]) -> bool {
    prefixes
        .iter()
        .any(|prefix| matches_plugin_prefix(plugin_name, prefix))
}

fn canonicalize_plugin_name(plugin_name: &str) -> String {
    if let Some(suffix) = plugin_name.strip_prefix("oh-my-opencode") {
        if suffix.is_empty() || suffix.starts_with('@') {
            return format!("oh-my-openagent{suffix}");
        }
    }
    plugin_name.to_string()
}

fn plugin_package_name(value: &Value) -> Option<&str> {
    value
        .as_str()
        .or_else(|| value.get("package").and_then(Value::as_str))
}

pub fn get_opencode_dir() -> PathBuf {
    if let Some(override_dir) = get_opencode_override_dir() {
        return override_dir;
    }

    crate::config::get_home_dir()
        .join(".config")
        .join("opencode")
}

pub fn get_opencode_config_path() -> PathBuf {
    let dir = get_opencode_dir();
    let json = dir.join("opencode.json");
    let jsonc = dir.join("opencode.jsonc");
    if jsonc.exists() {
        jsonc
    } else {
        json
    }
}

pub(crate) fn opencode_executable() -> PathBuf {
    crate::commands::build_tool_search_paths("opencode")
        .into_iter()
        .flat_map(|directory| crate::commands::tool_executable_candidates("opencode", &directory))
        .find(|candidate| candidate.is_file())
        .unwrap_or_else(|| PathBuf::from("opencode"))
}

pub(crate) fn resume_command(session_id: &str) -> String {
    format!("opencode --session {session_id}")
}

/// 获取 OpenCode SQLite 数据库路径
/// 优先级: OPENCODE_DB 环境变量 > XDG_DATA_HOME > ~/.local/share/opencode
pub fn get_opencode_db_path() -> PathBuf {
    // 支持 OPENCODE_DB 环境变量覆盖（忽略空字符串）
    if let Ok(custom_path) = std::env::var("OPENCODE_DB") {
        if !custom_path.is_empty() {
            let path = PathBuf::from(&custom_path);
            if path.is_absolute() {
                return path;
            }
            // 相对路径基于数据目录
            return get_opencode_data_dir().join(path);
        }
    }

    get_opencode_data_dir().join("opencode.db")
}

fn get_opencode_data_dir() -> PathBuf {
    // 尊重 XDG_DATA_HOME（按 XDG 规范，空字符串视为未设置）
    if let Ok(xdg_data) = std::env::var("XDG_DATA_HOME") {
        if !xdg_data.is_empty() {
            return PathBuf::from(xdg_data).join("opencode");
        }
    }

    // OpenCode 使用 xdg-basedir，不遵守 macOS/Windows 平台约定，
    // 所有平台默认都落在 ~/.local/share/opencode
    crate::config::get_home_dir()
        .join(".local")
        .join("share")
        .join("opencode")
}

#[allow(dead_code)]
pub fn get_opencode_env_path() -> PathBuf {
    get_opencode_dir().join(".env")
}

pub fn read_opencode_config() -> Result<Value, AppError> {
    let path = get_opencode_config_path();

    if !path.exists() {
        return Ok(json!({
            "$schema": "https://opencode.ai/config.json"
        }));
    }

    let content = std::fs::read_to_string(&path).map_err(|e| AppError::io(&path, e))?;
    let config: Value = json5::from_str(&content).map_err(|e| {
        AppError::Config(format!(
            "Failed to parse OpenCode config: {}: {e}",
            path.display()
        ))
    })?;
    validate_v2_config(&config)?;
    Ok(config)
}

pub fn write_opencode_config(config: &Value) -> Result<(), AppError> {
    validate_v2_config(config)?;
    let path = get_opencode_config_path();
    write_json_file(&path, config)?;

    log::debug!("OpenCode config written to {path:?}");
    Ok(())
}

pub(crate) fn validate_v2_config(config: &Value) -> Result<(), AppError> {
    let Some(root) = config.as_object() else {
        return Err(AppError::Config(
            "OpenCode config root must be an object".into(),
        ));
    };
    if let Some(key) = ["provider", "plugin", "agent", "small_model"]
        .into_iter()
        .find(|key| root.contains_key(*key))
    {
        return Err(AppError::Config(format!(
            "OpenCode V1 field '{key}' is unsupported; migrate this configuration to V2"
        )));
    }
    if root
        .get("mcp")
        .and_then(Value::as_object)
        .is_some_and(|mcp| {
            mcp.iter()
                .any(|(key, value)| key != "servers" && value.is_object())
        })
    {
        return Err(AppError::Config(
            "OpenCode V1 mcp entries are unsupported; use mcp.servers in V2".into(),
        ));
    }
    if let Some(providers) = root.get("providers").and_then(Value::as_object) {
        for (id, provider) in providers {
            if let Some(fields) = provider.as_object() {
                if let Some(key) = ["npm", "options", "api"]
                    .into_iter()
                    .find(|key| fields.contains_key(*key))
                {
                    return Err(AppError::Config(format!(
                        "OpenCode V1 providers.{id}.{key} is unsupported; use native V2 provider fields"
                    )));
                }
                if let Some(models) = fields.get("models").and_then(Value::as_object) {
                    for (model_id, model) in models {
                        if let Some(model_fields) = model.as_object() {
                            if let Some(key) = ["options", "modalities", "tool_call"]
                                .into_iter()
                                .find(|key| model_fields.contains_key(*key))
                            {
                                return Err(AppError::Config(format!(
                                    "OpenCode V1 providers.{id}.models.{model_id}.{key} is unsupported"
                                )));
                            }
                            if model_fields.get("variants").is_some_and(Value::is_object) {
                                return Err(AppError::Config(format!(
                                    "OpenCode V1 providers.{id}.models.{model_id}.variants is unsupported; use a V2 array"
                                )));
                            }
                        }
                    }
                }
            }
        }
    }
    if let Some(agents) = root.get("agents").and_then(Value::as_object) {
        for (id, agent) in agents {
            if let Some(fields) = agent.as_object() {
                if let Some(key) = [
                    "prompt",
                    "permission",
                    "disable",
                    "maxSteps",
                    "temperature",
                    "top_p",
                    "tools",
                    "options",
                ]
                .into_iter()
                .find(|key| fields.contains_key(*key))
                {
                    return Err(AppError::Config(format!(
                        "OpenCode V1 agents.{id}.{key} is unsupported; use native V2 agent fields"
                    )));
                }
            }
        }
    }
    if let Some(servers) = root
        .get("mcp")
        .and_then(|mcp| mcp.get("servers"))
        .and_then(Value::as_object)
    {
        for (id, server) in servers {
            if server.get("enabled").is_some() {
                return Err(AppError::Config(format!(
                    "OpenCode V1 mcp.servers.{id}.enabled is unsupported; use disabled in V2"
                )));
            }
        }
    }
    Ok(())
}

fn small_model_from_config(config: &Value) -> Result<Option<String>, AppError> {
    if let Some(model) = config
        .get("agents")
        .and_then(|agents| agents.get("title"))
        .and_then(|title| title.get("model"))
    {
        if let Some(model) = model.as_str() {
            return Ok(Some(model.trim().to_string()).filter(|model| !model.is_empty()));
        }
        if let (Some(provider), Some(model_id)) = (
            model.get("providerID").and_then(Value::as_str),
            model.get("model").and_then(Value::as_str),
        ) {
            let variant = model
                .get("variant")
                .and_then(Value::as_str)
                .map(|variant| format!("#{variant}"))
                .unwrap_or_default();
            return Ok(Some(format!("{provider}/{model_id}{variant}")));
        }
    }
    Ok(None)
}

fn set_small_model_in_config(config: &mut Value, model: Option<&str>) -> Result<(), AppError> {
    let root = config.as_object_mut().ok_or_else(|| {
        AppError::Config("OpenCode config root must be a JSON object".to_string())
    })?;
    root.remove("small_model");
    if let Some(model) = model.map(str::trim).filter(|model| !model.is_empty()) {
        let agents = root.entry("agents").or_insert_with(|| json!({}));
        if !agents.is_object() {
            return Err(AppError::Config("OpenCode agents must be an object".into()));
        }
        let title = agents
            .as_object_mut()
            .unwrap()
            .entry("title")
            .or_insert_with(|| json!({}));
        if !title.is_object() {
            return Err(AppError::Config(
                "OpenCode title agent must be an object".into(),
            ));
        }
        title["model"] = json!(model);
    } else if let Some(title) = root
        .get_mut("agents")
        .and_then(|agents| agents.get_mut("title"))
        .and_then(Value::as_object_mut)
    {
        title.remove("model");
    }
    Ok(())
}

fn v2_package(npm: &str) -> String {
    if let Some(name) = npm.strip_prefix("@ai-sdk/") {
        return format!("aisdk:@ai-sdk/{name}");
    }
    npm.to_string()
}

fn v1_package(package: &str) -> String {
    if let Some(name) = package.strip_prefix("aisdk:") {
        return name.to_string();
    }
    package.to_string()
}

fn built_in_package(id: &str) -> Option<&'static str> {
    match id {
        "openai" | "azure" => Some("@ai-sdk/openai"),
        "anthropic" => Some("@ai-sdk/anthropic"),
        "google" | "google-vertex" => Some("@ai-sdk/google"),
        "amazon-bedrock" => Some("@ai-sdk/amazon-bedrock"),
        _ => None,
    }
}

fn provider_to_v2(value: Value) -> Value {
    let Some(mut provider) = value.as_object().cloned() else {
        return value;
    };
    if provider.contains_key("package") {
        return Value::Object(provider);
    }
    if let Some(npm) = provider
        .remove("npm")
        .and_then(|value| value.as_str().map(str::to_owned))
    {
        provider.insert("package".into(), json!(v2_package(&npm)));
    }
    if let Some(api) = provider.remove("api") {
        let settings = provider.entry("settings").or_insert_with(|| json!({}));
        if settings.is_object() {
            settings["baseURL"] = api;
        }
    }
    if let Some(mut options) = provider
        .remove("options")
        .and_then(|value| value.as_object().cloned())
    {
        if let Some(headers) = options.remove("headers") {
            provider.insert("headers".into(), headers);
        }
        let settings = provider.entry("settings").or_insert_with(|| json!({}));
        if let Some(settings) = settings.as_object_mut() {
            settings.extend(options);
        }
    }
    if let Some(models) = provider.get_mut("models").and_then(Value::as_object_mut) {
        for model in models.values_mut() {
            let Some(fields) = model.as_object_mut() else {
                continue;
            };
            if let Some(settings) = fields.remove("options") {
                fields.insert("settings".into(), settings);
            }
            if let Some(id) = fields.remove("id") {
                fields.insert("modelID".into(), id);
            }
            if fields.get("status").and_then(Value::as_str) == Some("deprecated") {
                fields.remove("status");
                fields.insert("disabled".into(), Value::Bool(true));
            }
            if let Some(cost) = fields.get_mut("cost").and_then(Value::as_object_mut) {
                let mut cache = cost.remove("cache").unwrap_or_else(|| json!({}));
                for (old, new) in [("cache_read", "read"), ("cache_write", "write")] {
                    if let Some(value) = cost.remove(old) {
                        cache[new] = value;
                    }
                }
                if cache.as_object().is_some_and(|cache| !cache.is_empty()) {
                    cost.insert("cache".into(), cache);
                }
            }
            if let Some(modalities) = fields.remove("modalities") {
                let mut capabilities = fields.remove("capabilities").unwrap_or_else(|| json!({}));
                if let Some(input) = modalities.get("input") {
                    capabilities["input"] = input.clone();
                }
                if let Some(output) = modalities.get("output") {
                    capabilities["output"] = output.clone();
                }
                fields.insert("capabilities".into(), capabilities);
            }
            if let Some(tools) = fields.remove("tool_call") {
                let capabilities = fields.entry("capabilities").or_insert_with(|| json!({}));
                capabilities["tools"] = tools;
            }
            if let Some(variants) = fields
                .remove("variants")
                .and_then(|value| value.as_object().cloned())
            {
                fields.insert(
                    "variants".into(),
                    Value::Array(
                        variants
                            .into_iter()
                            .map(|(id, settings)| json!({"id": id, "settings": settings}))
                            .collect(),
                    ),
                );
            }
        }
    }
    Value::Object(provider)
}

fn provider_from_v2(value: Value) -> Value {
    let Some(mut provider) = value.as_object().cloned() else {
        return value;
    };
    if let Some(package) = provider
        .remove("package")
        .and_then(|value| value.as_str().map(str::to_owned))
    {
        provider.insert("npm".into(), json!(v1_package(&package)));
    }
    // The editor already uses options.baseURL, so keep the native endpoint there.
    let mut options = provider
        .remove("settings")
        .and_then(|value| value.as_object().cloned())
        .unwrap_or_default();
    if let Some(headers) = provider.remove("headers") {
        options.insert("headers".into(), headers);
    }
    provider.insert("options".into(), Value::Object(options));
    if let Some(models) = provider.get_mut("models").and_then(Value::as_object_mut) {
        for (model_id, model) in models {
            let Some(fields) = model.as_object_mut() else {
                continue;
            };
            fields.entry("name").or_insert_with(|| json!(model_id));
            if let Some(settings) = fields.remove("settings") {
                fields.insert("options".into(), settings);
            }
            if let Some(id) = fields.remove("modelID") {
                fields.insert("id".into(), id);
            }
            if fields.get("disabled") == Some(&Value::Bool(true)) {
                fields.remove("disabled");
                fields.insert("status".into(), json!("deprecated"));
            }
            if let Some(cost) = fields.get_mut("cost").and_then(Value::as_object_mut) {
                if let Some(cache) = cost.get("cache") {
                    let cache = cache.clone();
                    for (new, old) in [("read", "cache_read"), ("write", "cache_write")] {
                        if let Some(value) = cache.get(new) {
                            cost.insert(old.into(), value.clone());
                        }
                    }
                }
            }
            if let Some(capabilities) = fields.remove("capabilities") {
                let mut modalities = json!({});
                if let Some(input) = capabilities.get("input") {
                    modalities["input"] = input.clone();
                }
                if let Some(output) = capabilities.get("output") {
                    modalities["output"] = output.clone();
                }
                if modalities
                    .as_object()
                    .is_some_and(|fields| !fields.is_empty())
                {
                    fields.insert("modalities".into(), modalities);
                }
                if let Some(tools) = capabilities.get("tools") {
                    fields.insert("tool_call".into(), tools.clone());
                }
            }
            if let Some(variants) = fields
                .remove("variants")
                .and_then(|value| value.as_array().cloned())
            {
                let mut legacy = Map::new();
                for mut variant in variants {
                    if let Some(id) = variant.get("id").and_then(Value::as_str).map(str::to_owned) {
                        let settings = variant
                            .as_object_mut()
                            .and_then(|fields| fields.remove("settings"))
                            .unwrap_or_else(|| json!({}));
                        legacy.insert(id, settings);
                    }
                }
                fields.insert("variants".into(), Value::Object(legacy));
            }
        }
    }
    Value::Object(provider)
}

fn mcp_to_v2(mut config: Value) -> Value {
    if let Some(enabled) = config
        .as_object_mut()
        .and_then(|fields| fields.remove("enabled"))
        .and_then(|value| value.as_bool())
    {
        config["disabled"] = json!(!enabled);
    }
    if let Some(timeout) = config
        .get("timeout")
        .filter(|value| value.is_number())
        .cloned()
    {
        config["timeout"] = json!({"catalog": timeout, "execution": timeout});
    }
    if let Some(oauth) = config.get_mut("oauth").and_then(Value::as_object_mut) {
        for (old, new) in [
            ("clientId", "client_id"),
            ("clientSecret", "client_secret"),
            ("callbackPort", "callback_port"),
            ("redirectUri", "redirect_uri"),
        ] {
            if let Some(value) = oauth.remove(old) {
                oauth.entry(new).or_insert(value);
            }
        }
    }
    config
}

fn preserve_variant_fields(existing: &Value, next: &mut Value) {
    let Some(models) = next.get_mut("models").and_then(Value::as_object_mut) else {
        return;
    };
    for (model_id, model) in models {
        let Some(variants) = model.get_mut("variants").and_then(Value::as_array_mut) else {
            continue;
        };
        let previous = existing
            .get("models")
            .and_then(|models| models.get(model_id))
            .and_then(|model| model.get("variants"))
            .and_then(Value::as_array);
        for variant in variants {
            let Some(id) = variant.get("id").and_then(Value::as_str) else {
                continue;
            };
            let old = previous.and_then(|variants| {
                variants
                    .iter()
                    .find(|old| old.get("id").and_then(Value::as_str) == Some(id))
            });
            if let (Some(old), Some(next)) =
                (old.and_then(Value::as_object), variant.as_object_mut())
            {
                for (key, value) in old {
                    if key != "settings" && key != "id" {
                        next.entry(key.clone()).or_insert_with(|| value.clone());
                    }
                }
            }
        }
    }
}

pub fn get_small_model() -> Result<Option<String>, AppError> {
    small_model_from_config(&read_opencode_config()?)
}

pub fn set_small_model(model: Option<&str>) -> Result<(), AppError> {
    let mut config = read_opencode_config()?;
    set_small_model_in_config(&mut config, model)?;
    write_opencode_config(&config)
}

pub fn get_web_search_enabled() -> Result<bool, AppError> {
    Ok(web_search_enabled_v2(&read_opencode_config()?))
}

pub fn set_web_search_enabled(enabled: bool) -> Result<(), AppError> {
    set_v2_web_search_enabled(enabled)
}

fn web_search_enabled_v2(config: &Value) -> bool {
    config.get("websearch") != Some(&Value::Bool(false))
        && config.get("tools").and_then(|tools| tools.get("websearch")) != Some(&Value::Bool(false))
}

fn set_v2_web_search_enabled(enabled: bool) -> Result<(), AppError> {
    let mut config = read_opencode_config()?;
    let root = config
        .as_object_mut()
        .ok_or_else(|| AppError::Config("OpenCode config root must be an object".into()))?;
    if enabled {
        if root.get("websearch") == Some(&Value::Bool(false)) {
            root.remove("websearch");
        }
        if let Some(tools) = root.get_mut("tools").and_then(Value::as_object_mut) {
            if tools.get("websearch") == Some(&Value::Bool(false)) {
                tools.remove("websearch");
            }
        }
    } else {
        root.insert("websearch".into(), Value::Bool(false));
    }
    write_opencode_config(&config)
}

fn providers_from_config(config: &Value) -> Map<String, Value> {
    let mut providers = Map::new();
    if let Some(native) = config.get("providers").and_then(Value::as_object) {
        for (id, value) in native {
            let mut normalized = provider_from_v2(value.clone());
            if normalized.get("npm").is_none() {
                if let Some(package) = built_in_package(id) {
                    normalized["npm"] = json!(package);
                }
            }
            providers.insert(id.clone(), normalized);
        }
    }
    providers
}

pub fn get_providers() -> Result<Map<String, Value>, AppError> {
    Ok(providers_from_config(&read_opencode_config()?))
}

pub fn set_provider(id: &str, config: Value) -> Result<(), AppError> {
    let mut full_config = read_opencode_config()?;
    let existing_native = full_config
        .get("providers")
        .and_then(|providers| providers.get(id))
        .cloned();
    let key = "providers";
    let root = full_config
        .as_object_mut()
        .ok_or_else(|| AppError::Config("OpenCode config root must be an object".into()))?;
    let inferred_package = root
        .get("providers")
        .and_then(|providers| providers.get(id))
        .is_some_and(|existing| existing.get("package").is_none())
        && config.get("npm").and_then(Value::as_str) == built_in_package(id);
    let mut value = provider_to_v2(config);
    if let Some(existing) = &existing_native {
        preserve_variant_fields(existing, &mut value);
    }
    if inferred_package {
        value.as_object_mut().map(|fields| fields.remove("package"));
    }
    let providers = root.entry(key).or_insert_with(|| json!({}));
    let providers = providers
        .as_object_mut()
        .ok_or_else(|| AppError::Config(format!("OpenCode {key} must be an object")))?;
    providers.insert(id.to_string(), value);

    write_opencode_config(&full_config)
}

pub fn remove_provider(id: &str) -> Result<(), AppError> {
    let mut config = read_opencode_config()?;

    if let Some(providers) = config.get_mut("providers").and_then(Value::as_object_mut) {
        providers.remove(id);
    }

    write_opencode_config(&config)
}

pub fn get_typed_providers() -> Result<IndexMap<String, OpenCodeProviderConfig>, AppError> {
    let providers = get_providers()?;
    let mut result = IndexMap::new();

    for (id, value) in providers {
        match serde_json::from_value::<OpenCodeProviderConfig>(value.clone()) {
            Ok(config) => {
                result.insert(id, config);
            }
            Err(e) => {
                log::warn!("Failed to parse provider '{id}': {e}");
            }
        }
    }

    Ok(result)
}

pub fn set_typed_provider(id: &str, config: &OpenCodeProviderConfig) -> Result<(), AppError> {
    let value = serde_json::to_value(config).map_err(|e| AppError::JsonSerialize { source: e })?;
    set_provider(id, value)
}

fn mcp_servers_from_config(config: &Value) -> Map<String, Value> {
    config
        .get("mcp")
        .and_then(|mcp| mcp.get("servers"))
        .and_then(Value::as_object)
        .cloned()
        .unwrap_or_default()
}

pub fn get_mcp_servers() -> Result<Map<String, Value>, AppError> {
    Ok(mcp_servers_from_config(&read_opencode_config()?))
}

pub fn set_mcp_server(id: &str, config: Value) -> Result<(), AppError> {
    let mut full_config = read_opencode_config()?;

    let root = full_config
        .as_object_mut()
        .ok_or_else(|| AppError::Config("OpenCode config root must be an object".into()))?;
    let mcp = root.entry("mcp").or_insert_with(|| json!({}));
    let mcp = mcp
        .as_object_mut()
        .ok_or_else(|| AppError::Config("OpenCode mcp must be an object".into()))?;
    let servers = mcp.entry("servers").or_insert_with(|| json!({}));
    let servers = servers
        .as_object_mut()
        .ok_or_else(|| AppError::Config("OpenCode mcp.servers must be an object".into()))?;
    servers.insert(id.to_string(), mcp_to_v2(config));

    write_opencode_config(&full_config)
}

pub fn remove_mcp_server(id: &str) -> Result<(), AppError> {
    let mut config = read_opencode_config()?;

    if let Some(mcp) = config.get_mut("mcp").and_then(Value::as_object_mut) {
        if let Some(servers) = mcp.get_mut("servers").and_then(Value::as_object_mut) {
            servers.remove(id);
        }
    }

    write_opencode_config(&config)
}

pub fn add_plugin(plugin_name: &str) -> Result<(), AppError> {
    let mut config = read_opencode_config()?;
    let normalized_plugin_name = canonicalize_plugin_name(plugin_name);

    let key = "plugins";
    let plugins = config.get_mut(key).and_then(|v| v.as_array_mut());

    match plugins {
        Some(arr) => {
            // Mutual exclusion: standard OMO and OMO Slim cannot coexist as plugins
            if matches_any_plugin_prefix(&normalized_plugin_name, &STANDARD_OMO_PLUGIN_PREFIXES) {
                arr.retain(|v| {
                    plugin_package_name(v)
                        .map(|s| {
                            !matches_any_plugin_prefix(s, &STANDARD_OMO_PLUGIN_PREFIXES)
                                && !matches_any_plugin_prefix(s, &SLIM_OMO_PLUGIN_PREFIXES)
                        })
                        .unwrap_or(true)
                });
            } else if matches_any_plugin_prefix(&normalized_plugin_name, &SLIM_OMO_PLUGIN_PREFIXES)
            {
                arr.retain(|v| {
                    plugin_package_name(v)
                        .map(|s| {
                            !matches_any_plugin_prefix(s, &STANDARD_OMO_PLUGIN_PREFIXES)
                                && !matches_any_plugin_prefix(s, &SLIM_OMO_PLUGIN_PREFIXES)
                        })
                        .unwrap_or(true)
                });
            }

            let already_exists = arr
                .iter()
                .any(|v| plugin_package_name(v) == Some(normalized_plugin_name.as_str()));
            if !already_exists {
                arr.push(Value::String(normalized_plugin_name));
            }
        }
        None => {
            config[key] = json!([normalized_plugin_name]);
        }
    }

    write_opencode_config(&config)
}

pub fn remove_plugins_by_prefixes(prefixes: &[&str]) -> Result<(), AppError> {
    let mut config = read_opencode_config()?;

    for key in ["plugins"] {
        if let Some(arr) = config.get_mut(key).and_then(|v| v.as_array_mut()) {
            arr.retain(|v| {
                plugin_package_name(v)
                    .map(|s| !matches_any_plugin_prefix(s, prefixes))
                    .unwrap_or(true)
            });

            if arr.is_empty() {
                config.as_object_mut().map(|obj| obj.remove(key));
            }
        }
    }

    write_opencode_config(&config)
}

#[cfg(test)]
mod tests {
    use super::{
        mcp_servers_from_config, mcp_to_v2, plugin_package_name, preserve_variant_fields,
        provider_from_v2, provider_to_v2, providers_from_config, set_small_model_in_config,
        small_model_from_config, validate_v2_config, web_search_enabled_v2,
    };
    use serde_json::json;

    #[test]
    fn translates_provider_models_to_native_v2_and_back() {
        let legacy = json!({
            "npm": "@ai-sdk/openai-compatible",
            "name": "Example",
            "options": {"baseURL": "https://example.com/v1", "apiKey": "{env:API_KEY}", "headers": {"X-Test": "yes"}},
            "models": {"coding": {"name": "Coding", "options": {"reasoningEffort": "high"},
                "modalities": {"input": ["text", "image"], "output": ["text"]},
                "variants": {"fast": {"temperature": 0.2}}}}
        });
        let native = provider_to_v2(legacy.clone());
        assert_eq!(native["package"], "aisdk:@ai-sdk/openai-compatible");
        assert_eq!(native["settings"]["baseURL"], "https://example.com/v1");
        assert_eq!(native["headers"]["X-Test"], "yes");
        assert_eq!(
            native["models"]["coding"]["capabilities"]["input"][1],
            "image"
        );
        assert_eq!(native["models"]["coding"]["variants"][0]["id"], "fast");
        assert_eq!(provider_from_v2(native), legacy);
    }

    #[test]
    fn preserves_native_runtime_package_on_round_trip() {
        let native = json!({"package": "@opencode/ai/providers/openai/chat", "settings": {"baseURL": "https://example.com"}});
        assert_eq!(provider_to_v2(provider_from_v2(native.clone())), native);
    }

    #[test]
    fn preserves_native_variant_headers_when_editing_settings() {
        let existing = json!({"models": {"coding": {"variants": [
            {"id": "fast", "settings": {"temperature": 0.1}, "headers": {"X-Tier": "fast"}}
        ]}}});
        let mut next = json!({"models": {"coding": {"variants": [
            {"id": "fast", "settings": {"temperature": 0.2}}
        ]}}});
        preserve_variant_fields(&existing, &mut next);
        assert_eq!(
            next["models"]["coding"]["variants"][0]["settings"]["temperature"],
            0.2
        );
        assert_eq!(
            next["models"]["coding"]["variants"][0]["headers"]["X-Tier"],
            "fast"
        );
    }

    #[test]
    fn rejects_v1_config_fields() {
        assert!(validate_v2_config(&json!({"providers": {}})).is_ok());
        assert!(validate_v2_config(&json!({"provider": {}})).is_err());
        assert!(validate_v2_config(&json!({"mcp": {"old": {"type": "local"}}})).is_err());
        assert!(validate_v2_config(&json!({"providers": {"p": {"npm": "old"}}})).is_err());
        assert!(validate_v2_config(
            &json!({"providers": {"p": {"models": {"m": {"variants": {"high": {}}}}}}})
        )
        .is_err());
        assert!(validate_v2_config(
            &json!({"agents": {"reviewer": {"permission": {"edit": "deny"}}}})
        )
        .is_err());
        assert!(
            validate_v2_config(&json!({"mcp": {"servers": {"remote": {"enabled": true}}}}))
                .is_err()
        );
        assert!(validate_v2_config(&json!({"agents": {"reviewer": {"permissions": []}}, "providers": {"p": {"settings": {}}}, "mcp": {"servers": {"remote": {"disabled": false}}}})).is_ok());
    }

    #[test]
    fn reads_native_builtin_provider_without_explicit_package() {
        let providers = providers_from_config(&json!({
            "provider": {"openai": {"npm": "@ai-sdk/openai-compatible"}},
            "providers": {"openai": {"settings": {"baseURL": "https://proxy.example/v1"}, "models": {"coding": {}}}}
        }));
        assert_eq!(providers["openai"]["npm"], "@ai-sdk/openai");
        assert_eq!(
            providers["openai"]["options"]["baseURL"],
            "https://proxy.example/v1"
        );
        assert_eq!(providers["openai"]["models"]["coding"]["name"], "coding");
    }

    #[test]
    fn v2_mcp_uses_disabled_instead_of_enabled() {
        assert_eq!(
            mcp_to_v2(json!({"type":"local", "command":["npx"], "enabled":true, "timeout":30000})),
            json!({"type":"local", "command":["npx"], "disabled":false, "timeout":{"catalog":30000,"execution":30000}})
        );
    }

    #[test]
    fn v2_mcp_renames_oauth_fields() {
        let converted = mcp_to_v2(
            json!({"type":"remote", "url":"https://example.com/mcp", "oauth":{"clientId":"abc","callbackPort":1234}}),
        );
        assert_eq!(converted["oauth"]["client_id"], "abc");
        assert_eq!(converted["oauth"]["callback_port"], 1234);
        assert!(converted["oauth"].get("clientId").is_none());
    }

    #[test]
    fn v2_provider_translates_legacy_endpoint_and_model_fields() {
        let converted = provider_to_v2(json!({
            "npm":"@ai-sdk/openai-compatible", "api":"https://example.com/v1",
            "options":{"apiKey":"secret"},
            "models":{"old":{"id":"actual", "status":"deprecated", "cost":{"cache_read":1.0,"cache_write":2.0}}}
        }));
        assert_eq!(converted["settings"]["baseURL"], "https://example.com/v1");
        assert_eq!(converted["settings"]["apiKey"], "secret");
        assert_eq!(converted["models"]["old"]["modelID"], "actual");
        assert_eq!(converted["models"]["old"]["disabled"], true);
        assert_eq!(converted["models"]["old"]["cost"]["cache"]["read"], 1.0);
    }

    #[test]
    fn v2_mcp_lists_servers_without_global_settings() {
        let servers = mcp_servers_from_config(&json!({"mcp": {
            "timeout": {"catalog": 30_000},
            "old": {"type": "local", "command": ["old"]},
            "servers": {"new": {"type": "remote", "url": "https://example.com/mcp"}}
        }}));
        assert_eq!(servers.len(), 1);
        assert!(!servers.contains_key("old"));
        assert!(servers.contains_key("new"));
    }

    #[test]
    fn v2_web_search_respects_native_and_legacy_disables() {
        assert!(web_search_enabled_v2(&json!({})));
        assert!(!web_search_enabled_v2(&json!({"websearch": false})));
        assert!(!web_search_enabled_v2(
            &json!({"tools": {"websearch": false}})
        ));
    }

    #[test]
    fn recognizes_native_plugin_object_entries() {
        assert_eq!(
            plugin_package_name(&json!({"package": "oh-my-opencode-slim@2.2.17", "options": {}})),
            Some("oh-my-opencode-slim@2.2.17")
        );
    }

    #[test]
    fn title_model_uses_native_agent_field() {
        let mut config = json!({"agents": {"title": {"model": "old/model", "hidden": true}}});
        assert_eq!(
            small_model_from_config(&config).unwrap().as_deref(),
            Some("old/model")
        );
        set_small_model_in_config(&mut config, Some("new/model")).unwrap();
        assert_eq!(config["agents"]["title"]["model"], "new/model");
        assert_eq!(config["agents"]["title"]["hidden"], true);
        let expanded = json!({"agents": {"title": {"model": {"providerID": "openai", "model": "coding", "variant": "high"}}}});
        assert_eq!(
            small_model_from_config(&expanded).unwrap().as_deref(),
            Some("openai/coding#high")
        );
    }

    #[test]
    fn updates_small_model_without_touching_other_config() {
        let mut config = json!({
            "$schema": "https://opencode.ai/config.json",
            "providers": { "custom": { "name": "Custom" } },
            "plugins": ["oh-my-opencode-slim@latest"],
            "agents": { "build": { "mode": "primary" } }
        });
        let expected_other_fields = config.clone();

        set_small_model_in_config(&mut config, Some(" openai/gpt-5.6-mini ")).unwrap();

        assert_eq!(config["agents"]["title"]["model"], "openai/gpt-5.6-mini");
        for key in ["$schema", "providers", "plugins"] {
            assert_eq!(config[key], expected_other_fields[key]);
        }
    }

    #[test]
    fn empty_small_model_removes_the_field() {
        let mut config = json!({
            "agents": {"title": {"model": "opencode/big-pickle", "hidden": true}},
            "providers": { "custom": {} }
        });

        set_small_model_in_config(&mut config, Some("   ")).unwrap();

        assert!(config["agents"]["title"].get("model").is_none());
        assert_eq!(config["agents"]["title"]["hidden"], true);
        assert!(config.get("providers").is_some());
    }
}
