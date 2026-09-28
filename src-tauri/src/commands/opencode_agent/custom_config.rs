//! Custom agents defined in OpenCode JSON/JSONC configuration files.

use super::{builtin, file_modified_millis, validate_agent_id, OpenCodeAgentDocument};
use crate::{config::atomic_write, error::AppError};
use serde_json::Value;
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

fn definition<'a>(config: &'a Value, key: &str, id: &str) -> Option<&'a Value> {
    config.get(key)?.get(id)
}

fn document_from_entry(
    path: &Path,
    scope: &str,
    id: &str,
    value: &Value,
) -> Result<OpenCodeAgentDocument, AppError> {
    let mut fields = builtin::agent_from_v2(value.clone());
    let object = fields.as_object_mut().ok_or_else(|| {
        AppError::Config(format!(
            "Agent '{id}' in {} must be an object",
            path.display()
        ))
    })?;
    let prompt = object
        .remove("prompt")
        .and_then(|value| value.as_str().map(str::to_owned));
    Ok(OpenCodeAgentDocument {
        id: id.to_string(),
        scope: scope.to_string(),
        file_path: path.to_string_lossy().into_owned(),
        frontmatter: fields,
        prompt: prompt.clone().unwrap_or_default(),
        has_prompt_override: prompt.is_some(),
        last_modified: file_modified_millis(path),
        managed_by: None,
        built_in: false,
        native_v2: true,
        default_frontmatter: None,
    })
}

pub(super) fn list(agents_dir: &Path, scope: &str) -> Result<Vec<OpenCodeAgentDocument>, AppError> {
    let mut documents = BTreeMap::new();
    for path in builtin::config_paths(agents_dir, scope) {
        if !path.exists() {
            continue;
        }
        let config = builtin::read_config(&path)?;
        for key in ["agents"] {
            let Some(entries) = config.get(key).and_then(Value::as_object) else {
                continue;
            };
            for (id, value) in entries {
                if builtin::defaults(id).is_some() || validate_agent_id(id).is_err() {
                    continue;
                }
                documents.insert(id.clone(), document_from_entry(&path, scope, id, value)?);
            }
        }
    }
    Ok(documents.into_values().collect())
}

fn sources_for_id(
    agents_dir: &Path,
    scope: &str,
    id: &str,
) -> Result<Vec<(PathBuf, Value, &'static str)>, AppError> {
    let mut sources = Vec::new();
    for path in builtin::config_paths(agents_dir, scope) {
        if !path.exists() {
            continue;
        }
        let config = builtin::read_config(&path)?;
        for key in ["agents"] {
            if definition(&config, key, id).is_some() {
                sources.push((path.clone(), config.clone(), key));
            }
        }
    }
    Ok(sources)
}

pub(super) fn save(
    agents_dir: &Path,
    scope: &str,
    agent: &OpenCodeAgentDocument,
    original_id: Option<&str>,
) -> Result<Option<OpenCodeAgentDocument>, AppError> {
    let Some(original_id) = original_id else {
        if !sources_for_id(agents_dir, scope, &agent.id)?.is_empty() {
            return Err(AppError::InvalidInput(format!(
                "Agent '{}' already exists in OpenCode config",
                agent.id
            )));
        }
        return Ok(None);
    };
    if agents_dir.join(format!("{original_id}.md")).exists() {
        if agent.id != original_id && !sources_for_id(agents_dir, scope, &agent.id)?.is_empty() {
            return Err(AppError::InvalidInput(format!(
                "Agent '{}' already exists in OpenCode config",
                agent.id
            )));
        }
        return Ok(None);
    }
    let mut sources = sources_for_id(agents_dir, scope, original_id)?;
    if sources.is_empty() {
        if agent.id != original_id && !sources_for_id(agents_dir, scope, &agent.id)?.is_empty() {
            return Err(AppError::InvalidInput(format!(
                "Agent '{}' already exists in OpenCode config",
                agent.id
            )));
        }
        return Ok(None);
    }
    if sources.len() != 1 {
        return Err(AppError::InvalidInput(format!(
            "Agent '{original_id}' is defined in multiple config locations; edit those files directly"
        )));
    }
    if agent.id != original_id
        && (!sources_for_id(agents_dir, scope, &agent.id)?.is_empty()
            || agents_dir.join(format!("{}.md", agent.id)).exists())
    {
        return Err(AppError::InvalidInput(format!(
            "Agent '{}' already exists",
            agent.id
        )));
    }
    let (path, mut config, key) = sources.remove(0);
    let mut fields = agent.frontmatter.clone();
    let object = fields
        .as_object_mut()
        .ok_or_else(|| AppError::InvalidInput("Agent frontmatter must be an object".into()))?;
    if agent.has_prompt_override || !agent.prompt.is_empty() {
        object.insert("prompt".into(), Value::String(agent.prompt.clone()));
    }
    if key == "agents" {
        fields = builtin::agent_to_v2(fields);
    }
    let entries = config
        .get_mut(key)
        .and_then(Value::as_object_mut)
        .ok_or_else(|| AppError::Config(format!("OpenCode {key} must be an object")))?;
    if agent.id != original_id {
        entries.remove(original_id);
    }
    entries.insert(agent.id.clone(), fields);
    let bytes =
        serde_json::to_vec_pretty(&config).map_err(|error| AppError::Config(error.to_string()))?;
    atomic_write(&path, &bytes)?;
    Ok(Some(
        list(agents_dir, scope)?
            .into_iter()
            .find(|item| item.id == agent.id)
            .ok_or_else(|| AppError::Config("Saved Agent was not found".into()))?,
    ))
}

pub(super) fn delete(agents_dir: &Path, scope: &str, id: &str) -> Result<(), AppError> {
    let mut changes = Vec::new();
    for path in builtin::config_paths(agents_dir, scope) {
        if !path.exists() {
            continue;
        }
        let mut config = builtin::read_config(&path)?;
        let mut changed = false;
        for key in ["agents"] {
            if let Some(entries) = config.get_mut(key).and_then(Value::as_object_mut) {
                changed |= entries.remove(id).is_some();
            }
        }
        if changed {
            let before = std::fs::read(&path).map_err(|error| AppError::io(&path, error))?;
            let after = serde_json::to_vec_pretty(&config)
                .map_err(|error| AppError::Config(error.to_string()))?;
            changes.push((path, before, after));
        }
    }
    for index in 0..changes.len() {
        if let Err(error) = atomic_write(&changes[index].0, &changes[index].2) {
            for (path, before, _) in changes[..index].iter().rev() {
                if let Err(rollback) = atomic_write(path, before) {
                    return Err(AppError::Config(format!(
                        "{error}; failed to restore {}: {rollback}",
                        path.display()
                    )));
                }
            }
            return Err(error);
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn edits_and_deletes_native_v2_custom_config_agent() {
        let dir = tempfile::tempdir().unwrap();
        let agents_dir = dir.path().join("agents");
        let path = dir.path().join("opencode.jsonc");
        std::fs::write(&path, r#"{"agents":{"reviewer":{"description":"Review","system":"Old prompt","model":"openai/coding#high","permissions":[{"action":"shell","resource":"*","effect":"deny"}]}}}"#).unwrap();
        let mut agent = list(&agents_dir, "global").unwrap().remove(0);
        assert!(agent.native_v2);
        assert_eq!(agent.prompt, "Old prompt");
        assert_eq!(agent.frontmatter["variant"], "high");
        assert_eq!(agent.frontmatter["permission"]["bash"], "deny");
        agent.frontmatter["variant"] = json!("low");
        agent.prompt = "New prompt".into();
        let saved = save(&agents_dir, "global", &agent, Some("reviewer"))
            .unwrap()
            .unwrap();
        assert_eq!(saved.frontmatter["variant"], "low");
        let config = builtin::read_config(&path).unwrap();
        assert_eq!(config["agents"]["reviewer"]["model"], "openai/coding#low");
        assert_eq!(config["agents"]["reviewer"]["system"], "New prompt");
        assert_eq!(
            config["agents"]["reviewer"]["permissions"][0]["action"],
            "shell"
        );
        assert!(config.get("agent").is_none());
        delete(&agents_dir, "global", "reviewer").unwrap();
        assert!(builtin::read_config(&path).unwrap()["agents"]
            .get("reviewer")
            .is_none());
    }
}
