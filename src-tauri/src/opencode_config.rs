use crate::config::{write_json_file, write_text_file};
use crate::error::AppError;
use crate::provider::OpenCodeProviderConfig;
use crate::settings::get_opencode_override_dir;
use indexmap::IndexMap;
use serde_json::{json, Map, Value};
use std::path::{Path, PathBuf};

#[cfg(target_os = "windows")]
use winreg::enums::{HKEY_CURRENT_USER, KEY_READ, KEY_WRITE};
#[cfg(target_os = "windows")]
use winreg::RegKey;

const STANDARD_OMO_PLUGIN_PREFIXES: [&str; 2] = ["oh-my-openagent", "oh-my-opencode"];
const SLIM_OMO_PLUGIN_PREFIXES: [&str; 1] = ["oh-my-opencode-slim"];
const OPENCODE_WEB_SEARCH_ENV_VAR: &str = "OPENCODE_ENABLE_EXA";

#[cfg(not(target_os = "windows"))]
#[derive(Clone, Copy)]
enum ShellConfigSyntax {
    Posix,
    Fish,
}

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

pub fn get_opencode_dir() -> PathBuf {
    if let Some(override_dir) = get_opencode_override_dir() {
        return override_dir;
    }

    crate::config::get_home_dir()
        .join(".config")
        .join("opencode")
}

pub fn get_opencode_config_path() -> PathBuf {
    get_opencode_dir().join("opencode.json")
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
    json5::from_str(&content).map_err(|e| {
        AppError::Config(format!(
            "Failed to parse OpenCode config: {}: {e}",
            path.display()
        ))
    })
}

pub fn write_opencode_config(config: &Value) -> Result<(), AppError> {
    let path = get_opencode_config_path();
    write_json_file(&path, config)?;

    log::debug!("OpenCode config written to {path:?}");
    Ok(())
}

fn small_model_from_config(config: &Value) -> Result<Option<String>, AppError> {
    match config.get("small_model") {
        None | Some(Value::Null) => Ok(None),
        Some(Value::String(model)) => {
            let model = model.trim();
            Ok((!model.is_empty()).then(|| model.to_string()))
        }
        Some(_) => Err(AppError::Config(
            "OpenCode small_model must be a string".to_string(),
        )),
    }
}

fn set_small_model_in_config(config: &mut Value, model: Option<&str>) -> Result<(), AppError> {
    let root = config.as_object_mut().ok_or_else(|| {
        AppError::Config("OpenCode config root must be a JSON object".to_string())
    })?;

    match model.map(str::trim).filter(|value| !value.is_empty()) {
        Some(model) => {
            root.insert("small_model".to_string(), Value::String(model.to_string()));
        }
        None => {
            root.remove("small_model");
        }
    }

    Ok(())
}

pub fn get_small_model() -> Result<Option<String>, AppError> {
    small_model_from_config(&read_opencode_config()?)
}

pub fn set_small_model(model: Option<&str>) -> Result<(), AppError> {
    let mut config = read_opencode_config()?;
    set_small_model_in_config(&mut config, model)?;
    write_opencode_config(&config)
}

fn is_truthy_env_value(value: &str) -> bool {
    matches!(
        value.trim().to_ascii_lowercase().as_str(),
        "1" | "true" | "yes" | "on"
    )
}

#[cfg(not(target_os = "windows"))]
fn shell_assignment_value(line: &str) -> Option<String> {
    let trimmed = line.trim();
    if trimmed.is_empty() || trimmed.starts_with('#') {
        return None;
    }

    let posix = trimmed.strip_prefix("export ").unwrap_or(trimmed);
    if let Some((name, value)) = posix.split_once('=') {
        if name.trim() == OPENCODE_WEB_SEARCH_ENV_VAR {
            return Some(
                value
                    .split('#')
                    .next()
                    .unwrap_or_default()
                    .trim()
                    .trim_end_matches(';')
                    .trim_matches(['"', '\''])
                    .to_string(),
            );
        }
    }

    let mut words = trimmed.split_whitespace();
    if words.next() != Some("set") {
        return None;
    }

    while let Some(word) = words.next() {
        if word == OPENCODE_WEB_SEARCH_ENV_VAR {
            return Some(
                words
                    .next()?
                    .trim_end_matches(';')
                    .trim_matches(['"', '\''])
                    .to_string(),
            );
        }
    }

    None
}

#[cfg(not(target_os = "windows"))]
fn rewrite_shell_config(content: &str, syntax: ShellConfigSyntax, enabled: bool) -> String {
    let had_trailing_newline = content.ends_with('\n');
    let mut rewritten = content
        .lines()
        .filter(|line| shell_assignment_value(line).is_none())
        .collect::<Vec<_>>()
        .join("\n");

    if enabled {
        if !rewritten.is_empty() {
            rewritten.push('\n');
        }
        rewritten.push_str(match syntax {
            ShellConfigSyntax::Posix => "export OPENCODE_ENABLE_EXA=true",
            ShellConfigSyntax::Fish => "set -gx OPENCODE_ENABLE_EXA true",
        });
        rewritten.push('\n');
    } else if had_trailing_newline && !rewritten.is_empty() {
        rewritten.push('\n');
    }

    rewritten
}

#[cfg(not(target_os = "windows"))]
fn shell_config_paths(home: &Path) -> Vec<(PathBuf, ShellConfigSyntax)> {
    vec![
        (home.join(".zshrc"), ShellConfigSyntax::Posix),
        (home.join(".zprofile"), ShellConfigSyntax::Posix),
        (home.join(".bashrc"), ShellConfigSyntax::Posix),
        (home.join(".bash_profile"), ShellConfigSyntax::Posix),
        (home.join(".profile"), ShellConfigSyntax::Posix),
        (
            home.join(".config").join("fish").join("config.fish"),
            ShellConfigSyntax::Fish,
        ),
    ]
}

#[cfg(not(target_os = "windows"))]
fn write_shell_config(path: &Path, content: &str) -> Result<(), AppError> {
    let destination = if path.is_symlink() {
        std::fs::canonicalize(path).map_err(|error| AppError::io(path, error))?
    } else {
        path.to_path_buf()
    };
    write_text_file(&destination, content)
}

#[cfg(not(target_os = "windows"))]
fn active_shell_config(home: &Path, shell: Option<&str>) -> (PathBuf, ShellConfigSyntax) {
    match shell
        .and_then(|value| Path::new(value).file_name())
        .and_then(|value| value.to_str())
    {
        Some("zsh") => (home.join(".zshrc"), ShellConfigSyntax::Posix),
        Some("fish") => (
            home.join(".config").join("fish").join("config.fish"),
            ShellConfigSyntax::Fish,
        ),
        Some("bash") => (home.join(".bashrc"), ShellConfigSyntax::Posix),
        Some("sh" | "dash") => (home.join(".profile"), ShellConfigSyntax::Posix),
        Some(_) => (home.join(".profile"), ShellConfigSyntax::Posix),
        #[cfg(target_os = "macos")]
        None => (home.join(".zshrc"), ShellConfigSyntax::Posix),
        #[cfg(not(target_os = "macos"))]
        None => (home.join(".bashrc"), ShellConfigSyntax::Posix),
    }
}

#[cfg(not(target_os = "windows"))]
fn web_search_enabled_in_shell_configs(home: &Path) -> Result<bool, AppError> {
    for (path, _) in shell_config_paths(home) {
        if !path.exists() {
            continue;
        }
        let content = std::fs::read_to_string(&path).map_err(|error| AppError::io(&path, error))?;
        if content
            .lines()
            .filter_map(shell_assignment_value)
            .any(|value| is_truthy_env_value(&value))
        {
            return Ok(true);
        }
    }
    Ok(false)
}

#[cfg(not(target_os = "windows"))]
fn set_web_search_in_shell_configs(
    home: &Path,
    shell: Option<&str>,
    enabled: bool,
) -> Result<(), AppError> {
    for (path, syntax) in shell_config_paths(home) {
        if !path.exists() {
            continue;
        }
        let content = std::fs::read_to_string(&path).map_err(|error| AppError::io(&path, error))?;
        let rewritten = rewrite_shell_config(&content, syntax, false);
        if rewritten != content {
            write_shell_config(&path, &rewritten)?;
        }
    }

    if enabled {
        let (path, syntax) = active_shell_config(home, shell);
        let content = if path.exists() {
            std::fs::read_to_string(&path).map_err(|error| AppError::io(&path, error))?
        } else {
            String::new()
        };
        let rewritten = rewrite_shell_config(&content, syntax, true);
        write_shell_config(&path, &rewritten)?;
    }

    Ok(())
}

#[cfg(not(target_os = "windows"))]
pub fn get_web_search_enabled() -> Result<bool, AppError> {
    web_search_enabled_in_shell_configs(&crate::config::get_home_dir())
}

#[cfg(not(target_os = "windows"))]
pub fn set_web_search_enabled(enabled: bool) -> Result<(), AppError> {
    let home = crate::config::get_home_dir();
    let shell = std::env::var("SHELL").ok();
    set_web_search_in_shell_configs(&home, shell.as_deref(), enabled)
}

#[cfg(target_os = "windows")]
pub fn get_web_search_enabled() -> Result<bool, AppError> {
    let registry_enabled = RegKey::predef(HKEY_CURRENT_USER)
        .open_subkey_with_flags("Environment", KEY_READ)
        .ok()
        .and_then(|key| key.get_value::<String, _>(OPENCODE_WEB_SEARCH_ENV_VAR).ok())
        .map(|value| is_truthy_env_value(&value))
        .unwrap_or(false);
    Ok(registry_enabled)
}

#[cfg(target_os = "windows")]
pub fn set_web_search_enabled(enabled: bool) -> Result<(), AppError> {
    let (environment, _) = RegKey::predef(HKEY_CURRENT_USER)
        .create_subkey_with_flags("Environment", KEY_WRITE)
        .map_err(|error| {
            AppError::Config(format!(
                "Failed to open the current user's environment variables: {error}"
            ))
        })?;

    if enabled {
        environment
            .set_value(OPENCODE_WEB_SEARCH_ENV_VAR, &"true")
            .map_err(|error| {
                AppError::Config(format!("Failed to enable OpenCode web search: {error}"))
            })?;
    } else {
        match environment.delete_value(OPENCODE_WEB_SEARCH_ENV_VAR) {
            Ok(()) => {}
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => {
                return Err(AppError::Config(format!(
                    "Failed to disable OpenCode web search: {error}"
                )));
            }
        }
    }

    Ok(())
}

pub fn get_providers() -> Result<Map<String, Value>, AppError> {
    let config = read_opencode_config()?;
    Ok(config
        .get("provider")
        .and_then(|v| v.as_object())
        .cloned()
        .unwrap_or_default())
}

pub fn set_provider(id: &str, config: Value) -> Result<(), AppError> {
    let mut full_config = read_opencode_config()?;

    if full_config.get("provider").is_none() {
        full_config["provider"] = json!({});
    }

    if let Some(providers) = full_config
        .get_mut("provider")
        .and_then(|v| v.as_object_mut())
    {
        providers.insert(id.to_string(), config);
    }

    write_opencode_config(&full_config)
}

pub fn remove_provider(id: &str) -> Result<(), AppError> {
    let mut config = read_opencode_config()?;

    if let Some(providers) = config.get_mut("provider").and_then(|v| v.as_object_mut()) {
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

pub fn get_mcp_servers() -> Result<Map<String, Value>, AppError> {
    let config = read_opencode_config()?;
    Ok(config
        .get("mcp")
        .and_then(|v| v.as_object())
        .cloned()
        .unwrap_or_default())
}

pub fn set_mcp_server(id: &str, config: Value) -> Result<(), AppError> {
    let mut full_config = read_opencode_config()?;

    if full_config.get("mcp").is_none() {
        full_config["mcp"] = json!({});
    }

    if let Some(mcp) = full_config.get_mut("mcp").and_then(|v| v.as_object_mut()) {
        mcp.insert(id.to_string(), config);
    }

    write_opencode_config(&full_config)
}

pub fn remove_mcp_server(id: &str) -> Result<(), AppError> {
    let mut config = read_opencode_config()?;

    if let Some(mcp) = config.get_mut("mcp").and_then(|v| v.as_object_mut()) {
        mcp.remove(id);
    }

    write_opencode_config(&config)
}

pub fn add_plugin(plugin_name: &str) -> Result<(), AppError> {
    let mut config = read_opencode_config()?;
    let normalized_plugin_name = canonicalize_plugin_name(plugin_name);

    let plugins = config.get_mut("plugin").and_then(|v| v.as_array_mut());

    match plugins {
        Some(arr) => {
            // Mutual exclusion: standard OMO and OMO Slim cannot coexist as plugins
            if matches_any_plugin_prefix(&normalized_plugin_name, &STANDARD_OMO_PLUGIN_PREFIXES) {
                arr.retain(|v| {
                    v.as_str()
                        .map(|s| {
                            !matches_any_plugin_prefix(s, &STANDARD_OMO_PLUGIN_PREFIXES)
                                && !matches_any_plugin_prefix(s, &SLIM_OMO_PLUGIN_PREFIXES)
                        })
                        .unwrap_or(true)
                });
            } else if matches_any_plugin_prefix(&normalized_plugin_name, &SLIM_OMO_PLUGIN_PREFIXES)
            {
                arr.retain(|v| {
                    v.as_str()
                        .map(|s| {
                            !matches_any_plugin_prefix(s, &STANDARD_OMO_PLUGIN_PREFIXES)
                                && !matches_any_plugin_prefix(s, &SLIM_OMO_PLUGIN_PREFIXES)
                        })
                        .unwrap_or(true)
                });
            }

            let already_exists = arr
                .iter()
                .any(|v| v.as_str() == Some(normalized_plugin_name.as_str()));
            if !already_exists {
                arr.push(Value::String(normalized_plugin_name));
            }
        }
        None => {
            config["plugin"] = json!([normalized_plugin_name]);
        }
    }

    write_opencode_config(&config)
}

pub fn remove_plugins_by_prefixes(prefixes: &[&str]) -> Result<(), AppError> {
    let mut config = read_opencode_config()?;

    if let Some(arr) = config.get_mut("plugin").and_then(|v| v.as_array_mut()) {
        arr.retain(|v| {
            v.as_str()
                .map(|s| !matches_any_plugin_prefix(s, prefixes))
                .unwrap_or(true)
        });

        if arr.is_empty() {
            config.as_object_mut().map(|obj| obj.remove("plugin"));
        }
    }

    write_opencode_config(&config)
}

#[cfg(test)]
mod tests {
    #[cfg(not(target_os = "windows"))]
    use super::{
        rewrite_shell_config, set_web_search_in_shell_configs, shell_assignment_value,
        web_search_enabled_in_shell_configs, ShellConfigSyntax,
    };
    use super::{set_small_model_in_config, small_model_from_config};
    use serde_json::json;

    #[test]
    fn reads_and_normalizes_small_model() {
        let config = json!({ "small_model": "  opencode/north-mini-code-free  " });
        assert_eq!(
            small_model_from_config(&config).unwrap().as_deref(),
            Some("opencode/north-mini-code-free")
        );
    }

    #[test]
    fn updates_small_model_without_touching_other_config() {
        let mut config = json!({
            "$schema": "https://opencode.ai/config.json",
            "provider": { "custom": { "name": "Custom" } },
            "plugin": ["oh-my-opencode-slim@latest"],
            "agent": { "build": { "mode": "primary" } }
        });
        let expected_other_fields = config.clone();

        set_small_model_in_config(&mut config, Some(" openai/gpt-5.6-mini ")).unwrap();

        assert_eq!(config["small_model"], "openai/gpt-5.6-mini");
        for key in ["$schema", "provider", "plugin", "agent"] {
            assert_eq!(config[key], expected_other_fields[key]);
        }
    }

    #[test]
    fn empty_small_model_removes_the_field() {
        let mut config = json!({
            "small_model": "opencode/big-pickle",
            "provider": { "custom": {} }
        });

        set_small_model_in_config(&mut config, Some("   ")).unwrap();

        assert!(config.get("small_model").is_none());
        assert!(config.get("provider").is_some());
    }

    #[cfg(not(target_os = "windows"))]
    #[test]
    fn recognizes_posix_and_fish_web_search_assignments() {
        assert_eq!(
            shell_assignment_value("export OPENCODE_ENABLE_EXA=true").as_deref(),
            Some("true")
        );
        assert_eq!(
            shell_assignment_value("OPENCODE_ENABLE_EXA='1' # OpenCode").as_deref(),
            Some("1")
        );
        assert_eq!(
            shell_assignment_value("set -gx OPENCODE_ENABLE_EXA true").as_deref(),
            Some("true")
        );
        assert_eq!(shell_assignment_value("set -q OPENCODE_ENABLE_EXA"), None);
        assert_eq!(shell_assignment_value("# OPENCODE_ENABLE_EXA=true"), None);
        assert_eq!(shell_assignment_value("OPENCODE_ENABLE_EXAMPLE=true"), None);
    }

    #[cfg(not(target_os = "windows"))]
    #[test]
    fn enabling_web_search_replaces_existing_posix_assignment() {
        let source = "export EDITOR=vim\nOPENCODE_ENABLE_EXA=false\n";
        let rewritten = rewrite_shell_config(source, ShellConfigSyntax::Posix, true);

        assert_eq!(
            rewritten,
            "export EDITOR=vim\nexport OPENCODE_ENABLE_EXA=true\n"
        );
    }

    #[cfg(not(target_os = "windows"))]
    #[test]
    fn disabling_web_search_only_removes_the_target_assignment() {
        let source = "set -gx EDITOR nvim\nset -gx OPENCODE_ENABLE_EXA true\nset -gx OTHER value\n";
        let rewritten = rewrite_shell_config(source, ShellConfigSyntax::Fish, false);

        assert_eq!(rewritten, "set -gx EDITOR nvim\nset -gx OTHER value\n");
    }

    #[cfg(not(target_os = "windows"))]
    #[test]
    fn persists_web_search_setting_in_an_isolated_shell_profile() {
        let home = tempfile::tempdir().expect("create isolated home");
        let zshrc = home.path().join(".zshrc");
        std::fs::write(&zshrc, "export EDITOR=nvim\n").expect("seed zsh config");

        set_web_search_in_shell_configs(home.path(), Some("/bin/zsh"), true)
            .expect("enable web search");
        assert!(web_search_enabled_in_shell_configs(home.path()).expect("read enabled state"));
        assert_eq!(
            std::fs::read_to_string(&zshrc).expect("read enabled config"),
            "export EDITOR=nvim\nexport OPENCODE_ENABLE_EXA=true\n"
        );

        set_web_search_in_shell_configs(home.path(), Some("/bin/zsh"), false)
            .expect("disable web search");
        assert!(!web_search_enabled_in_shell_configs(home.path()).expect("read disabled state"));
        assert_eq!(
            std::fs::read_to_string(&zshrc).expect("read disabled config"),
            "export EDITOR=nvim\n"
        );
    }

    #[cfg(unix)]
    #[test]
    fn preserves_symlinked_shell_profiles() {
        use std::os::unix::fs::symlink;

        let home = tempfile::tempdir().expect("create isolated home");
        let dotfiles_dir = home.path().join("dotfiles");
        std::fs::create_dir(&dotfiles_dir).expect("create dotfiles directory");
        let target = dotfiles_dir.join("zshrc");
        std::fs::write(&target, "export EDITOR=nvim\n").expect("seed target config");
        let zshrc = home.path().join(".zshrc");
        symlink(&target, &zshrc).expect("create shell profile symlink");

        set_web_search_in_shell_configs(home.path(), Some("/bin/zsh"), true)
            .expect("enable web search");

        assert!(zshrc.is_symlink());
        assert_eq!(
            std::fs::read_to_string(&target).expect("read symlink target"),
            "export EDITOR=nvim\nexport OPENCODE_ENABLE_EXA=true\n"
        );
    }
}
