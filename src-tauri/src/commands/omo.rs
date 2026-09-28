use tauri::State;

use crate::services::omo::{OmoLocalFileData, SLIM, STANDARD};
use crate::services::OmoService;
use crate::store::AppState;
use serde::Serialize;
use serde_json::Value;
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

fn parse_opencode_v2_models(stdout: &str) -> Result<Vec<OmoOpenCodeModel>, String> {
    let response: Value = serde_json::from_str(stdout)
        .map_err(|error| format!("OpenCode returned invalid model data: {error}"))?;
    let data = response
        .get("data")
        .and_then(Value::as_array)
        .ok_or_else(|| "OpenCode returned a model response without data".to_string())?;
    Ok(data
        .iter()
        .filter_map(|model| {
            if model.get("enabled") == Some(&Value::Bool(false)) {
                return None;
            }
            let provider_id = model.get("providerID")?.as_str()?.to_string();
            let model_id = model.get("modelID")?.as_str()?.to_string();
            let name = model
                .get("name")
                .and_then(Value::as_str)
                .unwrap_or(&model_id)
                .to_string();
            let variants = model
                .get("variants")
                .and_then(Value::as_array)
                .map(|items| {
                    items
                        .iter()
                        .filter_map(|item| item.get("id").and_then(Value::as_str))
                        .map(str::to_string)
                        .collect()
                })
                .unwrap_or_default();
            Some(OmoOpenCodeModel {
                value: format!("{provider_id}/{model_id}"),
                provider_id,
                model_id,
                name,
                variants,
                options: model.get("settings").cloned(),
                limit: model.get("limit").cloned(),
            })
        })
        .collect())
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
    let mut last_error = None;

    for path in &search_paths {
        let combined_path = format!("{}{}{}", path.display(), separator, current_path);
        for executable in super::misc::tool_executable_candidates("opencode", path) {
            if !executable.exists() {
                continue;
            }

            let args = ["api", "--standalone", "model.list"];

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
                            "call \"{}\" api --standalone model.list",
                            executable.display()
                        ))
                        .env("PATH", &combined_path)
                        .creation_flags(CREATE_NO_WINDOW)
                        .output()
                } else {
                    Command::new(&executable)
                        .args(args)
                        .env("PATH", &combined_path)
                        .creation_flags(CREATE_NO_WINDOW)
                        .output()
                }
            };

            #[cfg(not(target_os = "windows"))]
            let output = Command::new(&executable)
                .args(args)
                .env("PATH", &combined_path)
                .output();

            match output {
                Ok(output) if output.status.success() => return Ok(output),
                Ok(output) => {
                    last_error = Some(String::from_utf8_lossy(&output.stderr).trim().to_string());
                }
                Err(error) => last_error = Some(error.to_string()),
            }
        }
    }

    Err(last_error
        .filter(|error| !error.is_empty())
        .map(|error| format!("OpenCode V2 failed to list models: {error}"))
        .unwrap_or_else(|| "OpenCode V2 CLI is unavailable".to_string()))
}

#[tauri::command]
pub async fn list_opencode_models_for_omo() -> Result<Vec<OmoOpenCodeModel>, String> {
    tauri::async_runtime::spawn_blocking(|| {
        let output = run_opencode_models_command()?;
        let stdout = String::from_utf8_lossy(&output.stdout);
        let models = parse_opencode_v2_models(&stdout)?;
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
    use super::parse_opencode_v2_models;

    #[test]
    fn parses_v2_model_catalog_with_variants_and_limits() {
        let output = r#"{"location":{},"data":[{"id":"openai/gpt-5.6","providerID":"openai","modelID":"gpt-5.6","name":"GPT-5.6","enabled":true,"variants":[{"id":"low","settings":{}},{"id":"high","settings":{}}],"settings":{"temperature":1},"limit":{"context":1000}},{"providerID":"openai","modelID":"disabled","enabled":false}]}"#;
        let models = parse_opencode_v2_models(output).unwrap();
        assert_eq!(models.len(), 1);
        assert_eq!(models[0].value, "openai/gpt-5.6");
        assert_eq!(models[0].variants, ["low", "high"]);
        assert_eq!(models[0].options.as_ref().unwrap()["temperature"], 1);
        assert_eq!(models[0].limit.as_ref().unwrap()["context"], 1000);
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
