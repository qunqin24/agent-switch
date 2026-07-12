use tauri::State;

use crate::services::omo::{OmoLocalFileData, SLIM, STANDARD};
use crate::services::OmoService;
use crate::store::AppState;
use serde::Serialize;
use serde_json::Value;
use std::collections::HashSet;
use std::process::{Command, Output};

#[cfg(target_os = "windows")]
use std::os::windows::process::CommandExt;

#[cfg(target_os = "windows")]
const CREATE_NO_WINDOW: u32 = 0x08000000;

#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct OmoOpenCodeModel {
    pub value: String,
    pub provider_id: String,
    pub model_id: String,
    pub name: String,
    pub variants: Vec<String>,
    pub options: Option<Value>,
    pub limit: Option<Value>,
}

fn update_json_depth(line: &str, depth: &mut i32) {
    let mut in_string = false;
    let mut escaped = false;
    for ch in line.chars() {
        if in_string {
            if escaped {
                escaped = false;
            } else if ch == '\\' {
                escaped = true;
            } else if ch == '"' {
                in_string = false;
            }
            continue;
        }
        match ch {
            '"' => in_string = true,
            '{' => *depth += 1,
            '}' => *depth -= 1,
            _ => {}
        }
    }
}

fn parse_opencode_verbose_models(stdout: &str) -> Vec<OmoOpenCodeModel> {
    let mut result = Vec::new();
    let mut seen = HashSet::new();
    let mut pending_key: Option<String> = None;
    let mut json_buffer = String::new();
    let mut json_depth = 0;

    for raw_line in stdout.lines() {
        let line = raw_line.trim();
        if json_depth == 0 {
            if line.starts_with('{') && pending_key.is_some() {
                json_buffer.clear();
                json_buffer.push_str(line);
                update_json_depth(line, &mut json_depth);
            } else if line.contains('/') && !line.chars().any(char::is_whitespace) {
                pending_key = Some(line.to_string());
            }
        } else {
            json_buffer.push('\n');
            json_buffer.push_str(line);
            update_json_depth(line, &mut json_depth);
        }

        if json_depth != 0 || json_buffer.is_empty() {
            continue;
        }
        let Some(value) = pending_key.take() else {
            json_buffer.clear();
            continue;
        };
        let Ok(metadata) = serde_json::from_str::<Value>(&json_buffer) else {
            json_buffer.clear();
            continue;
        };
        json_buffer.clear();

        let provider_id = metadata
            .get("providerID")
            .and_then(Value::as_str)
            .or_else(|| value.split_once('/').map(|(provider, _)| provider))
            .unwrap_or_default()
            .to_string();
        let model_id = metadata
            .get("id")
            .and_then(Value::as_str)
            .or_else(|| value.split_once('/').map(|(_, model)| model))
            .unwrap_or_default()
            .to_string();
        if provider_id.is_empty() || model_id.is_empty() || !seen.insert(value.clone()) {
            continue;
        }
        let name = metadata
            .get("name")
            .and_then(Value::as_str)
            .unwrap_or(&model_id)
            .to_string();
        let variants = metadata
            .get("variants")
            .and_then(Value::as_object)
            .map(|variants| variants.keys().cloned().collect())
            .unwrap_or_default();

        result.push(OmoOpenCodeModel {
            value,
            provider_id,
            model_id,
            name,
            variants,
            options: metadata.get("options").cloned(),
            limit: metadata.get("limit").cloned(),
        });
    }

    result
}

fn run_opencode_models_command() -> Result<Output, String> {
    let search_paths = super::misc::build_tool_search_paths("opencode");
    let current_path = std::env::var_os("PATH")
        .map(|value| value.to_string_lossy().into_owned())
        .unwrap_or_default();
    let separator = if cfg!(target_os = "windows") {
        ";"
    } else {
        ":"
    };

    for path in &search_paths {
        let combined_path = format!("{}{}{}", path.display(), separator, current_path);
        for executable in super::misc::tool_executable_candidates("opencode", path) {
            if !executable.exists() {
                continue;
            }

            #[cfg(target_os = "windows")]
            let output = {
                let extension = executable
                    .extension()
                    .and_then(|value| value.to_str())
                    .unwrap_or_default();
                if extension.eq_ignore_ascii_case("cmd") || extension.eq_ignore_ascii_case("bat") {
                    Command::new("cmd")
                        .args(["/D", "/S", "/C"])
                        .arg(format!(
                            "call \"{}\" models --pure --verbose",
                            executable.display()
                        ))
                        .env("PATH", &combined_path)
                        .creation_flags(CREATE_NO_WINDOW)
                        .output()
                } else {
                    Command::new(&executable)
                        .args(["models", "--pure", "--verbose"])
                        .env("PATH", &combined_path)
                        .creation_flags(CREATE_NO_WINDOW)
                        .output()
                }
            };

            #[cfg(not(target_os = "windows"))]
            let output = Command::new(&executable)
                .args(["models", "--pure", "--verbose"])
                .env("PATH", &combined_path)
                .output();

            if let Ok(output) = output {
                if output.status.success() {
                    return Ok(output);
                }
            }
        }
    }

    Err("OpenCode CLI is unavailable or failed to list models".to_string())
}

#[tauri::command]
pub async fn list_opencode_models_for_omo() -> Result<Vec<OmoOpenCodeModel>, String> {
    tauri::async_runtime::spawn_blocking(|| {
        let output = run_opencode_models_command()?;
        let stdout = String::from_utf8_lossy(&output.stdout);
        let models = parse_opencode_verbose_models(&stdout);
        if models.is_empty() {
            return Err("OpenCode returned an empty model catalog".to_string());
        }
        Ok(models)
    })
    .await
    .map_err(|error| format!("Failed to query OpenCode models: {error}"))?
}

#[tauri::command]
pub async fn read_omo_local_file() -> Result<OmoLocalFileData, String> {
    OmoService::read_local_file(&STANDARD).map_err(|e| e.to_string())
}

#[cfg(test)]
mod model_catalog_tests {
    use super::parse_opencode_verbose_models;

    #[test]
    fn parses_verbose_model_catalog_with_variants_and_limits() {
        let output = r#"
openai/gpt-5.6
{
  "id": "gpt-5.6",
  "providerID": "openai",
  "name": "GPT-5.6",
  "options": {},
  "limit": { "context": 1050000, "output": 128000 },
  "variants": {
    "low": { "reasoningEffort": "low" },
    "high": { "reasoningEffort": "high" }
  }
}
zhipuai-coding-plan/glm-5.2
{
  "id": "glm-5.2",
  "providerID": "zhipuai-coding-plan",
  "name": "GLM-5.2",
  "limit": { "context": 1000000 },
  "variants": { "high": {}, "max": {} }
}
"#;

        let models = parse_opencode_verbose_models(output);
        assert_eq!(models.len(), 2);
        assert_eq!(models[0].value, "openai/gpt-5.6");
        assert_eq!(models[0].variants, vec!["low", "high"]);
        assert_eq!(models[0].limit.as_ref().unwrap()["context"], 1050000);
        assert_eq!(models[1].value, "zhipuai-coding-plan/glm-5.2");
    }
}

#[tauri::command]
pub async fn get_current_omo_provider_id(state: State<'_, AppState>) -> Result<String, String> {
    let provider = state
        .db
        .get_current_omo_provider("opencode", "omo")
        .map_err(|e| e.to_string())?;
    Ok(provider.map(|p| p.id).unwrap_or_default())
}

#[tauri::command]
pub async fn disable_current_omo(state: State<'_, AppState>) -> Result<(), String> {
    let providers = state
        .db
        .get_all_providers("opencode")
        .map_err(|e| e.to_string())?;
    for (id, p) in &providers {
        if p.category.as_deref() == Some("omo") {
            state
                .db
                .clear_omo_provider_current("opencode", id, "omo")
                .map_err(|e| e.to_string())?;
        }
    }
    OmoService::delete_config_file(&STANDARD).map_err(|e| e.to_string())?;
    Ok(())
}

// ── OMO Slim commands ───────────────────────────────────────

#[tauri::command]
pub async fn read_omo_slim_local_file() -> Result<OmoLocalFileData, String> {
    OmoService::read_local_file(&SLIM).map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn get_current_omo_slim_provider_id(
    state: State<'_, AppState>,
) -> Result<String, String> {
    let provider = state
        .db
        .get_current_omo_provider("opencode", "omo-slim")
        .map_err(|e| e.to_string())?;
    Ok(provider.map(|p| p.id).unwrap_or_default())
}

#[tauri::command]
pub async fn disable_current_omo_slim(state: State<'_, AppState>) -> Result<(), String> {
    let providers = state
        .db
        .get_all_providers("opencode")
        .map_err(|e| e.to_string())?;
    for (id, p) in &providers {
        if p.category.as_deref() == Some("omo-slim") {
            state
                .db
                .clear_omo_provider_current("opencode", id, "omo-slim")
                .map_err(|e| e.to_string())?;
        }
    }
    OmoService::delete_config_file(&SLIM).map_err(|e| e.to_string())?;
    Ok(())
}
