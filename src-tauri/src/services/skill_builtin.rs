//! Read-only discovery of Skills supplied by a CLI or one of its plugins.
//!
//! These Skills do not belong to Agent Switch's install/update/uninstall lifecycle.
//! The frontend uses this provenance to label them and keep destructive actions
//! unavailable.

use anyhow::{anyhow, Context, Result};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Stdio;
use tokio::process::Command;
use tokio::time::{timeout, Duration};

use crate::app_config::AppType;

const PROVIDED_SKILL_DISCOVERY_TIMEOUT: Duration = Duration::from_secs(15);

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum CliProvidedSkillSource {
    Builtin,
    Plugin {
        #[serde(rename = "pluginName")]
        plugin_name: String,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CliProvidedSkill {
    pub id: String,
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    pub directory: String,
    pub path: String,
    pub source: CliProvidedSkillSource,
}

#[derive(Debug, Deserialize)]
struct SkillFrontmatter {
    name: String,
    #[serde(default)]
    description: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
struct OpenCodeSkillRecord {
    name: String,
    #[serde(default)]
    description: Option<String>,
    location: String,
}

#[derive(Debug, Deserialize)]
struct PluginSkillsManifest {
    #[serde(default)]
    skills: BTreeMap<String, PluginSkillManifestEntry>,
}

#[derive(Debug, Deserialize)]
struct PluginSkillManifestEntry {
    status: String,
}

#[derive(Debug)]
struct OpenCodePlugin {
    name: String,
    root: Option<PathBuf>,
    managed_skills_root: PathBuf,
    managed_skill_directories: BTreeSet<String>,
    bundled_skill_files: BTreeMap<String, PathBuf>,
}

pub async fn list_cli_provided_skills(app: &AppType) -> Result<Vec<CliProvidedSkill>> {
    match app {
        AppType::Codex => list_codex_builtin_skills(),
        AppType::Gemini => Ok(list_gemini_builtin_skills()),
        AppType::OpenCode => list_opencode_provided_skills().await,
        AppType::Claude | AppType::ClaudeDesktop | AppType::OpenClaw | AppType::Hermes => {
            Ok(Vec::new())
        }
    }
}

fn resolve_cli_executable(tool: &str) -> Option<PathBuf> {
    crate::commands::build_tool_search_paths(tool)
        .into_iter()
        .flat_map(|directory| crate::commands::tool_executable_candidates(tool, &directory))
        .find(|candidate| candidate.is_file())
}

fn parse_skill_frontmatter(content: &str) -> Option<SkillFrontmatter> {
    let mut lines = content.lines();
    if lines.next()? != "---" {
        return None;
    }

    let yaml = lines
        .by_ref()
        .take_while(|line| *line != "---")
        .collect::<Vec<_>>()
        .join("\n");
    serde_yaml::from_str(&yaml).ok()
}

fn builtin_skill_from_file(
    app: &AppType,
    directory: &str,
    path: &Path,
) -> Result<CliProvidedSkill> {
    let skill_path = path.join("SKILL.md");
    let content = fs::read_to_string(&skill_path)
        .with_context(|| format!("Failed to read built-in Skill: {}", skill_path.display()))?;
    let frontmatter = parse_skill_frontmatter(&content);
    let name = frontmatter
        .as_ref()
        .map(|value| value.name.trim())
        .filter(|name| !name.is_empty())
        .unwrap_or(directory)
        .to_string();

    Ok(CliProvidedSkill {
        id: format!("{}:builtin:{directory}", app.as_str()),
        name,
        description: frontmatter.and_then(|value| value.description),
        directory: directory.to_string(),
        path: path.to_string_lossy().to_string(),
        source: CliProvidedSkillSource::Builtin,
    })
}

fn codex_system_skills_dir() -> Option<PathBuf> {
    let codex_home = std::env::var_os("CODEX_HOME")
        .filter(|value| !value.is_empty())
        .map(PathBuf::from)
        .or_else(|| dirs::home_dir().map(|home| home.join(".codex")))?;
    Some(codex_home.join("skills").join(".system"))
}

fn list_codex_builtin_skills_from(root: &Path) -> Result<Vec<CliProvidedSkill>> {
    if !root.is_dir() {
        return Ok(Vec::new());
    }

    let mut skills = Vec::new();
    for entry in fs::read_dir(root)
        .with_context(|| format!("Failed to read Codex system Skills: {}", root.display()))?
    {
        let entry = entry?;
        let path = entry.path();
        if !path.is_dir() || !path.join("SKILL.md").is_file() {
            continue;
        }
        let directory = entry.file_name().to_string_lossy().to_string();
        skills.push(builtin_skill_from_file(&AppType::Codex, &directory, &path)?);
    }
    skills.sort_by(|left, right| left.name.cmp(&right.name));
    Ok(skills)
}

fn list_codex_builtin_skills() -> Result<Vec<CliProvidedSkill>> {
    match codex_system_skills_dir() {
        Some(root) => list_codex_builtin_skills_from(&root),
        None => Ok(Vec::new()),
    }
}

fn list_gemini_builtin_skills() -> Vec<CliProvidedSkill> {
    if resolve_cli_executable("gemini").is_none() {
        return Vec::new();
    }

    vec![CliProvidedSkill {
        id: "gemini:builtin:skill-creator".to_string(),
        name: "skill-creator".to_string(),
        description: Some(
            "Create new Agent Skills with the structure and metadata expected by Gemini CLI."
                .to_string(),
        ),
        directory: "skill-creator".to_string(),
        path: "<built-in>".to_string(),
        source: CliProvidedSkillSource::Builtin,
    }]
}

fn plugin_spec_from_value(value: &serde_json::Value) -> Option<&str> {
    value
        .as_str()
        .or_else(|| value.as_array()?.first()?.as_str())
        .map(str::trim)
        .filter(|value| !value.is_empty())
}

fn package_name_from_spec(spec: &str) -> Option<String> {
    let spec = spec.strip_prefix("npm:").unwrap_or(spec).trim();
    if spec.is_empty()
        || spec.starts_with("file:")
        || spec.starts_with("http:")
        || spec.starts_with("https:")
        || Path::new(spec).is_absolute()
    {
        return None;
    }

    if spec.starts_with('@') {
        let slash = spec.find('/')?;
        let version = spec[slash + 1..].rfind('@').map(|index| slash + 1 + index);
        return Some(spec[..version.unwrap_or(spec.len())].to_string());
    }

    Some(
        spec.split_once('@')
            .map_or(spec, |(name, _)| name)
            .to_string(),
    )
}

fn opencode_cache_dir() -> Option<PathBuf> {
    std::env::var_os("XDG_CACHE_HOME")
        .filter(|value| !value.is_empty())
        .map(PathBuf::from)
        .or_else(|| dirs::home_dir().map(|home| home.join(".cache")))
        .map(|cache| cache.join("opencode"))
}

fn package_root_for_spec(spec: &str, package_name: &str) -> Option<PathBuf> {
    let direct_path = spec
        .strip_prefix("file://")
        .or_else(|| spec.strip_prefix("file:"))
        .map(PathBuf::from)
        .or_else(|| {
            let path = PathBuf::from(spec);
            path.is_absolute().then_some(path)
        });
    if direct_path.is_some() {
        return direct_path;
    }

    Some(
        opencode_cache_dir()?
            .join("packages")
            .join(spec)
            .join("node_modules")
            .join(package_name),
    )
}

fn plugin_manifest_candidates(config_dir: &Path, package_name: &str) -> Vec<PathBuf> {
    let leaf = package_name.rsplit('/').next().unwrap_or(package_name);
    [format!(".{leaf}"), leaf.to_string()]
        .into_iter()
        .map(|directory| config_dir.join(directory).join("skills-manifest.json"))
        .collect()
}

fn read_managed_plugin_skills(path: &Path) -> BTreeSet<String> {
    let Ok(content) = fs::read_to_string(path) else {
        return BTreeSet::new();
    };
    let Ok(manifest) = serde_json::from_str::<PluginSkillsManifest>(&content) else {
        log::warn!("Ignoring invalid OpenCode plugin Skill manifest: {path:?}");
        return BTreeSet::new();
    };

    manifest
        .skills
        .into_iter()
        .filter_map(|(directory, entry)| (entry.status == "managed").then_some(directory))
        .collect()
}

fn bundled_plugin_skills(root: &Path) -> BTreeMap<String, PathBuf> {
    let mut skills = BTreeMap::new();
    for skills_root in [root.join("skills"), root.join("src").join("skills")] {
        let Ok(entries) = fs::read_dir(skills_root) else {
            continue;
        };
        for entry in entries.flatten() {
            let path = entry.path().join("SKILL.md");
            if path.is_file() {
                skills.insert(entry.file_name().to_string_lossy().to_string(), path);
            }
        }
    }
    skills
}

fn opencode_plugins_from_config(
    config: &serde_json::Value,
    config_dir: &Path,
) -> Vec<OpenCodePlugin> {
    config
        .get("plugin")
        .and_then(serde_json::Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(plugin_spec_from_value)
        .filter_map(|spec| {
            let package_name = package_name_from_spec(spec)?;
            let root = package_root_for_spec(spec, &package_name).filter(|path| path.is_dir());
            let managed_skill_directories = plugin_manifest_candidates(config_dir, &package_name)
                .into_iter()
                .find(|path| path.is_file())
                .map_or_else(BTreeSet::new, |path| read_managed_plugin_skills(&path));
            let bundled_skill_files = root
                .as_deref()
                .map_or_else(BTreeMap::new, bundled_plugin_skills);
            Some(OpenCodePlugin {
                name: package_name,
                root,
                managed_skills_root: config_dir.join("skills"),
                managed_skill_directories,
                bundled_skill_files,
            })
        })
        .collect()
}

fn record_directory(record: &OpenCodeSkillRecord) -> String {
    if record.location == "<built-in>" {
        return record.name.clone();
    }

    let path = Path::new(&record.location);
    let directory_path = if path.file_name().and_then(|name| name.to_str()) == Some("SKILL.md") {
        path.parent().unwrap_or(path)
    } else {
        path
    };
    directory_path
        .file_name()
        .map(|name| name.to_string_lossy().to_string())
        .filter(|name| !name.is_empty())
        .unwrap_or_else(|| record.name.clone())
}

fn record_skill_file(record: &OpenCodeSkillRecord) -> PathBuf {
    let path = PathBuf::from(&record.location);
    if path.file_name().and_then(|name| name.to_str()) == Some("SKILL.md") {
        path
    } else {
        path.join("SKILL.md")
    }
}

fn same_file_contents(left: &Path, right: &Path) -> bool {
    match (fs::read(left), fs::read(right)) {
        (Ok(left), Ok(right)) => left == right,
        _ => false,
    }
}

fn plugin_for_record<'a>(
    record: &OpenCodeSkillRecord,
    plugins: &'a [OpenCodePlugin],
) -> Option<&'a OpenCodePlugin> {
    let directory = record_directory(record);
    let record_path = Path::new(&record.location);
    let record_file = record_skill_file(record);

    plugins.iter().find(|plugin| {
        let is_in_managed_skills_root = record_path.starts_with(&plugin.managed_skills_root);
        (is_in_managed_skills_root
            && (plugin.managed_skill_directories.contains(&directory)
                || plugin.managed_skill_directories.contains(&record.name)))
            || plugin
                .root
                .as_deref()
                .is_some_and(|root| record_path.starts_with(root))
            || (is_in_managed_skills_root
                && plugin
                    .bundled_skill_files
                    .get(&directory)
                    .or_else(|| plugin.bundled_skill_files.get(&record.name))
                    .is_some_and(|bundled| same_file_contents(&record_file, bundled)))
    })
}

fn opencode_provided_records(
    records: Vec<OpenCodeSkillRecord>,
    plugins: &[OpenCodePlugin],
) -> Vec<CliProvidedSkill> {
    let mut skills = records
        .into_iter()
        .filter_map(|record| {
            let directory = record_directory(&record);
            let source = if record.location == "<built-in>" {
                CliProvidedSkillSource::Builtin
            } else {
                let plugin = plugin_for_record(&record, plugins)?;
                CliProvidedSkillSource::Plugin {
                    plugin_name: plugin.name.clone(),
                }
            };
            let source_id = match &source {
                CliProvidedSkillSource::Builtin => "builtin".to_string(),
                CliProvidedSkillSource::Plugin { plugin_name } => {
                    format!("plugin:{plugin_name}")
                }
            };

            Some(CliProvidedSkill {
                id: format!("opencode:{source_id}:{directory}"),
                name: record.name,
                description: record.description,
                directory,
                path: record.location,
                source,
            })
        })
        .collect::<Vec<_>>();
    skills.sort_by(|left, right| left.name.cmp(&right.name));
    skills
}

async fn list_opencode_provided_skills() -> Result<Vec<CliProvidedSkill>> {
    let Some(executable) = resolve_cli_executable("opencode") else {
        return Ok(Vec::new());
    };

    let mut command = Command::new(executable);
    command
        .args(["debug", "skill"])
        .env("NO_COLOR", "1")
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true);
    if let Some(home) = dirs::home_dir() {
        command.current_dir(home);
    }

    let output = timeout(PROVIDED_SKILL_DISCOVERY_TIMEOUT, command.output())
        .await
        .map_err(|_| anyhow!("OpenCode provided Skill discovery timed out"))??;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
        return Err(anyhow!(
            "OpenCode provided Skill discovery failed: {stderr}"
        ));
    }

    let records: Vec<OpenCodeSkillRecord> = serde_json::from_slice(&output.stdout)
        .context("OpenCode returned invalid provided Skill data")?;
    let config = crate::opencode_config::read_opencode_config()
        .context("Failed to read OpenCode plugin configuration")?;
    let config_dir = crate::opencode_config::get_opencode_dir();
    let plugins = opencode_plugins_from_config(&config, &config_dir);
    Ok(opencode_provided_records(records, &plugins))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_codex_system_skill_frontmatter() {
        let parsed = parse_skill_frontmatter(
            "---\nname: openai-docs\ndescription: Official docs\n---\n\n# Docs\n",
        )
        .expect("frontmatter should parse");

        assert_eq!(parsed.name, "openai-docs");
        assert_eq!(parsed.description.as_deref(), Some("Official docs"));
    }

    #[test]
    fn parses_scoped_and_unscoped_plugin_specs() {
        assert_eq!(
            package_name_from_spec("oh-my-opencode-slim@latest").as_deref(),
            Some("oh-my-opencode-slim")
        );
        assert_eq!(
            package_name_from_spec("@example/opencode-plugin@1.2.3").as_deref(),
            Some("@example/opencode-plugin")
        );
    }

    #[test]
    fn reads_only_managed_plugin_skills() {
        let temp = tempfile::tempdir().expect("tempdir");
        let manifest = temp.path().join("skills-manifest.json");
        fs::write(
            &manifest,
            r#"{"skills":{"deepwork":{"status":"managed"},"personal":{"status":"detached"}}}"#,
        )
        .expect("write manifest");

        let skills = read_managed_plugin_skills(&manifest);

        assert_eq!(skills, BTreeSet::from(["deepwork".to_string()]));
    }

    #[test]
    fn marks_opencode_builtin_and_manifest_managed_plugin_records() {
        let plugins = vec![OpenCodePlugin {
            name: "oh-my-opencode-slim".to_string(),
            root: None,
            managed_skills_root: PathBuf::from("/tmp/skills"),
            managed_skill_directories: BTreeSet::from(["deepwork".to_string()]),
            bundled_skill_files: BTreeMap::new(),
        }];
        let skills = opencode_provided_records(
            vec![
                OpenCodeSkillRecord {
                    name: "customize-opencode".to_string(),
                    description: Some("Configure OpenCode".to_string()),
                    location: "<built-in>".to_string(),
                },
                OpenCodeSkillRecord {
                    name: "deepwork".to_string(),
                    description: None,
                    location: "/tmp/skills/deepwork/SKILL.md".to_string(),
                },
                OpenCodeSkillRecord {
                    name: "personal".to_string(),
                    description: None,
                    location: "/tmp/skills/personal/SKILL.md".to_string(),
                },
            ],
            &plugins,
        );

        assert_eq!(skills.len(), 2);
        assert!(matches!(skills[0].source, CliProvidedSkillSource::Builtin));
        assert!(matches!(
            &skills[1].source,
            CliProvidedSkillSource::Plugin { plugin_name }
                if plugin_name == "oh-my-opencode-slim"
        ));
    }

    #[test]
    fn discovers_codex_system_skills_from_disk() {
        let temp = tempfile::tempdir().expect("tempdir");
        let skill_dir = temp.path().join("skill-installer");
        fs::create_dir_all(&skill_dir).expect("create skill dir");
        fs::write(
            skill_dir.join("SKILL.md"),
            "---\nname: skill-installer\ndescription: Install Skills\n---\n",
        )
        .expect("write skill");

        let skills = list_codex_builtin_skills_from(temp.path()).expect("discover skills");

        assert_eq!(skills.len(), 1);
        assert_eq!(skills[0].id, "codex:builtin:skill-installer");
        assert_eq!(skills[0].description.as_deref(), Some("Install Skills"));
        assert!(matches!(skills[0].source, CliProvidedSkillSource::Builtin));
    }
}
