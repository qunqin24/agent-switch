//! Native built-in agent identities and overrides for OpenCode V2.
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
        "title" => json!({"mode": "primary", "hidden": true}),
        _ => json!({"mode": "primary"}),
    })
}

pub(super) fn config_paths(agents_dir: &Path, scope: &str) -> Vec<PathBuf> {
    let directory = agents_dir.parent().expect("agents directory has a parent");
    let mut paths = Vec::new();
    if scope != "global" {
        if let Some(project) = directory.parent() {
            paths.extend([
                project.join("opencode.json"),
                project.join("opencode.jsonc"),
            ]);
        }
    }
    paths.extend([
        directory.join("opencode.json"),
        directory.join("opencode.jsonc"),
    ]);
    paths
}

fn markdown_paths(agents_dir: &Path, id: &str) -> [PathBuf; 1] {
    [agents_dir.join(format!("{id}.md"))]
}

pub(super) fn read_config(path: &Path) -> Result<Value, AppError> {
    let raw = std::fs::read_to_string(path).map_err(|error| AppError::io(path, error))?;
    let config: Value = json5::from_str(&raw).map_err(|error| {
        AppError::Config(format!(
            "Invalid OpenCode config {}: {error}",
            path.display()
        ))
    })?;
    crate::opencode_config::validate_v2_config(&config)?;
    if config
        .get("agents")
        .is_some_and(|agents| !agents.is_object())
    {
        return Err(AppError::Config(format!(
            "OpenCode config and agent fields must be objects: {}",
            path.display()
        )));
    }
    Ok(config)
}

pub(super) fn agent_from_v2(mut fields: Value) -> Value {
    let Some(object) = fields.as_object_mut() else {
        return fields;
    };
    if let Some(system) = object.remove("system") {
        object.insert("prompt".into(), system);
    }
    if let Some(disabled) = object.remove("disabled") {
        object.insert("disable".into(), disabled);
    }
    if let Some(rules) = object.get("permissions").and_then(Value::as_array) {
        let mut permission = serde_json::Map::new();
        let mut simple = true;
        for rule in rules {
            let action = rule.get("action").and_then(Value::as_str);
            let resource = rule.get("resource").and_then(Value::as_str);
            let effect = rule.get("effect").and_then(Value::as_str);
            if let (Some(action), Some("*"), Some(effect)) = (action, resource, effect) {
                if action.contains('*') {
                    simple = false;
                    break;
                }
                let action = match action {
                    "shell" => "bash",
                    "subagent" => "task",
                    _ => action,
                };
                if permission.insert(action.into(), json!(effect)).is_some() {
                    simple = false;
                    break;
                }
            } else {
                simple = false;
                break;
            }
        }
        if simple {
            object.remove("permissions");
            if !permission.is_empty() {
                object.insert("permission".into(), Value::Object(permission));
            }
        }
    }
    if let Some(model) = object.get("model").and_then(Value::as_object) {
        if let (Some(provider), Some(model_id)) = (
            model.get("providerID").and_then(Value::as_str),
            model.get("model").and_then(Value::as_str),
        ) {
            let variant = model
                .get("variant")
                .and_then(Value::as_str)
                .map(|variant| format!("#{variant}"))
                .unwrap_or_default();
            object.insert(
                "model".into(),
                json!(format!("{provider}/{model_id}{variant}")),
            );
        }
    }
    if let Some(model) = object
        .get("model")
        .and_then(Value::as_str)
        .map(str::to_owned)
    {
        if let Some((model, variant)) = model.rsplit_once('#') {
            object.insert("model".into(), json!(model));
            object.insert("variant".into(), json!(variant));
        }
    }
    fields
}

pub(super) fn agent_to_v2(mut fields: Value) -> Value {
    let Some(object) = fields.as_object_mut() else {
        return fields;
    };
    if let Some(prompt) = object.remove("prompt") {
        object.insert("system".into(), prompt);
    }
    if let Some(disable) = object.remove("disable") {
        object.insert("disabled".into(), disable);
    }
    if let Some(steps) = object.remove("maxSteps") {
        object.insert("steps".into(), steps);
    }
    if let Some(variant) = object
        .remove("variant")
        .and_then(|value| value.as_str().map(str::to_owned))
    {
        if let Some(model) = object
            .get("model")
            .and_then(Value::as_str)
            .map(str::to_owned)
        {
            object.insert("model".into(), json!(format!("{model}#{variant}")));
        }
    }
    if let Some(options) = object
        .remove("options")
        .and_then(|value| value.as_object().cloned())
    {
        let request = object.entry("request").or_insert_with(|| json!({}));
        let body = request
            .as_object_mut()
            .map(|request| request.entry("body").or_insert_with(|| json!({})));
        if let Some(body) = body.and_then(Value::as_object_mut) {
            body.extend(options);
        }
    }
    for key in ["temperature", "top_p"] {
        if let Some(value) = object.remove(key) {
            let request = object.entry("request").or_insert_with(|| json!({}));
            if request.get("body").is_none() {
                request["body"] = json!({});
            }
            request["body"][key] = value;
        }
    }
    if let Some(permission) = object
        .remove("permission")
        .and_then(|value| value.as_object().cloned())
    {
        let rules = object.entry("permissions").or_insert_with(|| json!([]));
        if let Some(rules) = rules.as_array_mut() {
            for (action, effect) in permission {
                let action = match action.as_str() {
                    "bash" => "shell",
                    "task" => "subagent",
                    "write" | "patch" => "edit",
                    _ => &action,
                };
                if let Some(effect) = effect.as_str() {
                    rules.push(json!({"action": action, "resource": "*", "effect": effect}));
                } else if let Some(resources) = effect.as_object() {
                    for (resource, effect) in resources {
                        if let Some(effect) = effect.as_str() {
                            rules.push(
                                json!({"action": action, "resource": resource, "effect": effect}),
                            );
                        }
                    }
                }
            }
        }
    }
    fields
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
                let native = config.get("agents").and_then(|agents| agents.get(id));
                if let Some(value) = native {
                    if !value.is_object() {
                        return Err(AppError::Config(format!("Agent '{id}' must be an object")));
                    }
                    let mut value = agent_from_v2(value.clone());
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
                fields = agent_from_v2(fields);
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
                native_v2: true,
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
    config_key: Option<&'static str>,
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
        let key = "agents";
        let mut fields = config
            .get(key)
            .and_then(|agents| agents.get(id))
            .cloned()
            .unwrap_or_else(|| json!({}));
        if !fields.is_object() {
            return Err(AppError::Config(format!("Agent '{id}' must be an object")));
        }
        fields = agent_from_v2(fields);
        normalize_permissions(&mut fields);
        sources.push(AgentSource {
            path,
            config: Some(config),
            config_key: Some(key),
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
        fields = agent_from_v2(fields);
        fields["prompt"] = json!(prompt.trim());
        normalize_permissions(&mut fields);
        sources.push(AgentSource {
            path,
            config: None,
            config_key: None,
            before: fields.clone(),
            fields,
        });
    }
    if let Some(document) = document {
        if sources.is_empty() {
            sources.push(AgentSource {
                path: agents_dir.parent().unwrap().join("opencode.json"),
                config: Some(json!({})),
                config_key: Some("agents"),
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
            let key = source.config_key.unwrap_or("agents");
            if document.is_none() {
                for key in ["agents"] {
                    if let Some(agents) = config.get_mut(key).and_then(Value::as_object_mut) {
                        agents.shift_remove(id);
                    }
                }
            } else {
                if config.get(key).is_none() {
                    config[key] = json!({});
                }
                config[key][id] = agent_to_v2(source.fields);
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
            let fields = agent_to_v2(fields);
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
    fn keeps_ordered_native_permission_exceptions() {
        let native = json!({"permissions": [
            {"action":"shell", "resource":"*", "effect":"deny"},
            {"action":"shell", "resource":"git status", "effect":"allow"}
        ]});
        let normalized = agent_from_v2(native.clone());
        assert!(normalized.get("permission").is_none());
        assert_eq!(agent_to_v2(normalized), native);
    }

    #[test]
    fn edits_native_v2_builtin_markdown_in_place() {
        let dir = tempfile::tempdir().unwrap();
        let agents_dir = dir.path().join("agents");
        std::fs::create_dir_all(&agents_dir).unwrap();
        let path = agents_dir.join("build.md");
        std::fs::write(&path, "---\nmodel: openai/coding#high\npermissions:\n  - action: shell\n    resource: \"*\"\n    effect: ask\n---\n\nBuild carefully.\n").unwrap();
        let mut build = list(&agents_dir, "global")
            .unwrap()
            .into_iter()
            .find(|agent| agent.id == "build")
            .unwrap();
        assert!(build.native_v2);
        assert_eq!(build.frontmatter["permission"]["bash"], "ask");
        build.frontmatter["variant"] = json!("low");
        write(&agents_dir, "global", "build", Some(&build)).unwrap();
        let (fields, _) = split_agent_markdown(&std::fs::read_to_string(path).unwrap()).unwrap();
        assert_eq!(fields["model"], "openai/coding#low");
        assert_eq!(fields["permissions"][0]["action"], "shell");
        assert!(fields.get("permission").is_none());
    }

    #[test]
    fn edits_native_v2_builtin_without_legacy_agent_fields() {
        let dir = tempfile::tempdir().unwrap();
        let agents_dir = dir.path().join("agents");
        let config = dir.path().join("opencode.json");
        std::fs::write(&config, r#"{"agents":{"build":{"system":"Old prompt","model":"openai/coding#high","permissions":[{"action":"shell","resource":"git push *","effect":"ask"}]}}}"#).unwrap();

        let build = list(&agents_dir, "global")
            .unwrap()
            .into_iter()
            .find(|agent| agent.id == "build")
            .unwrap();
        assert_eq!(build.prompt, "Old prompt");
        assert_eq!(build.frontmatter["model"], "openai/coding");
        assert_eq!(build.frontmatter["variant"], "high");
        let mut edited = build.clone();
        edited.prompt = "New prompt".into();
        edited.frontmatter["description"] = json!("Review changes");
        write(&agents_dir, "global", "build", Some(&edited)).unwrap();
        let saved = read_config(&config).unwrap();
        assert_eq!(saved["agents"]["build"]["system"], "New prompt");
        assert_eq!(saved["agents"]["build"]["model"], "openai/coding#high");
        assert_eq!(
            saved["agents"]["build"]["permissions"][0]["resource"],
            "git push *"
        );
        assert!(saved.get("agent").is_none());
    }

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
    fn model_only_save_does_not_replace_native_prompt_or_permissions() {
        let dir = tempfile::tempdir().unwrap();
        let agents_dir = dir.path().join("agents");
        let mut explore = list(&agents_dir, "global").unwrap().remove(3);
        explore.frontmatter = json!({"model": "test/model"});
        write(&agents_dir, "global", "explore", Some(&explore)).unwrap();
        let config = read_config(&dir.path().join("opencode.json")).unwrap();
        assert_eq!(config["agents"]["explore"], json!({"model":"test/model"}));
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
}
