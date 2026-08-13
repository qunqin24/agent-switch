//! Pi session usage tracking.
//!
//! Pi stores every completed LLM call as an independent `usage` object in its
//! JSONL session tree. Unlike Codex, these values are not cumulative, so each
//! usage-bearing entry can be imported directly. Pi also persists the exact
//! provider/model and per-dimension USD cost for assistant messages.

use crate::database::{lock_conn, Database};
use crate::error::AppError;
use crate::pi_config::get_pi_sessions_dir;
use crate::proxy::usage::calculator::CostCalculator;
use crate::proxy::usage::parser::TokenUsage;
use crate::services::session_usage::{
    get_sync_state, metadata_modified_nanos, update_sync_state, SessionSyncResult,
};
use crate::services::usage_stats::{find_model_pricing, should_skip_session_insert, DedupKey};
use rust_decimal::Decimal;
use serde_json::Value;
use std::collections::HashMap;
use std::fs;
use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};
use std::time::SystemTime;

const PI_SESSION_FALLBACK_PROVIDER_ID: &str = "_pi_session";
const PI_SESSION_DATA_SOURCE: &str = "pi_session";
const MAX_SCAN_DEPTH: usize = 8;

#[derive(Clone, Debug, Default)]
struct PiModelContext {
    provider: String,
    model: String,
}

#[derive(Clone, Debug)]
struct PiUsageCost {
    input: String,
    output: String,
    cache_read: String,
    cache_write: String,
    total: String,
}

#[derive(Clone, Debug)]
struct PiUsage {
    input: u32,
    output: u32,
    cache_read: u32,
    cache_write: u32,
    cost: Option<PiUsageCost>,
}

impl PiUsage {
    fn is_zero(&self) -> bool {
        self.input == 0 && self.output == 0 && self.cache_read == 0 && self.cache_write == 0
    }
}

#[derive(Clone, Debug)]
struct PiUsageRecord {
    entry_id: String,
    kind: &'static str,
    provider: String,
    model: String,
    usage: PiUsage,
    timestamp: i64,
    status_code: i64,
    error_message: Option<String>,
}

pub fn sync_pi_usage(db: &Database) -> Result<SessionSyncResult, AppError> {
    sync_pi_usage_from_dir(db, &get_pi_sessions_dir())
}

fn sync_pi_usage_from_dir(
    db: &Database,
    sessions_dir: &Path,
) -> Result<SessionSyncResult, AppError> {
    let files = collect_pi_session_files(sessions_dir);
    let mut result = SessionSyncResult {
        imported: 0,
        updated: 0,
        skipped: 0,
        files_scanned: files.len() as u32,
        errors: vec![],
    };

    for file_path in &files {
        match sync_single_pi_file(db, file_path) {
            Ok((imported, skipped)) => {
                result.imported += imported;
                result.skipped += skipped;
            }
            Err(error) => {
                let message = format!("Pi 会话文件解析失败 {}: {error}", file_path.display());
                log::warn!("[PI-SYNC] {message}");
                result.errors.push(message);
            }
        }
    }

    if result.imported > 0 {
        log::info!(
            "[PI-SYNC] 同步完成: 导入 {} 条, 跳过 {} 条, 扫描 {} 个文件",
            result.imported,
            result.skipped,
            result.files_scanned
        );
    }

    Ok(result)
}

fn collect_pi_session_files(sessions_dir: &Path) -> Vec<PathBuf> {
    let mut files = Vec::new();
    collect_pi_session_files_in_dir(sessions_dir, 0, &mut files);
    files
}

fn collect_pi_session_files_in_dir(dir: &Path, depth: usize, files: &mut Vec<PathBuf>) {
    if depth > MAX_SCAN_DEPTH {
        return;
    }

    let Ok(entries) = fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let Ok(file_type) = entry.file_type() else {
            continue;
        };
        if file_type.is_symlink() {
            continue;
        }

        let path = entry.path();
        if file_type.is_dir() {
            collect_pi_session_files_in_dir(&path, depth + 1, files);
        } else if file_type.is_file()
            && path.extension().and_then(|extension| extension.to_str()) == Some("jsonl")
        {
            files.push(path);
        }
    }
}

fn sync_single_pi_file(db: &Database, file_path: &Path) -> Result<(u32, u32), AppError> {
    let file_path_string = file_path.to_string_lossy().to_string();
    let metadata = fs::metadata(file_path)
        .map_err(|error| AppError::Config(format!("无法读取 Pi 会话元数据: {error}")))?;
    let file_modified = metadata_modified_nanos(&metadata);
    let (last_modified, _last_offset) = get_sync_state(db, &file_path_string)?;
    if file_modified <= last_modified {
        return Ok((0, 0));
    }

    let file = fs::File::open(file_path)
        .map_err(|error| AppError::Config(format!("无法打开 Pi 会话文件: {error}")))?;
    let reader = BufReader::new(file);
    let mut contexts = HashMap::<String, PiModelContext>::new();
    let mut session_id = file_path
        .file_stem()
        .and_then(|stem| stem.to_str())
        .unwrap_or("unknown")
        .to_string();
    let mut line_offset = 0_i64;
    let mut imported = 0_u32;
    let mut skipped = 0_u32;

    // Pi may rewrite a session when materializing a branch, so line offsets are
    // not a durable incremental boundary. Re-scan changed files and rely on the
    // stable session/entry request ID to avoid duplicates.
    for line in reader.lines() {
        line_offset += 1;
        let line = match line {
            Ok(line) if !line.trim().is_empty() => line,
            Ok(_) | Err(_) => continue,
        };
        let value = match serde_json::from_str::<Value>(&line) {
            Ok(value) => value,
            Err(_) => continue,
        };

        if value.get("type").and_then(Value::as_str) == Some("session") {
            if let Some(id) = value.get("id").and_then(Value::as_str) {
                session_id = id.to_string();
            }
            continue;
        }

        let context_parent_id =
            if value.get("type").and_then(Value::as_str) == Some("branch_summary") {
                // Branch-summary usage is generated while leaving the old branch;
                // prefer that branch's model when Pi provides `fromId`.
                value
                    .get("fromId")
                    .and_then(Value::as_str)
                    .or_else(|| value.get("parentId").and_then(Value::as_str))
            } else {
                value.get("parentId").and_then(Value::as_str)
            };
        let parent_context = context_parent_id
            .and_then(|parent_id| contexts.get(parent_id))
            .cloned()
            .unwrap_or_default();
        let context = context_for_entry(&value, parent_context);
        if let Some(entry_id) = value.get("id").and_then(Value::as_str) {
            contexts.insert(entry_id.to_string(), context.clone());
        }

        let Some(record) = parse_usage_record(&value, &context) else {
            continue;
        };
        let request_id = format!(
            "pi_session:{session_id}:{}:{}",
            record.kind, record.entry_id
        );
        if insert_pi_usage_record(db, &request_id, &session_id, &record)? {
            imported += 1;
        } else {
            skipped += 1;
        }
    }

    update_sync_state(db, &file_path_string, file_modified, line_offset)?;
    Ok((imported, skipped))
}

fn context_for_entry(value: &Value, mut context: PiModelContext) -> PiModelContext {
    match value.get("type").and_then(Value::as_str) {
        Some("model_change") => {
            if let Some(provider) = value.get("provider").and_then(Value::as_str) {
                context.provider = provider.to_string();
            }
            if let Some(model) = value.get("modelId").and_then(Value::as_str) {
                context.model = model.to_string();
            }
        }
        Some("message") => {
            if let Some(message) = value.get("message") {
                if message.get("role").and_then(Value::as_str) == Some("assistant") {
                    if let Some(provider) = message.get("provider").and_then(Value::as_str) {
                        context.provider = provider.to_string();
                    }
                    if let Some(model) = message.get("model").and_then(Value::as_str) {
                        context.model = model.to_string();
                    }
                }
            }
        }
        _ => {}
    }
    context
}

fn parse_usage_record(value: &Value, context: &PiModelContext) -> Option<PiUsageRecord> {
    let entry_id = value.get("id")?.as_str()?.to_string();
    let entry_timestamp = value.get("timestamp");
    let entry_type = value.get("type")?.as_str()?;

    let (kind, usage_value, message_timestamp, status_code, error_message) = match entry_type {
        "message" => {
            let message = value.get("message")?;
            let role = message.get("role")?.as_str()?;
            match role {
                "assistant" => {
                    let stop_reason = message
                        .get("stopReason")
                        .and_then(Value::as_str)
                        .unwrap_or("stop");
                    let status = match stop_reason {
                        "error" => 500,
                        "aborted" => 499,
                        _ => 200,
                    };
                    let error = message
                        .get("errorMessage")
                        .and_then(Value::as_str)
                        .map(str::to_string)
                        .or_else(|| (status >= 400).then(|| format!("Pi request {stop_reason}")));
                    (
                        "assistant",
                        message.get("usage")?,
                        message.get("timestamp"),
                        status,
                        error,
                    )
                }
                "toolResult" | "tool_result" => {
                    let is_error = message
                        .get("isError")
                        .and_then(Value::as_bool)
                        .unwrap_or(false);
                    (
                        "tool",
                        message.get("usage")?,
                        message.get("timestamp"),
                        if is_error { 500 } else { 200 },
                        is_error.then(|| "Pi tool result reported an error".to_string()),
                    )
                }
                _ => return None,
            }
        }
        "compaction" => ("compaction", value.get("usage")?, None, 200, None),
        "branch_summary" => ("branch_summary", value.get("usage")?, None, 200, None),
        _ => return None,
    };

    let usage = parse_pi_usage(usage_value)?;
    if usage.is_zero() {
        return None;
    }

    Some(PiUsageRecord {
        entry_id,
        kind,
        provider: nonempty_or(&context.provider, PI_SESSION_FALLBACK_PROVIDER_ID),
        model: nonempty_or(&context.model, "unknown"),
        usage,
        timestamp: parse_timestamp(message_timestamp)
            .or_else(|| parse_timestamp(entry_timestamp))
            .unwrap_or_else(now_timestamp),
        status_code,
        error_message,
    })
}

fn parse_pi_usage(value: &Value) -> Option<PiUsage> {
    if !value.is_object() {
        return None;
    }
    Some(PiUsage {
        input: json_u32(value.get("input")),
        output: json_u32(value.get("output")),
        cache_read: json_u32(value.get("cacheRead")),
        cache_write: json_u32(value.get("cacheWrite")),
        cost: parse_pi_cost(value.get("cost")),
    })
}

fn parse_pi_cost(value: Option<&Value>) -> Option<PiUsageCost> {
    let value = value?.as_object()?;
    let input = json_nonnegative_number(value.get("input")).unwrap_or(0.0);
    let output = json_nonnegative_number(value.get("output")).unwrap_or(0.0);
    let cache_read = json_nonnegative_number(value.get("cacheRead")).unwrap_or(0.0);
    let cache_write = json_nonnegative_number(value.get("cacheWrite")).unwrap_or(0.0);
    let total = json_nonnegative_number(value.get("total"))
        .unwrap_or(input + output + cache_read + cache_write);
    Some(PiUsageCost {
        input: input.to_string(),
        output: output.to_string(),
        cache_read: cache_read.to_string(),
        cache_write: cache_write.to_string(),
        total: total.to_string(),
    })
}

fn insert_pi_usage_record(
    db: &Database,
    request_id: &str,
    session_id: &str,
    record: &PiUsageRecord,
) -> Result<bool, AppError> {
    let conn = lock_conn!(db.conn);
    let dedup_key = DedupKey {
        app_type: "pi",
        model: &record.model,
        input_tokens: record.usage.input,
        output_tokens: record.usage.output,
        cache_read_tokens: record.usage.cache_read,
        cache_creation_tokens: record.usage.cache_write,
        created_at: record.timestamp,
    };
    if should_skip_session_insert(&conn, request_id, &dedup_key)? {
        return Ok(false);
    }

    let (input_cost, output_cost, cache_read_cost, cache_write_cost, total_cost) =
        if let Some(cost) = &record.usage.cost {
            (
                cost.input.clone(),
                cost.output.clone(),
                cost.cache_read.clone(),
                cost.cache_write.clone(),
                cost.total.clone(),
            )
        } else {
            calculate_fallback_costs(&conn, record)
        };
    let provider_type = if record.usage.cost.is_some() {
        // Marks even an explicit zero as authoritative so pricing backfill does
        // not invent a bill for Pi's local/free models.
        "pi_session_reported_cost"
    } else {
        PI_SESSION_DATA_SOURCE
    };

    let inserted = conn
        .execute(
            "INSERT OR IGNORE INTO proxy_request_logs (
                request_id, provider_id, app_type, model, request_model, pricing_model,
                input_tokens, output_tokens, cache_read_tokens, cache_creation_tokens,
                input_cost_usd, output_cost_usd, cache_read_cost_usd, cache_creation_cost_usd,
                total_cost_usd, latency_ms, first_token_ms, status_code, error_message,
                session_id, provider_type, is_streaming, cost_multiplier, created_at, data_source
            ) VALUES (?1, ?2, 'pi', ?3, ?3, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11,
                      ?12, 0, NULL, ?13, ?14, ?15, ?16, 1, '1.0', ?17, ?18)",
            rusqlite::params![
                request_id,
                record.provider,
                record.model,
                record.usage.input,
                record.usage.output,
                record.usage.cache_read,
                record.usage.cache_write,
                input_cost,
                output_cost,
                cache_read_cost,
                cache_write_cost,
                total_cost,
                record.status_code,
                record.error_message,
                session_id,
                provider_type,
                record.timestamp,
                PI_SESSION_DATA_SOURCE,
            ],
        )
        .map_err(|error| AppError::Database(format!("插入 Pi 会话日志失败: {error}")))?;

    if inserted > 0 {
        crate::usage_events::notify_log_recorded();
    }
    Ok(inserted > 0)
}

fn calculate_fallback_costs(
    conn: &rusqlite::Connection,
    record: &PiUsageRecord,
) -> (String, String, String, String, String) {
    let usage = TokenUsage {
        input_tokens: record.usage.input,
        output_tokens: record.usage.output,
        cache_read_tokens: record.usage.cache_read,
        cache_creation_tokens: record.usage.cache_write,
        model: Some(record.model.clone()),
        message_id: None,
    };
    match find_model_pricing(conn, &record.model) {
        Some(pricing) => {
            let cost = CostCalculator::calculate_for_app("pi", &usage, &pricing, Decimal::ONE);
            (
                cost.input_cost.to_string(),
                cost.output_cost.to_string(),
                cost.cache_read_cost.to_string(),
                cost.cache_creation_cost.to_string(),
                cost.total_cost.to_string(),
            )
        }
        None => (
            "0".to_string(),
            "0".to_string(),
            "0".to_string(),
            "0".to_string(),
            "0".to_string(),
        ),
    }
}

fn json_u32(value: Option<&Value>) -> u32 {
    value
        .and_then(Value::as_u64)
        .unwrap_or(0)
        .min(u32::MAX as u64) as u32
}

fn json_nonnegative_number(value: Option<&Value>) -> Option<f64> {
    value
        .and_then(Value::as_f64)
        .filter(|number| number.is_finite() && *number >= 0.0)
}

fn parse_timestamp(value: Option<&Value>) -> Option<i64> {
    let value = value?;
    if let Some(timestamp) = value.as_i64() {
        return Some(if timestamp.abs() >= 10_000_000_000 {
            timestamp / 1000
        } else {
            timestamp
        });
    }
    value
        .as_str()
        .and_then(|timestamp| chrono::DateTime::parse_from_rfc3339(timestamp).ok())
        .map(|timestamp| timestamp.timestamp())
}

fn nonempty_or(value: &str, fallback: &str) -> String {
    let value = value.trim();
    if value.is_empty() {
        fallback.to_string()
    } else {
        value.to_string()
    }
}

fn now_timestamp() -> i64 {
    SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .map(|duration| duration.as_secs() as i64)
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_assistant_usage_with_exact_pi_costs() {
        let value = serde_json::json!({
            "type": "message",
            "id": "assistant-1",
            "parentId": "user-1",
            "timestamp": "2026-08-13T14:24:35.956Z",
            "message": {
                "role": "assistant",
                "provider": "opencode-go",
                "model": "deepseek-v4-flash",
                "timestamp": 1786631073849_i64,
                "usage": {
                    "input": 4799,
                    "output": 65,
                    "cacheRead": 100,
                    "cacheWrite": 20,
                    "reasoning": 36,
                    "totalTokens": 4984,
                    "cost": {
                        "input": 0.00033593,
                        "output": 0.0000091,
                        "cacheRead": 0.000001,
                        "cacheWrite": 0.000002,
                        "total": 0.00034803
                    }
                },
                "stopReason": "stop"
            }
        });
        let context = context_for_entry(&value, PiModelContext::default());
        let record = parse_usage_record(&value, &context).expect("valid Pi usage");
        assert_eq!(record.provider, "opencode-go");
        assert_eq!(record.model, "deepseek-v4-flash");
        assert_eq!(record.usage.input, 4799);
        assert_eq!(record.usage.output, 65);
        assert_eq!(record.usage.cache_read, 100);
        assert_eq!(record.usage.cache_write, 20);
        assert_eq!(record.timestamp, 1_786_631_073);
        assert_eq!(record.usage.cost.unwrap().total, "0.00034803");
    }

    #[test]
    fn sync_imports_assistant_tool_and_summary_usage_once() -> Result<(), AppError> {
        let temp = tempfile::tempdir().expect("temp dir should be created");
        let session_dir = temp.path().join("sessions").join("--project--");
        fs::create_dir_all(&session_dir).expect("session directory should be created");
        let session_path = session_dir.join("session.jsonl");
        let lines = [
            serde_json::json!({
                "type": "session", "version": 3, "id": "session-1",
                "timestamp": "2026-08-13T10:00:00Z", "cwd": "/tmp/project"
            }),
            serde_json::json!({
                "type": "model_change", "id": "model-1", "parentId": null,
                "timestamp": "2026-08-13T10:00:01Z", "provider": "anthropic",
                "modelId": "claude-sonnet-4-5"
            }),
            serde_json::json!({
                "type": "message", "id": "assistant-1", "parentId": "model-1",
                "timestamp": "2026-08-13T10:00:02Z",
                "message": {
                    "role": "assistant", "provider": "anthropic", "model": "claude-sonnet-4-5",
                    "timestamp": 1_786_608_002_000_i64, "stopReason": "stop",
                    "usage": {
                        "input": 100, "output": 20, "cacheRead": 40, "cacheWrite": 10,
                        "totalTokens": 170,
                        "cost": {"input": 0.001, "output": 0.002, "cacheRead": 0.0001, "cacheWrite": 0.0002, "total": 0.0033}
                    }
                }
            }),
            serde_json::json!({
                "type": "message", "id": "tool-1", "parentId": "assistant-1",
                "timestamp": "2026-08-13T10:00:03Z",
                "message": {
                    "role": "toolResult", "toolName": "subagent", "isError": false,
                    "timestamp": 1_786_608_003_000_i64,
                    "usage": {
                        "input": 50, "output": 10, "cacheRead": 0, "cacheWrite": 0,
                        "totalTokens": 60,
                        "cost": {"input": 0.0005, "output": 0.001, "cacheRead": 0, "cacheWrite": 0, "total": 0.0015}
                    }
                }
            }),
            serde_json::json!({
                "type": "compaction", "id": "compact-1", "parentId": "tool-1",
                "timestamp": "2026-08-13T10:00:04Z", "summary": "summary", "tokensBefore": 1000,
                "usage": {
                    "input": 30, "output": 5, "cacheRead": 0, "cacheWrite": 0,
                    "totalTokens": 35,
                    "cost": {"input": 0.0003, "output": 0.0005, "cacheRead": 0, "cacheWrite": 0, "total": 0.0008}
                }
            }),
            serde_json::json!({
                "type": "branch_summary", "id": "branch-1", "parentId": "compact-1",
                "timestamp": "2026-08-13T10:00:05Z", "fromId": "assistant-1", "summary": "branch",
                "usage": {
                    "input": 20, "output": 4, "cacheRead": 0, "cacheWrite": 0,
                    "totalTokens": 24,
                    "cost": {"input": 0.0002, "output": 0.0004, "cacheRead": 0, "cacheWrite": 0, "total": 0.0006}
                }
            }),
        ];
        let content = lines
            .into_iter()
            .map(|line| line.to_string())
            .collect::<Vec<_>>()
            .join("\n")
            + "\n";
        fs::write(&session_path, content).expect("Pi session should be written");

        let db = Database::memory()?;
        let first = sync_pi_usage_from_dir(&db, &temp.path().join("sessions"))?;
        assert_eq!(first.imported, 4);
        assert_eq!(first.files_scanned, 1);

        let conn = lock_conn!(db.conn);
        let totals: (i64, i64, i64, i64, String) = conn.query_row(
            "SELECT SUM(input_tokens), SUM(output_tokens), SUM(cache_read_tokens),
                    SUM(cache_creation_tokens), printf('%.4f', SUM(CAST(total_cost_usd AS REAL)))
             FROM proxy_request_logs WHERE app_type = 'pi'",
            [],
            |row| {
                Ok((
                    row.get(0)?,
                    row.get(1)?,
                    row.get(2)?,
                    row.get(3)?,
                    row.get(4)?,
                ))
            },
        )?;
        assert_eq!(totals, (200, 39, 40, 10, "0.0062".to_string()));
        let provider: String = conn.query_row(
            "SELECT provider_id FROM proxy_request_logs WHERE request_id LIKE '%assistant-1'",
            [],
            |row| row.get(0),
        )?;
        assert_eq!(provider, "anthropic");
        drop(conn);

        let second = sync_pi_usage_from_dir(&db, &temp.path().join("sessions"))?;
        assert_eq!(second.imported, 0);
        Ok(())
    }

    #[test]
    fn pi_reported_zero_cost_is_not_repriced() -> Result<(), AppError> {
        let db = Database::memory()?;
        {
            let conn = lock_conn!(db.conn);
            conn.execute(
                "INSERT INTO model_pricing (
                    model_id, display_name, input_cost_per_million, output_cost_per_million,
                    cache_read_cost_per_million, cache_creation_cost_per_million
                 ) VALUES ('local-model', 'Local', '10', '20', '1', '2')",
                [],
            )?;
        }
        let record = PiUsageRecord {
            entry_id: "assistant-free".to_string(),
            kind: "assistant",
            provider: "ollama".to_string(),
            model: "local-model".to_string(),
            usage: PiUsage {
                input: 100,
                output: 10,
                cache_read: 0,
                cache_write: 0,
                cost: Some(PiUsageCost {
                    input: "0".to_string(),
                    output: "0".to_string(),
                    cache_read: "0".to_string(),
                    cache_write: "0".to_string(),
                    total: "0".to_string(),
                }),
            },
            timestamp: 1000,
            status_code: 200,
            error_message: None,
        };
        assert!(insert_pi_usage_record(
            &db,
            "pi_session:session-free:assistant:assistant-free",
            "session-free",
            &record,
        )?);
        assert_eq!(db.backfill_missing_usage_costs()?, 0);

        let conn = lock_conn!(db.conn);
        let cost: String = conn.query_row(
            "SELECT total_cost_usd FROM proxy_request_logs WHERE app_type = 'pi'",
            [],
            |row| row.get(0),
        )?;
        assert_eq!(cost, "0");
        Ok(())
    }
}
