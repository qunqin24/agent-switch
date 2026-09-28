mod builtin;
mod custom_config;

use crate::config::atomic_write;
use crate::error::AppError;
use crate::opencode_config::get_opencode_dir;
use crate::services::omo::OmoService;
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct OpenCodeAgentDocument {
    pub id: String,
    pub scope: String,
    pub file_path: String,
    pub frontmatter: Value,
    pub prompt: String,
    #[serde(default)]
    pub has_prompt_override: bool,
    pub last_modified: Option<i64>,
    pub managed_by: Option<String>,
    #[serde(default)]
    pub built_in: bool,
    #[serde(default)]
    pub native_v2: bool,
    #[serde(default)]
    pub default_frontmatter: Option<Value>,
}

const OMO_SLIM_SOURCE: &str = "omo-slim";

fn validate_agent_id(id: &str) -> Result<(), AppError> {
    if id.is_empty()
        || id.len() > 80
        || id.split('/').any(|segment| {
            segment.is_empty()
                || !segment
                    .chars()
                    .all(|ch| ch.is_ascii_alphanumeric() || ch == '-' || ch == '_')
        })
    {
        return Err(AppError::InvalidInput(
            "Agent ID must contain path segments with only letters, numbers, hyphens, or underscores".to_string(),
        ));
    }
    Ok(())
}

fn agent_markdown_path(agents_dir: &Path, id: &str) -> Result<PathBuf, AppError> {
    validate_agent_id(id)?;
    let mut parent = agents_dir.to_path_buf();
    let mut segments = id.split('/').peekable();
    while let Some(segment) = segments.next() {
        if segments.peek().is_none() {
            break;
        }
        parent.push(segment);
        match std::fs::symlink_metadata(&parent) {
            Ok(metadata) if !metadata.is_dir() || metadata.file_type().is_symlink() => {
                return Err(AppError::InvalidInput(format!(
                    "Agent path contains an unsafe directory: {}",
                    parent.display()
                )));
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => return Err(AppError::io(&parent, error)),
            Ok(_) => {}
        }
    }
    Ok(agents_dir.join(format!("{id}.md")))
}

fn resolve_agents_dir(scope: &str, project_dir: Option<&str>) -> Result<PathBuf, AppError> {
    match scope {
        "global" => Ok(get_opencode_dir().join("agents")),
        "project" => {
            let project_dir = project_dir
                .filter(|value| !value.trim().is_empty())
                .ok_or_else(|| AppError::InvalidInput("Project directory is required".into()))?;
            let project_dir = PathBuf::from(project_dir);
            if !project_dir.is_absolute() || !project_dir.is_dir() {
                return Err(AppError::InvalidInput(
                    "Project directory must be an existing absolute directory".into(),
                ));
            }
            Ok(project_dir.join(".opencode").join("agents"))
        }
        _ => Err(AppError::InvalidInput(format!(
            "Unsupported Agent scope: {scope}"
        ))),
    }
}

fn split_agent_markdown(raw: &str) -> Result<(Value, String), AppError> {
    let mut offset = 0usize;
    let mut lines = raw.split_inclusive('\n');
    let Some(first) = lines.next() else {
        return Ok((Value::Object(Map::new()), String::new()));
    };
    if first.trim_end_matches(['\r', '\n']) != "---" {
        return Ok((Value::Object(Map::new()), raw.to_string()));
    }
    offset += first.len();
    let yaml_start = offset;
    let mut yaml_end = None;
    let mut body_start = raw.len();

    for line in lines {
        if line.trim_end_matches(['\r', '\n']) == "---" {
            yaml_end = Some(offset);
            body_start = offset + line.len();
            break;
        }
        offset += line.len();
    }

    let Some(yaml_end) = yaml_end else {
        return Err(AppError::Config(
            "Agent Markdown frontmatter is missing its closing delimiter".into(),
        ));
    };
    let yaml = &raw[yaml_start..yaml_end];
    let frontmatter = if yaml.trim().is_empty() {
        Value::Object(Map::new())
    } else {
        serde_yaml::from_str::<Value>(yaml).map_err(|error| {
            AppError::Config(format!("Failed to parse Agent frontmatter: {error}"))
        })?
    };
    if !frontmatter.is_object() {
        return Err(AppError::Config(
            "Agent frontmatter must be a YAML object".into(),
        ));
    }

    let prompt = raw[body_start..]
        .trim_start_matches(['\r', '\n'])
        .to_string();
    Ok((frontmatter, prompt))
}

fn serialize_agent_markdown(frontmatter: &Value, prompt: &str) -> Result<String, AppError> {
    if !frontmatter.is_object() {
        return Err(AppError::InvalidInput(
            "Agent frontmatter must be an object".into(),
        ));
    }
    let yaml = serde_yaml::to_string(frontmatter)
        .map_err(|error| AppError::Config(format!("Failed to serialize Agent: {error}")))?;
    let yaml = yaml
        .strip_prefix("---\n")
        .unwrap_or(&yaml)
        .trim_end_matches(['\r', '\n']);
    let prompt = prompt.trim_start_matches(['\r', '\n']);
    let mut output = format!("---\n{yaml}\n---\n\n{prompt}");
    if !output.ends_with('\n') {
        output.push('\n');
    }
    Ok(output)
}

fn file_modified_millis(path: &Path) -> Option<i64> {
    let modified = std::fs::metadata(path).ok()?.modified().ok()?;
    let millis = modified
        .duration_since(std::time::UNIX_EPOCH)
        .ok()?
        .as_millis();
    i64::try_from(millis).ok()
}

fn list_agents_in_dir(
    agents_dir: &Path,
    scope: &str,
) -> Result<Vec<OpenCodeAgentDocument>, AppError> {
    if !agents_dir.exists() {
        return Ok(Vec::new());
    }
    let mut paths = Vec::new();
    let mut directories = vec![agents_dir.to_path_buf()];
    while let Some(directory) = directories.pop() {
        for entry in
            std::fs::read_dir(&directory).map_err(|error| AppError::io(&directory, error))?
        {
            let entry = entry.map_err(|error| AppError::io(&directory, error))?;
            let file_type = entry
                .file_type()
                .map_err(|error| AppError::io(entry.path(), error))?;
            let path = entry.path();
            if file_type.is_dir() {
                directories.push(path);
            } else if file_type.is_file()
                && path.extension().and_then(|value| value.to_str()) == Some("md")
            {
                paths.push(path);
            }
        }
    }
    paths.sort();

    paths
        .into_iter()
        .map(|path| {
            let id = path
                .strip_prefix(agents_dir)
                .ok()
                .and_then(|relative| relative.with_extension("").to_str().map(str::to_owned))
                .ok_or_else(|| AppError::Config("Agent path is not valid UTF-8".into()))?
                .replace(std::path::MAIN_SEPARATOR, "/");
            validate_agent_id(&id)?;
            let raw = std::fs::read_to_string(&path).map_err(|error| AppError::io(&path, error))?;
            let (frontmatter, prompt) = split_agent_markdown(&raw)?;
            Ok(OpenCodeAgentDocument {
                id,
                scope: scope.to_string(),
                file_path: path.to_string_lossy().to_string(),
                frontmatter: builtin::agent_from_v2(frontmatter),
                prompt,
                has_prompt_override: true,
                last_modified: file_modified_millis(&path),
                managed_by: None,
                built_in: false,
                native_v2: true,
                default_frontmatter: None,
            })
        })
        .collect()
}

fn mark_managed_agents(
    agents: &mut [OpenCodeAgentDocument],
    scope: &str,
    managed_ids: &std::collections::HashSet<String>,
) {
    if scope != "global" {
        return;
    }
    for agent in agents {
        if managed_ids.contains(&agent.id) {
            agent.managed_by = Some(OMO_SLIM_SOURCE.to_string());
        }
    }
}

fn ensure_agent_mutable(
    scope: &str,
    id: &str,
    managed_ids: &std::collections::HashSet<String>,
) -> Result<(), AppError> {
    if scope == "global" && managed_ids.contains(id) {
        return Err(AppError::InvalidInput(format!(
            "Agent '{id}' is managed by OMO Slim and cannot be changed here"
        )));
    }
    Ok(())
}

fn save_agent_in_dir(
    agents_dir: &Path,
    scope: &str,
    agent: OpenCodeAgentDocument,
    original_id: Option<&str>,
) -> Result<OpenCodeAgentDocument, AppError> {
    validate_agent_id(&agent.id)?;
    if let Some(original_id) = original_id {
        validate_agent_id(original_id)?;
    }
    if builtin::defaults(&agent.id).is_some()
        || original_id.is_some_and(|id| builtin::defaults(id).is_some())
    {
        return Err(AppError::InvalidInput(
            "Built-in Agent IDs cannot be created or renamed".into(),
        ));
    }
    std::fs::create_dir_all(agents_dir).map_err(|error| AppError::io(agents_dir, error))?;

    let target_path = agent_markdown_path(agents_dir, &agent.id)?;
    let source_path = original_id
        .map(|id| agent_markdown_path(agents_dir, id))
        .transpose()?;
    let is_rename = source_path
        .as_ref()
        .is_some_and(|source| source != &target_path);
    let is_create = source_path.is_none();
    if (is_create || is_rename) && target_path.exists() {
        return Err(AppError::InvalidInput(format!(
            "Agent '{}' already exists",
            agent.id
        )));
    }

    let fields = builtin::agent_to_v2(agent.frontmatter.clone());
    let contents = serialize_agent_markdown(&fields, &agent.prompt)?;
    if let Some(parent) = target_path.parent() {
        std::fs::create_dir_all(parent).map_err(|error| AppError::io(parent, error))?;
    }
    atomic_write(&target_path, contents.as_bytes())?;

    if let Some(source_path) = source_path.filter(|source| source != &target_path) {
        if source_path.exists() {
            if let Err(error) = std::fs::remove_file(&source_path) {
                let _ = std::fs::remove_file(&target_path);
                return Err(AppError::io(source_path, error));
            }
        }
    }

    let (frontmatter, prompt) = split_agent_markdown(&contents)?;
    Ok(OpenCodeAgentDocument {
        id: agent.id,
        scope: scope.to_string(),
        file_path: target_path.to_string_lossy().to_string(),
        frontmatter: builtin::agent_from_v2(frontmatter),
        prompt,
        has_prompt_override: true,
        last_modified: file_modified_millis(&target_path),
        managed_by: None,
        built_in: false,
        native_v2: true,
        default_frontmatter: None,
    })
}

fn delete_agent_in_dir(agents_dir: &Path, id: &str) -> Result<(), AppError> {
    let path = agent_markdown_path(agents_dir, id)?;
    if path.exists() {
        std::fs::remove_file(&path).map_err(|error| AppError::io(path, error))?;
    }
    Ok(())
}

fn sorted_mcp_server_ids(mcp_servers: Map<String, Value>) -> Vec<String> {
    let mut ids = mcp_servers
        .into_iter()
        .map(|(id, _)| id)
        .collect::<Vec<_>>();
    ids.sort_by_key(|id| id.to_lowercase());
    ids
}

#[tauri::command]
pub async fn list_opencode_agents(
    scope: String,
    #[allow(non_snake_case)] projectDir: Option<String>,
) -> Result<Vec<OpenCodeAgentDocument>, String> {
    let agents_dir = resolve_agents_dir(&scope, projectDir.as_deref()).map_err(String::from)?;
    let mut agents = builtin::list(&agents_dir, &scope).map_err(String::from)?;
    let markdown = list_agents_in_dir(&agents_dir, &scope).map_err(String::from)?;
    let markdown_ids: std::collections::HashSet<_> =
        markdown.iter().map(|agent| agent.id.as_str()).collect();
    agents.extend(
        custom_config::list(&agents_dir, &scope)
            .map_err(String::from)?
            .into_iter()
            .filter(|agent| !markdown_ids.contains(agent.id.as_str())),
    );
    agents.extend(
        markdown
            .into_iter()
            .filter(|agent| builtin::defaults(&agent.id).is_none()),
    );
    let managed_ids = OmoService::slim_managed_agent_ids();
    mark_managed_agents(&mut agents, &scope, &managed_ids);
    Ok(agents)
}

#[tauri::command]
pub async fn list_opencode_mcp_server_ids() -> Result<Vec<String>, String> {
    crate::opencode_config::get_mcp_servers()
        .map(sorted_mcp_server_ids)
        .map_err(String::from)
}

#[tauri::command]
pub async fn save_opencode_agent(
    scope: String,
    #[allow(non_snake_case)] projectDir: Option<String>,
    agent: OpenCodeAgentDocument,
    #[allow(non_snake_case)] originalId: Option<String>,
) -> Result<OpenCodeAgentDocument, String> {
    let agents_dir = resolve_agents_dir(&scope, projectDir.as_deref()).map_err(String::from)?;
    agent_markdown_path(&agents_dir, &agent.id).map_err(String::from)?;
    if let Some(original_id) = originalId.as_deref() {
        agent_markdown_path(&agents_dir, original_id).map_err(String::from)?;
    }
    let managed_ids = OmoService::slim_managed_agent_ids();
    ensure_agent_mutable(&scope, &agent.id, &managed_ids).map_err(String::from)?;
    if let Some(original_id) = originalId.as_deref() {
        ensure_agent_mutable(&scope, original_id, &managed_ids).map_err(String::from)?;
    }
    if builtin::defaults(&agent.id).is_some() {
        if originalId.as_deref() != Some(agent.id.as_str()) {
            return Err("Built-in Agent IDs cannot be created or renamed".into());
        }
        builtin::write(&agents_dir, &scope, &agent.id, Some(&agent)).map_err(String::from)?;
        return builtin::list(&agents_dir, &scope)
            .map_err(String::from)?
            .into_iter()
            .find(|item| item.id == agent.id)
            .ok_or_else(|| "Built-in Agent not found".into());
    }
    if let Some(saved) = custom_config::save(&agents_dir, &scope, &agent, originalId.as_deref())
        .map_err(String::from)?
    {
        return Ok(saved);
    }
    save_agent_in_dir(&agents_dir, &scope, agent, originalId.as_deref()).map_err(String::from)
}

#[tauri::command]
pub async fn delete_opencode_agent(
    scope: String,
    #[allow(non_snake_case)] projectDir: Option<String>,
    id: String,
) -> Result<(), String> {
    let agents_dir = resolve_agents_dir(&scope, projectDir.as_deref()).map_err(String::from)?;
    agent_markdown_path(&agents_dir, &id).map_err(String::from)?;
    let managed_ids = OmoService::slim_managed_agent_ids();
    ensure_agent_mutable(&scope, &id, &managed_ids).map_err(String::from)?;
    if builtin::defaults(&id).is_some() {
        return Err("Built-in Agents cannot be deleted; restore defaults instead".into());
    }
    custom_config::delete(&agents_dir, &scope, &id).map_err(String::from)?;
    delete_agent_in_dir(&agents_dir, &id).map_err(String::from)
}

#[tauri::command]
pub async fn reset_opencode_agent(
    scope: String,
    #[allow(non_snake_case)] projectDir: Option<String>,
    id: String,
) -> Result<(), String> {
    let agents_dir = resolve_agents_dir(&scope, projectDir.as_deref()).map_err(String::from)?;
    ensure_agent_mutable(&scope, &id, &OmoService::slim_managed_agent_ids())
        .map_err(String::from)?;
    builtin::write(&agents_dir, &scope, &id, None).map_err(String::from)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn parses_and_serializes_agent_markdown() {
        let raw = "---\ndescription: Review code\nmode: subagent\nmodel: openai/gpt-5.6\npermission:\n  edit: deny\n---\n\nReview carefully.\n";
        let (frontmatter, prompt) = split_agent_markdown(raw).unwrap();
        assert_eq!(frontmatter["mode"], "subagent");
        assert_eq!(frontmatter["permission"]["edit"], "deny");
        assert_eq!(prompt, "Review carefully.\n");

        let serialized = serialize_agent_markdown(&frontmatter, &prompt).unwrap();
        let (roundtrip_frontmatter, roundtrip_prompt) = split_agent_markdown(&serialized).unwrap();
        assert_eq!(roundtrip_frontmatter, frontmatter);
        assert_eq!(roundtrip_prompt, prompt);
    }

    #[test]
    fn edits_native_v2_markdown_agent_without_legacy_fields() {
        let dir = tempfile::tempdir().unwrap();
        let agents_dir = dir.path().join("agents");
        std::fs::create_dir_all(&agents_dir).unwrap();
        let path = agents_dir.join("reviewer.md");
        std::fs::write(&path, "---\ndescription: Review code\nmode: subagent\nmodel: openai/coding#high\ndisabled: true\nsteps: 8\npermissions:\n  - action: shell\n    resource: \"*\"\n    effect: deny\nrequest:\n  body:\n    temperature: 0.2\n---\n\nReview carefully.\n").unwrap();

        let mut agent = list_agents_in_dir(&agents_dir, "global").unwrap().remove(0);
        assert!(agent.native_v2);
        assert_eq!(agent.frontmatter["model"], "openai/coding");
        assert_eq!(agent.frontmatter["variant"], "high");
        assert_eq!(agent.frontmatter["permission"]["bash"], "deny");
        assert_eq!(agent.frontmatter["request"]["body"]["temperature"], 0.2);
        assert_eq!(agent.frontmatter["disable"], true);
        agent.frontmatter["variant"] = json!("low");
        save_agent_in_dir(&agents_dir, "global", agent, Some("reviewer")).unwrap();

        let (saved, _) = split_agent_markdown(&std::fs::read_to_string(path).unwrap()).unwrap();
        assert_eq!(saved["model"], "openai/coding#low");
        assert_eq!(saved["permissions"][0]["action"], "shell");
        assert_eq!(saved["request"]["body"]["temperature"], 0.2);
        assert_eq!(saved["disabled"], true);
        assert!(saved.get("permission").is_none());
        assert!(saved.get("disable").is_none());
        assert!(saved.get("variant").is_none());
    }

    #[test]
    fn saves_renames_lists_and_deletes_agents() {
        let dir = tempfile::tempdir().unwrap();
        let agents_dir = dir.path().join("agents");
        let agent = OpenCodeAgentDocument {
            id: "reviewer".into(),
            scope: "global".into(),
            file_path: String::new(),
            frontmatter: json!({
                "description": "Review code",
                "mode": "subagent",
                "model": "openai/gpt-5.6"
            }),
            prompt: "Review carefully.".into(),
            has_prompt_override: true,
            last_modified: None,
            managed_by: None,
            built_in: false,
            native_v2: true,
            default_frontmatter: None,
        };
        save_agent_in_dir(&agents_dir, "global", agent.clone(), None).unwrap();

        let raw = std::fs::read_to_string(agents_dir.join("reviewer.md")).unwrap();
        assert!(raw.contains("mode: subagent"));
        assert!(!raw.contains("permission:"));

        let listed = list_agents_in_dir(&agents_dir, "global").unwrap();
        assert_eq!(listed.len(), 1);
        assert_eq!(listed[0].id, "reviewer");
        assert!(listed[0].native_v2);

        let renamed = OpenCodeAgentDocument {
            id: "security-reviewer".into(),
            ..agent
        };
        save_agent_in_dir(&agents_dir, "global", renamed, Some("reviewer")).unwrap();
        assert!(!agents_dir.join("reviewer.md").exists());
        assert!(agents_dir.join("security-reviewer.md").exists());

        delete_agent_in_dir(&agents_dir, "security-reviewer").unwrap();
        assert!(list_agents_in_dir(&agents_dir, "global")
            .unwrap()
            .is_empty());
    }

    #[test]
    fn rejects_unsafe_agent_ids() {
        assert!(validate_agent_id("../oracle").is_err());
        assert!(validate_agent_id("team//oracle").is_err());
        assert!(validate_agent_id("team/../oracle").is_err());
        assert!(validate_agent_id("review agent").is_err());
        assert!(validate_agent_id("review-agent_2").is_ok());
        assert!(validate_agent_id("team/review-agent_2").is_ok());
    }

    #[test]
    fn manages_nested_v2_markdown_agents() {
        let dir = tempfile::tempdir().unwrap();
        let agents_dir = dir.path().join("agents");
        let agent = OpenCodeAgentDocument {
            id: "team/reviewer".into(),
            scope: "project".into(),
            file_path: String::new(),
            frontmatter: json!({"description": "Review code", "mode": "subagent"}),
            prompt: "Review carefully.".into(),
            has_prompt_override: true,
            last_modified: None,
            managed_by: None,
            built_in: false,
            native_v2: true,
            default_frontmatter: None,
        };
        save_agent_in_dir(&agents_dir, "project", agent, None).unwrap();
        let listed = list_agents_in_dir(&agents_dir, "project").unwrap();
        assert_eq!(listed.len(), 1);
        assert_eq!(listed[0].id, "team/reviewer");
        let renamed = OpenCodeAgentDocument {
            id: "team/auditor".into(),
            ..listed[0].clone()
        };
        save_agent_in_dir(&agents_dir, "project", renamed, Some("team/reviewer")).unwrap();
        assert!(!agents_dir.join("team/reviewer.md").exists());
        assert!(agents_dir.join("team/auditor.md").exists());
        delete_agent_in_dir(&agents_dir, "team/auditor").unwrap();
        assert!(list_agents_in_dir(&agents_dir, "project")
            .unwrap()
            .is_empty());
    }

    #[test]
    fn rejects_duplicate_agent_creation_without_overwriting_existing_file() {
        let dir = tempfile::tempdir().unwrap();
        let agents_dir = dir.path().join("agents");
        let original = OpenCodeAgentDocument {
            id: "reviewer".into(),
            scope: "global".into(),
            file_path: String::new(),
            frontmatter: json!({
                "description": "Original reviewer",
                "mode": "subagent"
            }),
            prompt: "Original prompt.".into(),
            has_prompt_override: true,
            last_modified: None,
            managed_by: None,
            built_in: false,
            native_v2: true,
            default_frontmatter: None,
        };
        save_agent_in_dir(&agents_dir, "global", original, None).unwrap();

        let duplicate = OpenCodeAgentDocument {
            id: "reviewer".into(),
            scope: "global".into(),
            file_path: String::new(),
            frontmatter: json!({
                "description": "Replacement reviewer",
                "mode": "subagent"
            }),
            prompt: "Replacement prompt.".into(),
            has_prompt_override: true,
            last_modified: None,
            managed_by: None,
            built_in: false,
            native_v2: true,
            default_frontmatter: None,
        };
        let error = save_agent_in_dir(&agents_dir, "global", duplicate, None).unwrap_err();

        assert!(error.to_string().contains("already exists"));
        let listed = list_agents_in_dir(&agents_dir, "global").unwrap();
        assert_eq!(listed.len(), 1);
        assert_eq!(listed[0].frontmatter["description"], "Original reviewer");
        assert_eq!(listed[0].prompt, "Original prompt.\n");
    }

    #[test]
    fn marks_and_protects_only_global_managed_agents() {
        let managed_ids = ["oracle".to_string()].into_iter().collect();
        let mut agents = vec![OpenCodeAgentDocument {
            id: "oracle".into(),
            scope: "global".into(),
            file_path: String::new(),
            frontmatter: json!({}),
            prompt: String::new(),
            has_prompt_override: true,
            last_modified: None,
            managed_by: None,
            built_in: false,
            native_v2: true,
            default_frontmatter: None,
        }];

        mark_managed_agents(&mut agents, "global", &managed_ids);
        assert_eq!(agents[0].managed_by.as_deref(), Some(OMO_SLIM_SOURCE));
        assert!(ensure_agent_mutable("global", "oracle", &managed_ids).is_err());
        assert!(ensure_agent_mutable("project", "oracle", &managed_ids).is_ok());
        assert!(ensure_agent_mutable("global", "reviewer", &managed_ids).is_ok());
    }

    #[test]
    fn sorts_opencode_mcp_server_ids_for_display() {
        let servers = serde_json::from_value::<Map<String, Value>>(json!({
            "github": { "type": "remote" },
            "Context7": { "type": "remote" },
            "filesystem": { "type": "local" }
        }))
        .unwrap();

        assert_eq!(
            sorted_mcp_server_ids(servers),
            vec!["Context7", "filesystem", "github"]
        );
    }
}
