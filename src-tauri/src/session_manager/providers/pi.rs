use std::collections::{HashMap, HashSet};
use std::fs::{self, File};
use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};
use std::time::UNIX_EPOCH;

use serde_json::Value;

use crate::pi_config::get_pi_sessions_dir;
#[cfg(test)]
use crate::pi_config::resolve_pi_configured_path as resolve_configured_path;
use crate::session_manager::{SessionMessage, SessionMeta};

use super::utils::{
    extract_text, parse_timestamp_to_ms, path_basename, truncate_summary, TITLE_MAX_CHARS,
};

const PROVIDER_ID: &str = "pi";
const SUMMARY_MAX_CHARS: usize = 160;
const MAX_SCAN_DEPTH: usize = 8;

struct ParsedSession {
    header: Value,
    entries: Vec<Value>,
    active_indices: Vec<usize>,
}

pub fn session_root() -> PathBuf {
    get_pi_sessions_dir()
}

pub fn scan_sessions() -> Vec<SessionMeta> {
    let root = session_root();
    if !root.exists() {
        return Vec::new();
    }

    let mut files = Vec::new();
    collect_session_files(&root, 0, &mut files);
    files
        .iter()
        .filter_map(|path| parse_session(path))
        .collect()
}

fn collect_session_files(dir: &Path, depth: usize, files: &mut Vec<PathBuf>) {
    if depth > MAX_SCAN_DEPTH {
        return;
    }

    let Ok(entries) = fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        let Ok(file_type) = entry.file_type() else {
            continue;
        };

        if file_type.is_symlink() {
            continue;
        }
        if file_type.is_dir() {
            collect_session_files(&path, depth + 1, files);
        } else if file_type.is_file()
            && path.extension().and_then(|extension| extension.to_str()) == Some("jsonl")
        {
            files.push(path);
        }
    }
}

pub fn load_messages(path: &Path) -> Result<Vec<SessionMessage>, String> {
    let parsed = read_session(path)
        .ok_or_else(|| format!("Failed to parse Pi session: {}", path.display()))?;
    let mut messages = Vec::new();

    for index in parsed.active_indices {
        let entry = &parsed.entries[index];
        let entry_type = entry.get("type").and_then(Value::as_str).unwrap_or("");

        let (role, content, nested_timestamp) = match entry_type {
            "message" => {
                let Some(message) = entry.get("message") else {
                    continue;
                };
                let role = normalize_role(
                    message
                        .get("role")
                        .and_then(Value::as_str)
                        .unwrap_or("unknown"),
                );
                let content = message.get("content").map(extract_text).unwrap_or_default();
                let timestamp = message.get("timestamp").and_then(parse_timestamp_to_ms);
                (role, content, timestamp)
            }
            "custom_message" if entry.get("display").and_then(Value::as_bool) != Some(false) => {
                let content = entry.get("content").map(extract_text).unwrap_or_default();
                ("system".to_string(), content, None)
            }
            "compaction" | "branch_summary" => {
                let content = entry
                    .get("summary")
                    .and_then(Value::as_str)
                    .unwrap_or_default()
                    .to_string();
                ("system".to_string(), content, None)
            }
            _ => continue,
        };

        if content.trim().is_empty() {
            continue;
        }
        let ts = entry
            .get("timestamp")
            .and_then(parse_timestamp_to_ms)
            .or(nested_timestamp);
        messages.push(SessionMessage { role, content, ts });
    }

    Ok(messages)
}

pub fn delete_session(_root: &Path, path: &Path, session_id: &str) -> Result<bool, String> {
    let meta = parse_session(path)
        .ok_or_else(|| format!("Failed to parse Pi session metadata: {}", path.display()))?;
    if meta.session_id != session_id {
        return Err(format!(
            "Pi session ID mismatch: expected {session_id}, found {}",
            meta.session_id
        ));
    }

    fs::remove_file(path).map_err(|error| {
        format!(
            "Failed to delete Pi session file {}: {error}",
            path.display()
        )
    })?;
    Ok(true)
}

fn parse_session(path: &Path) -> Option<SessionMeta> {
    let parsed = read_session(path)?;
    let session_id = parsed.header.get("id")?.as_str()?.to_string();
    let project_dir = parsed
        .header
        .get("cwd")
        .and_then(Value::as_str)
        .filter(|cwd| !cwd.trim().is_empty())
        .map(str::to_string);
    let created_at = parsed
        .header
        .get("timestamp")
        .and_then(parse_timestamp_to_ms);

    let mut session_name: Option<String> = None;
    let mut first_user_message: Option<String> = None;
    let mut summary: Option<String> = None;
    let mut last_active_at: Option<i64> = None;

    for index in &parsed.active_indices {
        let entry = &parsed.entries[*index];
        last_active_at = entry
            .get("timestamp")
            .and_then(parse_timestamp_to_ms)
            .or(last_active_at);

        match entry.get("type").and_then(Value::as_str) {
            Some("session_info") => {
                session_name = entry
                    .get("name")
                    .and_then(Value::as_str)
                    .map(str::trim)
                    .filter(|name| !name.is_empty())
                    .map(str::to_string);
            }
            Some("message") => {
                let Some(message) = entry.get("message") else {
                    continue;
                };
                let role = message.get("role").and_then(Value::as_str).unwrap_or("");
                let text = message.get("content").map(extract_text).unwrap_or_default();
                if text.trim().is_empty() {
                    continue;
                }
                if role == "user" && first_user_message.is_none() {
                    first_user_message = Some(text.clone());
                }
                if role == "user" || role == "assistant" {
                    summary = Some(text);
                }
            }
            Some("compaction") | Some("branch_summary") => {
                summary = entry
                    .get("summary")
                    .and_then(Value::as_str)
                    .filter(|value| !value.trim().is_empty())
                    .map(str::to_string);
            }
            _ => {}
        }
    }

    let title = session_name
        .or(first_user_message)
        .map(|value| truncate_summary(&value, TITLE_MAX_CHARS))
        .or_else(|| project_dir.as_deref().and_then(path_basename));
    let summary = summary.map(|value| truncate_summary(&value, SUMMARY_MAX_CHARS));
    let modified_at = file_modified_ms(path);

    Some(SessionMeta {
        provider_id: PROVIDER_ID.to_string(),
        session_id: session_id.clone(),
        title,
        summary,
        project_dir,
        created_at,
        last_active_at: last_active_at.or(modified_at).or(created_at),
        source_path: Some(path.to_string_lossy().to_string()),
        resume_command: is_safe_session_id(&session_id)
            .then(|| format!("pi --session {session_id}")),
    })
}

fn read_session(path: &Path) -> Option<ParsedSession> {
    let file = File::open(path).ok()?;
    let reader = BufReader::new(file);
    let mut header: Option<Value> = None;
    let mut entries = Vec::new();

    for line in reader.lines().map_while(Result::ok) {
        let Ok(value) = serde_json::from_str::<Value>(&line) else {
            continue;
        };
        if value.get("type").and_then(Value::as_str) == Some("session") && header.is_none() {
            header = Some(value);
        } else if value.get("id").and_then(Value::as_str).is_some() {
            entries.push(value);
        }
    }

    let header = header?;
    let active_indices = active_branch_indices(&entries);
    Some(ParsedSession {
        header,
        entries,
        active_indices,
    })
}

fn active_branch_indices(entries: &[Value]) -> Vec<usize> {
    let by_id: HashMap<&str, usize> = entries
        .iter()
        .enumerate()
        .filter_map(|(index, entry)| {
            entry
                .get("id")
                .and_then(Value::as_str)
                .map(|id| (id, index))
        })
        .collect();
    let mut current = entries
        .iter()
        .rev()
        .find_map(|entry| entry.get("id").and_then(Value::as_str));
    let mut seen = HashSet::new();
    let mut branch = Vec::new();

    while let Some(id) = current {
        if !seen.insert(id) {
            break;
        }
        let Some(index) = by_id.get(id).copied() else {
            break;
        };
        branch.push(index);
        current = entries[index].get("parentId").and_then(Value::as_str);
    }
    branch.reverse();
    branch
}

fn normalize_role(role: &str) -> String {
    match role {
        "toolResult" | "tool_result" => "tool".to_string(),
        other => other.to_string(),
    }
}

fn is_safe_session_id(session_id: &str) -> bool {
    !session_id.is_empty()
        && session_id
            .chars()
            .all(|character| character.is_ascii_alphanumeric() || matches!(character, '-' | '_'))
}

fn file_modified_ms(path: &Path) -> Option<i64> {
    let duration = fs::metadata(path)
        .ok()?
        .modified()
        .ok()?
        .duration_since(UNIX_EPOCH)
        .ok()?;
    i64::try_from(duration.as_millis()).ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn write_session(path: &Path) {
        fs::write(
            path,
            concat!(
                "{\"type\":\"session\",\"version\":3,\"id\":\"session-1\",\"timestamp\":\"2026-08-13T10:00:00Z\",\"cwd\":\"/tmp/pi-project\"}\n",
                "{\"type\":\"message\",\"id\":\"user-1\",\"parentId\":null,\"timestamp\":\"2026-08-13T10:00:01Z\",\"message\":{\"role\":\"user\",\"content\":[{\"type\":\"text\",\"text\":\"Initial prompt\"}]}}\n",
                "{\"type\":\"message\",\"id\":\"abandoned\",\"parentId\":\"user-1\",\"timestamp\":\"2026-08-13T10:00:02Z\",\"message\":{\"role\":\"assistant\",\"content\":[{\"type\":\"text\",\"text\":\"Abandoned answer\"}]}}\n",
                "{\"type\":\"message\",\"id\":\"user-2\",\"parentId\":\"user-1\",\"timestamp\":\"2026-08-13T10:00:03Z\",\"message\":{\"role\":\"user\",\"content\":\"Revised prompt\"}}\n",
                "{\"type\":\"message\",\"id\":\"assistant-2\",\"parentId\":\"user-2\",\"timestamp\":\"2026-08-13T10:00:04Z\",\"message\":{\"role\":\"assistant\",\"content\":[{\"type\":\"text\",\"text\":\"Current answer\"},{\"type\":\"toolCall\",\"name\":\"read\",\"arguments\":{}}]}}\n",
                "{\"type\":\"session_info\",\"id\":\"info-1\",\"parentId\":\"assistant-2\",\"timestamp\":\"2026-08-13T10:00:05Z\",\"name\":\"Named Pi session\"}\n"
            ),
        )
        .expect("write Pi session");
    }

    #[test]
    fn parses_metadata_from_active_branch() {
        let temp = tempfile::tempdir().expect("tempdir");
        let path = temp.path().join("session.jsonl");
        write_session(&path);

        let meta = parse_session(&path).expect("parse session");
        assert_eq!(meta.session_id, "session-1");
        assert_eq!(meta.title.as_deref(), Some("Named Pi session"));
        assert_eq!(
            meta.summary.as_deref(),
            Some("Current answer\n[Tool: read]")
        );
        assert_eq!(meta.project_dir.as_deref(), Some("/tmp/pi-project"));
        assert_eq!(
            meta.resume_command.as_deref(),
            Some("pi --session session-1")
        );
        assert_eq!(meta.last_active_at, Some(1_786_615_205_000));
    }

    #[test]
    fn loads_only_messages_on_active_branch() {
        let temp = tempfile::tempdir().expect("tempdir");
        let path = temp.path().join("session.jsonl");
        write_session(&path);

        let messages = load_messages(&path).expect("load messages");
        assert_eq!(messages.len(), 3);
        assert_eq!(messages[0].content, "Initial prompt");
        assert_eq!(messages[1].content, "Revised prompt");
        assert_eq!(messages[2].content, "Current answer\n[Tool: read]");
        assert!(!messages
            .iter()
            .any(|message| message.content.contains("Abandoned")));
    }

    #[test]
    fn maps_tool_results_and_context_entries() {
        let temp = tempfile::tempdir().expect("tempdir");
        let path = temp.path().join("session.jsonl");
        fs::write(
            &path,
            concat!(
                "{\"type\":\"session\",\"version\":3,\"id\":\"session-2\",\"timestamp\":\"2026-08-13T10:00:00Z\",\"cwd\":\"/tmp\"}\n",
                "{\"type\":\"message\",\"id\":\"tool-1\",\"parentId\":null,\"timestamp\":\"2026-08-13T10:00:01Z\",\"message\":{\"role\":\"toolResult\",\"content\":[{\"type\":\"text\",\"text\":\"tool output\"}]}}\n",
                "{\"type\":\"compaction\",\"id\":\"compact-1\",\"parentId\":\"tool-1\",\"timestamp\":\"2026-08-13T10:00:02Z\",\"summary\":\"Earlier context\"}\n",
                "{\"type\":\"custom_message\",\"id\":\"custom-1\",\"parentId\":\"compact-1\",\"timestamp\":\"2026-08-13T10:00:03Z\",\"customType\":\"notice\",\"content\":\"Visible notice\",\"display\":true}\n"
            ),
        )
        .expect("write Pi session");

        let messages = load_messages(&path).expect("load messages");
        assert_eq!(messages.len(), 3);
        assert_eq!(messages[0].role, "tool");
        assert_eq!(messages[1].role, "system");
        assert_eq!(messages[1].content, "Earlier context");
        assert_eq!(messages[2].content, "Visible notice");

        let meta = parse_session(&path).expect("parse metadata");
        assert_eq!(meta.summary.as_deref(), Some("Earlier context"));
    }

    #[test]
    fn delete_validates_session_id() {
        let temp = tempfile::tempdir().expect("tempdir");
        let path = temp.path().join("session.jsonl");
        write_session(&path);

        let error = delete_session(temp.path(), &path, "wrong-session")
            .expect_err("mismatched ID must be rejected");
        assert!(error.contains("ID mismatch"));
        assert!(path.exists());

        assert!(delete_session(temp.path(), &path, "session-1").expect("delete session"));
        assert!(!path.exists());
    }

    #[test]
    fn resolves_tilde_and_relative_session_directories() {
        let relative = resolve_configured_path(".pi-sessions").expect("relative path");
        assert_eq!(
            relative,
            std::env::current_dir()
                .expect("current dir")
                .join(".pi-sessions")
        );

        if let Some(home) = dirs::home_dir() {
            assert_eq!(
                resolve_configured_path("~/pi-sessions"),
                Some(home.join("pi-sessions"))
            );
        }
    }
}
