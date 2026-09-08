//! Native agent identities come from OpenCode v1.18.29's agent/agent.ts.
//! Only presentation defaults are copied: prompts and permissions must remain
//! inherited from the installed OpenCode version, including dynamic path rules.
use super::{
    file_modified_millis, serialize_agent_markdown, split_agent_markdown, OpenCodeAgentDocument,
};
use crate::{config::atomic_write, error::AppError};
use serde_json::{json, Value};
use std::path::{Path, PathBuf};

const IDS: [&str; 7] = [
    "build",
    "plan",
    "general",
    "explore",
    "compaction",
    "title",
    "summary",
];

pub(super) fn defaults(id: &str) -> Option<Value> {
    if !IDS.contains(&id) {
        return None;
    }
    Some(match id {
        "general" | "explore" => json!({"mode": "subagent"}),
        "compaction" | "summary" => json!({"mode": "primary", "hidden": true}),
        "title" => json!({"mode": "primary", "hidden": true, "temperature": 0.5}),
        _ => json!({"mode": "primary"}),
    })
}

fn config_paths(agents_dir: &Path, scope: &str) -> Vec<PathBuf> {
    let directory = agents_dir.parent().expect("agents directory has a parent");
    let mut paths = Vec::new();
    if scope == "global" {
        paths.push(directory.join("config.json"));
    } else if let Some(project) = directory.parent() {
        paths.extend([
            project.join("opencode.json"),
            project.join("opencode.jsonc"),
        ]);
    }
    paths.extend([
        directory.join("opencode.json"),
        directory.join("opencode.jsonc"),
    ]);
    paths
}

fn markdown_paths(agents_dir: &Path, id: &str) -> [PathBuf; 2] {
    [
        agents_dir
            .parent()
            .unwrap()
            .join("agent")
            .join(format!("{id}.md")),
        agents_dir.join(format!("{id}.md")),
    ]
}

fn read_config(path: &Path) -> Result<Value, AppError> {
    let raw = std::fs::read_to_string(path).map_err(|error| AppError::io(path, error))?;
    let config: Value = json5::from_str(&raw).map_err(|error| {
        AppError::Config(format!(
            "Invalid OpenCode config {}: {error}",
            path.display()
        ))
    })?;
    if !config.is_object()
        || config
            .get("agent")
            .is_some_and(|agents| !agents.is_object())
    {
        return Err(AppError::Config(format!(
            "OpenCode config and agent fields must be objects: {}",
            path.display()
        )));
    }
    Ok(config)
}

fn merge(target: &mut Value, source: Value) {
    if let (Some(target), Some(source)) = (target.as_object_mut(), source.as_object()) {
        for (key, value) in source {
            merge(
                target.entry(key.clone()).or_insert(Value::Null),
                value.clone(),
            );
        }
    } else {
        *target = source;
    }
}

fn normalize_permissions(fields: &mut Value) {
    if let Some(action) = fields.get("permission").and_then(Value::as_str) {
        fields["permission"] = json!({"*": action});
    }
}

pub(super) fn list(agents_dir: &Path, scope: &str) -> Result<Vec<OpenCodeAgentDocument>, AppError> {
    let configs = config_paths(agents_dir, scope)
        .into_iter()
        .filter(|path| path.exists())
        .map(|path| read_config(&path).map(|config| (path, config)))
        .collect::<Result<Vec<_>, _>>()?;
    IDS.into_iter()
        .map(|id| {
            let mut frontmatter = json!({});
            let mut file_path = String::new();
            let mut last_modified = None;
            for (path, config) in &configs {
                if let Some(value) = config.get("agent").and_then(|agents| agents.get(id)) {
                    if !value.is_object() {
                        return Err(AppError::Config(format!("Agent '{id}' must be an object")));
                    }
                    let mut value = value.clone();
                    normalize_permissions(&mut value);
                    merge(&mut frontmatter, value);
                    file_path = path.to_string_lossy().into_owned();
                    last_modified = file_modified_millis(path);
                }
            }
            for path in markdown_paths(agents_dir, id) {
                if !path.exists() {
                    continue;
                }
                let raw =
                    std::fs::read_to_string(&path).map_err(|error| AppError::io(&path, error))?;
                let (mut fields, prompt) = split_agent_markdown(&raw)?;
                fields["prompt"] = Value::String(prompt.trim().into());
                normalize_permissions(&mut fields);
                merge(&mut frontmatter, fields);
                file_path = path.to_string_lossy().into_owned();
                last_modified = file_modified_millis(&path);
            }
            let prompt = frontmatter
                .as_object_mut()
                .unwrap()
                .shift_remove("prompt")
                .and_then(|value| value.as_str().map(str::to_owned));
            let has_prompt_override = prompt.is_some();
            Ok(OpenCodeAgentDocument {
                id: id.into(),
                scope: scope.into(),
                file_path,
                frontmatter,
                prompt: prompt.unwrap_or_default(),
                has_prompt_override,
                last_modified,
                managed_by: None,
                built_in: true,
                default_frontmatter: defaults(id),
            })
        })
        .collect()
}

struct FileChange {
    path: PathBuf,
    before: Option<Vec<u8>>,
    after: Option<Vec<u8>>,
}

impl FileChange {
    fn new(path: PathBuf, after: Option<Vec<u8>>) -> Result<Self, AppError> {
        let before = if path.exists() {
            Some(std::fs::read(&path).map_err(|error| AppError::io(&path, error))?)
        } else {
            None
        };
        Ok(Self {
            path,
            before,
            after,
        })
    }

    fn write(&self, bytes: Option<&[u8]>) -> Result<(), AppError> {
        match bytes {
            Some(bytes) => atomic_write(&self.path, bytes),
            None if self.path.exists() => {
                std::fs::remove_file(&self.path).map_err(|error| AppError::io(&self.path, error))
            }
            None => Ok(()),
        }
    }
}

// Keep each unchanged value in its original file. In particular, JSON file
// substitutions are relative to that file, while Markdown tokens are literal.
struct AgentSource {
    path: PathBuf,
    config: Option<Value>,
    before: Value,
    fields: Value,
}

fn source_value<'a>(fields: &'a Value, path: &[String]) -> Option<&'a Value> {
    let mut value = fields;
    for key in path {
        value = value.get(key)?;
    }
    Some(value)
}

fn remove_value(fields: &mut Value, path: &[String]) {
    let Some((key, parents)) = path.split_last() else {
        return;
    };
    let mut value = fields;
    for parent in parents {
        let Some(next) = value.get_mut(parent) else {
            return;
        };
        value = next;
    }
    if let Some(object) = value.as_object_mut() {
        object.shift_remove(key);
    }
}

fn set_value(fields: &mut Value, path: &[String], next: Value) {
    let mut value = fields;
    for key in path {
        if !value.is_object() {
            *value = json!({});
        }
        value = value
            .as_object_mut()
            .unwrap()
            .entry(key.clone())
            .or_insert(Value::Null);
    }
    *value = next;
}

fn edit_sources(
    sources: &mut [AgentSource],
    path: &mut Vec<String>,
    before: Option<&Value>,
    after: Option<&Value>,
) {
    if before == after {
        return;
    }
    if let (Some(before), Some(after)) = (
        before.and_then(Value::as_object),
        after.and_then(Value::as_object),
    ) {
        let mut keys = before.keys().cloned().collect::<Vec<_>>();
        keys.extend(
            after
                .keys()
                .filter(|key| !before.contains_key(*key))
                .cloned(),
        );
        for key in keys {
            path.push(key.clone());
            edit_sources(sources, path, before.get(&key), after.get(&key));
            path.pop();
        }
        return;
    }
    let target = sources
        .iter()
        .rposition(|source| source_value(&source.fields, path).is_some())
        .unwrap_or(sources.len() - 1);
    for (index, source) in sources.iter_mut().enumerate() {
        if after.is_none() || (index != target && after.is_some_and(Value::is_object)) {
            remove_value(&mut source.fields, path);
        }
    }
    if let Some(after) = after {
        set_value(&mut sources[target].fields, path, after.clone());
    }
}

fn ordered_equal(left: &Value, right: &Value) -> bool {
    // Map equality ignores insertion order, which controls permission precedence.
    match (left, right) {
        (Value::Object(left), Value::Object(right)) => {
            left.len() == right.len()
                && left
                    .iter()
                    .zip(right)
                    .all(|((left_key, left), (right_key, right))| {
                        left_key == right_key && ordered_equal(left, right)
                    })
        }
        (Value::Array(left), Value::Array(right)) => {
            left.len() == right.len()
                && left
                    .iter()
                    .zip(right)
                    .all(|(left, right)| ordered_equal(left, right))
        }
        _ => left == right,
    }
}

fn align_permission_order(
    sources: &mut [AgentSource],
    path: &mut Vec<String>,
    before: Option<&Value>,
    desired: &Value,
) {
    let Some(object) = desired.as_object() else {
        return;
    };
    let keys = object.keys().collect::<Vec<_>>();
    // New rules can be placed beside an existing rule without relocating any
    // existing values. Prefer the following anchor so broad MCP rules precede
    // their existing tool-specific exceptions, even across configuration files.
    for (position, key) in keys.iter().enumerate() {
        if before.and_then(|value| value.get(*key)).is_some() {
            continue;
        }
        let anchor = keys[position + 1..]
            .iter()
            .find_map(|next| {
                before.and_then(|value| value.get(*next))?;
                let mut anchor_path = path.clone();
                anchor_path.push((*next).clone());
                sources
                    .iter()
                    .position(|source| source_value(&source.fields, &anchor_path).is_some())
            })
            .or_else(|| {
                keys[..position].iter().rev().find_map(|previous| {
                    let mut anchor_path = path.clone();
                    anchor_path.push((*previous).clone());
                    sources
                        .iter()
                        .position(|source| source_value(&source.fields, &anchor_path).is_some())
                })
            })
            .unwrap_or(sources.len() - 1);
        path.push((*key).clone());
        for source in sources.iter_mut() {
            remove_value(&mut source.fields, path);
            // Moving a new rule must not leave empty containers in the file
            // where the generic field editor initially placed it.
            for length in (1..path.len()).rev() {
                let parent = &path[..length];
                if source_value(&source.before, parent).is_none()
                    && source_value(&source.fields, parent)
                        .and_then(Value::as_object)
                        .is_some_and(|object| object.is_empty())
                {
                    remove_value(&mut source.fields, parent);
                }
            }
        }
        set_value(&mut sources[anchor].fields, path, object[*key].clone());
        path.pop();
    }
    for source in sources.iter_mut() {
        let Some(existing) = source_value(&source.fields, path).and_then(Value::as_object) else {
            continue;
        };
        let mut ordered = serde_json::Map::new();
        for key in &keys {
            if let Some(value) = existing.get(*key) {
                ordered.insert((*key).clone(), value.clone());
            }
        }
        for (key, value) in existing {
            if !ordered.contains_key(key) {
                ordered.insert(key.clone(), value.clone());
            }
        }
        set_value(&mut source.fields, path, Value::Object(ordered));
    }
    for (key, value) in object {
        path.push(key.clone());
        align_permission_order(
            sources,
            path,
            before.and_then(|before| before.get(key)),
            value,
        );
        path.pop();
    }
}

/// Reset removes this scope's overrides. Saves patch only changed values at
/// their source, and parse all files before performing any mutation.
pub(super) fn write(
    agents_dir: &Path,
    scope: &str,
    id: &str,
    document: Option<&OpenCodeAgentDocument>,
) -> Result<(), AppError> {
    if defaults(id).is_none() {
        return Err(AppError::InvalidInput(
            "Only built-in agents can be reset".into(),
        ));
    }
    let mut sources = Vec::new();
    for path in config_paths(agents_dir, scope) {
        if !path.exists() {
            continue;
        }
        let config = read_config(&path)?;
        let mut fields = config
            .get("agent")
            .and_then(|agents| agents.get(id))
            .cloned()
            .unwrap_or_else(|| json!({}));
        if !fields.is_object() {
            return Err(AppError::Config(format!("Agent '{id}' must be an object")));
        }
        normalize_permissions(&mut fields);
        sources.push(AgentSource {
            path,
            config: Some(config),
            before: fields.clone(),
            fields,
        });
    }
    for path in markdown_paths(agents_dir, id) {
        if !path.exists() {
            continue;
        }
        let raw = std::fs::read_to_string(&path).map_err(|error| AppError::io(&path, error))?;
        let (mut fields, prompt) = split_agent_markdown(&raw)?;
        fields["prompt"] = json!(prompt.trim());
        normalize_permissions(&mut fields);
        sources.push(AgentSource {
            path,
            config: None,
            before: fields.clone(),
            fields,
        });
    }
    if let Some(document) = document {
        if sources.is_empty() {
            sources.push(AgentSource {
                path: agents_dir.parent().unwrap().join("opencode.json"),
                config: Some(json!({})),
                before: json!({}),
                fields: json!({}),
            });
        }
        let mut current = json!({});
        for source in &sources {
            merge(&mut current, source.fields.clone());
        }
        let mut next = document.frontmatter.clone();
        let object = next
            .as_object_mut()
            .ok_or_else(|| AppError::InvalidInput("Agent frontmatter must be an object".into()))?;
        object.shift_remove("prompt");
        if document.has_prompt_override || !document.prompt.is_empty() {
            object.insert("prompt".into(), json!(document.prompt));
        } else if sources.iter().any(|source| source.config.is_none()) {
            return Err(AppError::InvalidInput(
                "Use restore defaults to remove a Markdown prompt override".into(),
            ));
        }
        normalize_permissions(&mut next);
        edit_sources(&mut sources, &mut Vec::new(), Some(&current), Some(&next));
        if let Some(permission) = next.get("permission") {
            align_permission_order(
                &mut sources,
                &mut vec!["permission".into()],
                current.get("permission"),
                permission,
            );
            let mut effective = json!({});
            for source in &sources {
                merge(&mut effective, source.fields.clone());
            }
            if !effective
                .get("permission")
                .is_some_and(|actual| ordered_equal(actual, permission))
            {
                return Err(AppError::InvalidInput(
                    "Cannot preserve the requested permission rule order across configuration files; adjust those rules in their original files".into(),
                ));
            }
        }
    }
    let mut changes = Vec::new();
    for source in sources {
        if document.is_some() && ordered_equal(&source.fields, &source.before) {
            continue;
        }
        let bytes = if let Some(mut config) = source.config {
            let before = config.clone();
            if document.is_none() {
                if let Some(agents) = config.get_mut("agent").and_then(Value::as_object_mut) {
                    agents.shift_remove(id);
                }
            } else {
                if config.get("agent").is_none() {
                    config["agent"] = json!({});
                }
                config["agent"][id] = source.fields;
            }
            if ordered_equal(&config, &before) {
                continue;
            }
            Some(
                serde_json::to_vec_pretty(&config)
                    .map_err(|error| AppError::Config(error.to_string()))?,
            )
        } else if document.is_none() {
            None
        } else {
            let mut fields = source.fields;
            let prompt = fields
                .as_object_mut()
                .unwrap()
                .shift_remove("prompt")
                .and_then(|value| value.as_str().map(str::to_owned))
                .unwrap_or_default();
            Some(serialize_agent_markdown(&fields, &prompt)?.into_bytes())
        };
        changes.push(FileChange::new(source.path, bytes)?);
    }
    for (index, change) in changes.iter().enumerate() {
        if let Err(error) = change.write(change.after.as_deref()) {
            for previous in changes[..index].iter().rev() {
                if let Err(rollback_error) = previous.write(previous.before.as_deref()) {
                    return Err(AppError::Config(format!(
                        "{error}; failed to restore {}: {rollback_error}",
                        previous.path.display()
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

    #[test]
    fn lists_native_agents_without_creating_files() {
        let dir = tempfile::tempdir().unwrap();
        let agents_dir = dir.path().join("agents");
        let agents = list(&agents_dir, "global").unwrap();
        assert_eq!(agents.len(), 7);
        assert!(!agents_dir.exists());
        for agent in &agents {
            assert!(agent.built_in);
            assert_eq!(agent.frontmatter, json!({}));
            assert!(agent.prompt.is_empty());
            assert!(agent.file_path.is_empty());
        }
        assert_eq!(
            agents[0].default_frontmatter.as_ref().unwrap()["mode"],
            "primary"
        );
        assert_eq!(agents[3].id, "explore");
        assert_eq!(
            agents[3].default_frontmatter.as_ref().unwrap()["mode"],
            "subagent"
        );
        assert_eq!(
            agents[5].default_frontmatter.as_ref().unwrap()["hidden"],
            true
        );
    }

    #[test]
    fn merges_overrides_and_resets_only_selected_agent() {
        let dir = tempfile::tempdir().unwrap();
        let agents_dir = dir.path().join("agents");
        std::fs::create_dir_all(&agents_dir).unwrap();
        let json_path = dir.path().join("opencode.json");
        let jsonc_path = dir.path().join("opencode.jsonc");
        std::fs::write(&json_path, r#"{"provider":{"test":{}},"agent":{"plan":{"model":"test/model","permission":{"bash":"ask"}},"custom":{"mode":"all"}}}"#).unwrap();
        std::fs::write(&jsonc_path, "{ // settings\n \"agent\": {\"plan\": {\"disable\":true,\"permission\":{\"edit\":\"deny\"}}},\n}").unwrap();
        std::fs::write(
            agents_dir.join("plan.md"),
            "---\ntemperature: 0.2\ncustom_option: keep\n---\nCustom plan prompt.\n",
        )
        .unwrap();
        std::fs::write(agents_dir.join("custom.md"), "Custom agent").unwrap();
        let mut plan = list(&agents_dir, "global").unwrap().remove(1);
        assert_eq!(plan.frontmatter["model"], "test/model");
        assert_eq!(plan.frontmatter["disable"], true);
        assert_eq!(
            plan.frontmatter["permission"],
            json!({"bash":"ask","edit":"deny"})
        );
        assert_eq!(plan.prompt, "Custom plan prompt.");
        plan.frontmatter["model"] = json!("test/new-model");
        write(&agents_dir, "global", "plan", Some(&plan)).unwrap();
        let saved = list(&agents_dir, "global").unwrap().remove(1);
        assert_eq!(saved.frontmatter, plan.frontmatter);
        assert_eq!(saved.prompt, plan.prompt);
        assert!(agents_dir.join("plan.md").exists());
        assert_eq!(
            read_config(&json_path).unwrap()["agent"]["plan"]["model"],
            "test/new-model"
        );
        write(&agents_dir, "global", "plan", None).unwrap();
        let reset = list(&agents_dir, "global").unwrap().remove(1);
        assert_eq!(reset.frontmatter, json!({}));
        assert!(reset.prompt.is_empty());
        assert!(reset.file_path.is_empty());
        assert_eq!(
            read_config(&json_path).unwrap()["provider"],
            json!({"test":{}})
        );
        assert_eq!(
            read_config(&json_path).unwrap()["agent"]["custom"],
            json!({"mode":"all"})
        );
        assert_eq!(
            std::fs::read_to_string(agents_dir.join("custom.md")).unwrap(),
            "Custom agent"
        );
        write(&agents_dir, "global", "plan", None).unwrap();
    }

    #[test]
    fn model_only_save_does_not_replace_native_prompt_or_permissions() {
        let dir = tempfile::tempdir().unwrap();
        let agents_dir = dir.path().join("agents");
        let mut explore = list(&agents_dir, "global").unwrap().remove(3);
        explore.frontmatter = json!({"model": "test/model"});
        write(&agents_dir, "global", "explore", Some(&explore)).unwrap();
        let config = read_config(&dir.path().join("opencode.json")).unwrap();
        assert_eq!(config["agent"]["explore"], json!({"model":"test/model"}));
    }

    #[test]
    fn project_reset_removes_root_and_dot_opencode_overrides() {
        let dir = tempfile::tempdir().unwrap();
        let project = dir.path().join("project");
        let agents_dir = project.join(".opencode/agents");
        std::fs::create_dir_all(project.join(".opencode/agent")).unwrap();
        let global_path = dir.path().join("opencode.json");
        let raw = r#"{"agent":{"build":{"model":"test/model"}}}"#;
        std::fs::write(&global_path, raw).unwrap();
        std::fs::write(project.join("opencode.json"), raw).unwrap();
        std::fs::write(project.join(".opencode/opencode.jsonc"), raw).unwrap();
        std::fs::write(
            project.join(".opencode/agent/build.md"),
            "---\nhidden: true\n---\n",
        )
        .unwrap();
        write(&agents_dir, "project", "build", None).unwrap();
        assert_eq!(
            list(&agents_dir, "project").unwrap()[0].frontmatter,
            json!({})
        );
        assert_eq!(std::fs::read_to_string(global_path).unwrap(), raw);
        assert!(!project.join(".opencode/agent/build.md").exists());
    }

    #[test]
    fn invalid_config_aborts_reset_before_mutating_any_file() {
        let dir = tempfile::tempdir().unwrap();
        let config = dir.path().join("opencode.json");
        let raw = r#"{"agent":{"plan":{"disable":true}}}"#;
        std::fs::write(&config, raw).unwrap();
        std::fs::write(dir.path().join("opencode.jsonc"), "invalid").unwrap();
        assert!(write(&dir.path().join("agents"), "global", "plan", None).is_err());
        assert_eq!(std::fs::read_to_string(config).unwrap(), raw);
        assert!(write(&dir.path().join("agents"), "global", "custom", None).is_err());
    }
    #[test]
    fn saves_model_without_relocating_relative_json_references() {
        let dir = tempfile::tempdir().unwrap();
        let agents_dir = dir.path().join(".opencode/agents");
        std::fs::create_dir_all(agents_dir.parent().unwrap()).unwrap();
        let root = dir.path().join("opencode.json");
        let nested = dir.path().join(".opencode/opencode.jsonc");
        let raw = r#"{"agent":{"plan":{"prompt":"{file:./prompts/plan.txt}","options":{"custom":"{file:./custom.txt}"}}}}"#;
        std::fs::write(&root, raw).unwrap();
        std::fs::write(&nested, r#"{"agent":{"plan":{"model":"old/model"}}}"#).unwrap();
        let mut plan = list(&agents_dir, "project").unwrap().remove(1);
        plan.frontmatter["model"] = json!("new/model");
        write(&agents_dir, "project", "plan", Some(&plan)).unwrap();
        assert_eq!(std::fs::read_to_string(&root).unwrap(), raw);
        assert_eq!(
            read_config(&nested).unwrap()["agent"]["plan"],
            json!({"model":"new/model"})
        );
        plan.frontmatter["options"]["new_option"] = json!(true);
        write(&agents_dir, "project", "plan", Some(&plan)).unwrap();
        assert_eq!(std::fs::read_to_string(&root).unwrap(), raw);
        assert_eq!(
            list(&agents_dir, "project").unwrap()[1].frontmatter,
            plan.frontmatter
        );
    }

    #[test]
    fn preserves_markdown_literal_templates_when_editing_model() {
        let dir = tempfile::tempdir().unwrap();
        let agents_dir = dir.path().join("agents");
        std::fs::create_dir_all(&agents_dir).unwrap();
        let path = agents_dir.join("plan.md");
        std::fs::write(&path, "---\nmodel: old/model\ncustom: '{env:EXAMPLE}'\n---\nUse literal {file:./example.txt} and {env:EXAMPLE}.\n").unwrap();
        let mut plan = list(&agents_dir, "global").unwrap().remove(1);
        let prompt = plan.prompt.clone();
        plan.frontmatter["model"] = json!("new/model");
        write(&agents_dir, "global", "plan", Some(&plan)).unwrap();
        assert!(!dir.path().join("opencode.json").exists());
        assert!(path.exists());
        let saved = list(&agents_dir, "global").unwrap().remove(1);
        assert_eq!(saved.prompt, prompt);
        assert_eq!(saved.frontmatter["custom"], "{env:EXAMPLE}");
        assert_eq!(saved.frontmatter["model"], "new/model");
    }

    #[test]
    fn preserves_explicit_empty_json_and_markdown_prompts() {
        for markdown in [false, true] {
            let dir = tempfile::tempdir().unwrap();
            let agents_dir = dir.path().join("agents");
            std::fs::create_dir_all(&agents_dir).unwrap();
            let json_path = dir.path().join("opencode.json");
            std::fs::write(
                &json_path,
                if markdown {
                    r#"{"agent":{"plan":{"prompt":"Lower priority prompt"}}}"#
                } else {
                    r#"{"agent":{"plan":{"prompt":""}}}"#
                },
            )
            .unwrap();
            if markdown {
                std::fs::write(agents_dir.join("plan.md"), "---\nmode: primary\n---\n   \n")
                    .unwrap();
            }
            let mut plan = list(&agents_dir, "global").unwrap().remove(1);
            assert!(plan.has_prompt_override);
            assert_eq!(plan.prompt, "");
            plan.frontmatter["model"] = json!("test/model");
            write(&agents_dir, "global", "plan", Some(&plan)).unwrap();
            let saved = list(&agents_dir, "global").unwrap().remove(1);
            assert!(saved.has_prompt_override);
            assert_eq!(saved.prompt, "");
            assert_eq!(
                read_config(&json_path).unwrap()["agent"]["plan"]["prompt"],
                if markdown {
                    "Lower priority prompt"
                } else {
                    ""
                }
            );
        }
    }

    #[test]
    fn normalizes_scalar_permissions_before_merging_and_editing() {
        let dir = tempfile::tempdir().unwrap();
        let agents_dir = dir.path().join("agents");
        let lower = dir.path().join("opencode.json");
        let upper = dir.path().join("opencode.jsonc");
        let raw = r#"{"agent":{"build":{"permission":"deny"}}}"#;
        std::fs::write(&lower, raw).unwrap();
        std::fs::write(
            &upper,
            r#"{"agent":{"build":{"permission":{"read":"allow"}}}}"#,
        )
        .unwrap();
        let mut build = list(&agents_dir, "global").unwrap().remove(0);
        assert_eq!(
            build.frontmatter["permission"],
            json!({"*":"deny","read":"allow"})
        );
        build.frontmatter["permission"]["read"] = json!("ask");
        write(&agents_dir, "global", "build", Some(&build)).unwrap();
        assert_eq!(std::fs::read_to_string(&lower).unwrap(), raw);
        assert_eq!(
            list(&agents_dir, "global").unwrap()[0].frontmatter["permission"],
            json!({"*":"deny","read":"ask"})
        );
        build.frontmatter["permission"]
            .as_object_mut()
            .unwrap()
            .remove("*");
        write(&agents_dir, "global", "build", Some(&build)).unwrap();
        assert_eq!(
            list(&agents_dir, "global").unwrap()[0].frontmatter["permission"],
            json!({"read":"ask"})
        );
    }

    #[test]
    fn removing_nested_overrides_clears_all_sources_without_moving_other_values() {
        let dir = tempfile::tempdir().unwrap();
        let agents_dir = dir.path().join("agents");
        let lower = dir.path().join("opencode.json");
        let upper = dir.path().join("opencode.jsonc");
        std::fs::write(&lower, r#"{"agent":{"build":{"permission":{"read":"deny","*":"ask"},"options":{"keep":"{file:./keep.txt}","remove":"old"}}}}"#).unwrap();
        std::fs::write(
            &upper,
            r#"{"agent":{"build":{"permission":{"read":"allow"},"options":{"remove":"new"}}}}"#,
        )
        .unwrap();
        let mut build = list(&agents_dir, "global").unwrap().remove(0);
        build.frontmatter["permission"]
            .as_object_mut()
            .unwrap()
            .remove("read");
        build.frontmatter["options"]
            .as_object_mut()
            .unwrap()
            .remove("remove");
        write(&agents_dir, "global", "build", Some(&build)).unwrap();
        assert_eq!(
            list(&agents_dir, "global").unwrap()[0].frontmatter,
            build.frontmatter
        );
        assert_eq!(
            read_config(&lower).unwrap()["agent"]["build"]["options"],
            json!({"keep":"{file:./keep.txt}"})
        );
        assert!(read_config(&upper).unwrap()["agent"]["build"]["options"]
            .get("remove")
            .is_none());
    }

    #[test]
    fn deleting_permission_keeps_remaining_rule_precedence() {
        let dir = tempfile::tempdir().unwrap();
        let agents_dir = dir.path().join("agents");
        let config = dir.path().join("opencode.json");
        std::fs::write(
            &config,
            r#"{"agent":{"build":{"permission":{"read":"allow","*":"ask","bash":"deny"}}}}"#,
        )
        .unwrap();
        let mut build = list(&agents_dir, "global").unwrap().remove(0);
        build.frontmatter["permission"]
            .as_object_mut()
            .unwrap()
            .shift_remove("read");
        write(&agents_dir, "global", "build", Some(&build)).unwrap();
        let config = read_config(&config).unwrap();
        let keys = config["agent"]["build"]["permission"]
            .as_object()
            .unwrap()
            .keys()
            .map(String::as_str)
            .collect::<Vec<_>>();
        assert_eq!(keys, vec!["*", "bash"]);
    }
    #[test]
    fn new_mcp_wildcard_precedes_specific_exception_in_same_or_earlier_source() {
        for extra_source in [false, true] {
            let dir = tempfile::tempdir().unwrap();
            let agents_dir = dir.path().join("agents");
            let lower = dir.path().join("opencode.json");
            std::fs::write(
                &lower,
                r#"{"agent":{"build":{"permission":{"github_search":"deny"}}}}"#,
            )
            .unwrap();
            let upper = dir.path().join("opencode.jsonc");
            let upper_raw = r#"{"agent":{"build":{"model":"test/model"}}}"#;
            if extra_source {
                std::fs::write(&upper, upper_raw).unwrap();
            }
            let mut build = list(&agents_dir, "global").unwrap().remove(0);
            build.frontmatter["permission"] = json!({"github_*":"allow","github_search":"deny"});
            write(&agents_dir, "global", "build", Some(&build)).unwrap();
            let saved = list(&agents_dir, "global").unwrap().remove(0);
            assert!(ordered_equal(
                &saved.frontmatter["permission"],
                &build.frontmatter["permission"]
            ));
            assert!(ordered_equal(
                &read_config(&lower).unwrap()["agent"]["build"]["permission"],
                &build.frontmatter["permission"]
            ));
            if extra_source {
                assert_eq!(std::fs::read_to_string(&upper).unwrap(), upper_raw);
            }
        }
    }

    #[test]
    fn nested_permission_patterns_preserve_requested_order() {
        let dir = tempfile::tempdir().unwrap();
        let agents_dir = dir.path().join("agents");
        let config = dir.path().join("opencode.json");
        std::fs::write(
            &config,
            r#"{"agent":{"build":{"permission":{"bash":{"git push*":"deny"}}}}}"#,
        )
        .unwrap();
        let mut build = list(&agents_dir, "global").unwrap().remove(0);
        build.frontmatter["permission"]["bash"] = json!({"*":"allow","git push*":"deny"});
        write(&agents_dir, "global", "build", Some(&build)).unwrap();
        let saved = list(&agents_dir, "global").unwrap().remove(0);
        assert!(ordered_equal(
            &saved.frontmatter["permission"],
            &build.frontmatter["permission"]
        ));
        // A reorder without a value change must also be persisted.
        build.frontmatter["permission"]["bash"] = json!({"git push*":"deny","*":"allow"});
        write(&agents_dir, "global", "build", Some(&build)).unwrap();
        let saved = list(&agents_dir, "global").unwrap().remove(0);
        assert!(ordered_equal(
            &saved.frontmatter["permission"],
            &build.frontmatter["permission"]
        ));
    }

    #[test]
    fn impossible_cross_source_reorder_fails_before_writing() {
        let dir = tempfile::tempdir().unwrap();
        let agents_dir = dir.path().join("agents");
        let lower = dir.path().join("opencode.json");
        let upper = dir.path().join("opencode.jsonc");
        let lower_raw = r#"{"agent":{"build":{"permission":{"github_search":"deny"}}}}"#;
        let upper_raw = r#"{"agent":{"build":{"permission":{"github_*":"allow"}}}}"#;
        std::fs::write(&lower, lower_raw).unwrap();
        std::fs::write(&upper, upper_raw).unwrap();
        let mut build = list(&agents_dir, "global").unwrap().remove(0);
        build.frontmatter["permission"] = json!({"github_*":"allow","github_search":"deny"});
        let error = write(&agents_dir, "global", "build", Some(&build)).unwrap_err();
        assert!(error.to_string().contains("permission rule order"));
        assert_eq!(std::fs::read_to_string(&lower).unwrap(), lower_raw);
        assert_eq!(std::fs::read_to_string(&upper).unwrap(), upper_raw);
    }
}
