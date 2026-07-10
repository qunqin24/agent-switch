//! Grok Build 会话日志使用追踪
//!
//! 从 `~/.grok/logs/unified.jsonl` 的 `shell.turn.inference_done` 事件读取每轮
//! 推理用量，并以会话目录中的 `events.jsonl` 关联每轮实际使用的模型。

use crate::database::{lock_conn, Database};
use crate::error::AppError;
use crate::proxy::usage::calculator::CostCalculator;
use crate::proxy::usage::parser::TokenUsage;
use crate::services::session_usage::{
    get_sync_state, metadata_modified_nanos, update_sync_state, SessionSyncResult,
};
use crate::services::usage_stats::{find_model_pricing, should_skip_session_insert, DedupKey};
use rust_decimal::Decimal;
use std::collections::HashMap;
use std::fs;
use std::io::{BufRead, BufReader, Read};
use std::path::Path;
use std::time::SystemTime;

const GROK_SESSION_PROVIDER_ID: &str = "_grok_session";
const GROK_SESSION_DATA_SOURCE: &str = "grok_session";
const GROK_LOG_SIZE_SYNC_SUFFIX: &str = "::grok-size-v1";
const GROK_LOG_FINGERPRINT_SYNC_SUFFIX: &str = "::grok-fingerprint-v1";

type GrokTurnModelMap = HashMap<(String, u64), String>;
type GrokTimestampModelMap = HashMap<(String, String), String>;

#[derive(Debug)]
struct GrokTurnUsage {
    session_id: String,
    timestamp: String,
    input_tokens: u32,
    output_tokens: u32,
    cache_read_tokens: u32,
    turn_number: Option<u64>,
}

#[derive(Debug)]
struct StoredGrokSessionLog {
    request_id: String,
    session_id: String,
    input_tokens: u32,
    output_tokens: u32,
    cache_read_tokens: u32,
}

/// 同步 Grok Build 使用数据（从 unified.jsonl）。
pub fn sync_grok_usage(db: &Database) -> Result<SessionSyncResult, AppError> {
    let Some(home) = dirs::home_dir() else {
        return Ok(empty_result());
    };
    sync_grok_usage_from_dir(db, &home.join(".grok"))
}

fn sync_grok_usage_from_dir(db: &Database, grok_dir: &Path) -> Result<SessionSyncResult, AppError> {
    let log_path = grok_dir.join("logs").join("unified.jsonl");
    if !log_path.exists() {
        return Ok(empty_result());
    }

    let file_path = log_path.to_string_lossy().to_string();
    let metadata = fs::metadata(&log_path)
        .map_err(|error| AppError::Config(format!("无法读取 Grok unified 日志元数据: {error}")))?;
    let file_modified = metadata_modified_nanos(&metadata);
    let file_size = metadata.len().min(i64::MAX as u64) as i64;
    let file_fingerprint = grok_log_fingerprint(&log_path)?;
    let (last_modified, last_offset) = get_sync_state(db, &file_path)?;
    let size_sync_key = format!("{file_path}{GROK_LOG_SIZE_SYNC_SUFFIX}");
    let fingerprint_sync_key = format!("{file_path}{GROK_LOG_FINGERPRINT_SYNC_SUFFIX}");
    let (_, last_size) = get_sync_state(db, &size_sync_key)?;
    let (_, last_fingerprint) = get_sync_state(db, &fingerprint_sync_key)?;
    // unified.jsonl 轮转/截断后，旧行号已失效。检测到文件身份变化时从头读，
    // request_id 去重会避免重复写入未变化的旧记录。
    let reset_offset =
        last_offset > 0 && (file_size < last_size || file_fingerprint != last_fingerprint);
    let start_offset = if reset_offset { 0 } else { last_offset };
    if reset_offset {
        log::info!("[GROK-SYNC] 检测到 unified 日志轮转，重新扫描文件");
    }

    let turn_models = collect_session_turn_models(&grok_dir.join("sessions"));
    // 早期版本写入的 unknown 行不会再次经过增量导入。只有能从具体 turn 关联到
    // events.jsonl 模型时才回填，绝不以会话当前模型猜测历史轮次。
    let updated = reconcile_unknown_grok_models(db, &log_path, &turn_models)?;
    if file_modified <= last_modified && !reset_offset {
        return Ok(SessionSyncResult {
            updated,
            files_scanned: 1,
            ..empty_result()
        });
    }

    let file = fs::File::open(&log_path)
        .map_err(|error| AppError::Config(format!("无法打开 Grok unified 日志: {error}")))?;
    let reader = BufReader::new(file);
    let mut result = SessionSyncResult {
        imported: 0,
        updated,
        skipped: 0,
        files_scanned: 1,
        errors: vec![],
    };
    let mut line_offset = 0_i64;

    for line in reader.lines() {
        line_offset += 1;
        if line_offset <= start_offset {
            continue;
        }

        let line = match line {
            Ok(line) if line.contains("shell.turn.inference_done") => line,
            Ok(_) | Err(_) => continue,
        };
        let value = match serde_json::from_str::<serde_json::Value>(&line) {
            Ok(value) => value,
            Err(_) => continue,
        };
        let Some(turn) = parse_grok_turn_usage(&value) else {
            continue;
        };
        if turn.input_tokens == 0 && turn.output_tokens == 0 && turn.cache_read_tokens == 0 {
            continue;
        }

        let model = model_for_turn(&turn_models, &turn).unwrap_or("unknown");
        let request_id = format!(
            "grok_session:{}:{}:{}",
            turn.session_id, turn.timestamp, line_offset
        );
        match insert_grok_session_entry(db, &request_id, &turn, model) {
            Ok(true) => result.imported += 1,
            Ok(false) => result.skipped += 1,
            Err(error) => {
                let message = format!("Grok 会话日志插入失败 {request_id}: {error}");
                log::warn!("[GROK-SYNC] {message}");
                result.errors.push(message);
                result.skipped += 1;
            }
        }
    }

    update_sync_state(db, &file_path, file_modified, line_offset)?;
    update_sync_state(db, &size_sync_key, file_modified, file_size)?;
    update_sync_state(db, &fingerprint_sync_key, file_modified, file_fingerprint)?;
    if result.imported > 0 {
        log::info!(
            "[GROK-SYNC] 同步完成: 导入 {} 条, 跳过 {} 条",
            result.imported,
            result.skipped
        );
    }

    Ok(result)
}

fn empty_result() -> SessionSyncResult {
    SessionSyncResult {
        imported: 0,
        updated: 0,
        skipped: 0,
        files_scanned: 0,
        errors: vec![],
    }
}

fn parse_grok_turn_usage(value: &serde_json::Value) -> Option<GrokTurnUsage> {
    if value.get("msg")?.as_str()? != "shell.turn.inference_done" {
        return None;
    }

    let session_id = value.get("sid")?.as_str()?.to_string();
    let timestamp = value.get("ts")?.as_str()?.to_string();
    let context = value.get("ctx")?;
    let completion_tokens = json_u32(context.get("completion_tokens"));
    let reasoning_tokens = json_u32(context.get("reasoning_tokens"));

    Some(GrokTurnUsage {
        session_id,
        timestamp,
        // Grok Build reports total prompt tokens with cached prompt tokens included.
        input_tokens: json_u32(context.get("prompt_tokens")),
        output_tokens: completion_tokens.saturating_add(reasoning_tokens),
        cache_read_tokens: json_u32(context.get("cached_prompt_tokens")),
        // `loop_index` starts at 1 while events.jsonl `turn_number` starts at 0.
        turn_number: context
            .get("turn_number")
            .and_then(serde_json::Value::as_u64)
            .or_else(|| {
                context
                    .get("loop_index")
                    .and_then(serde_json::Value::as_u64)
                    .and_then(|index| index.checked_sub(1))
            }),
    })
}

fn json_u32(value: Option<&serde_json::Value>) -> u32 {
    value
        .and_then(serde_json::Value::as_u64)
        .unwrap_or(0)
        .min(u32::MAX as u64) as u32
}

fn collect_session_turn_models(sessions_dir: &Path) -> GrokTurnModelMap {
    let mut models = HashMap::new();
    // Grok stores sessions as either `sessions/<session>` or
    // `sessions/<project>/<session>`; one nested project level covers both.
    collect_session_turn_models_in_dir(sessions_dir, 1, &mut models);

    models
}

fn collect_session_turn_models_in_dir(
    directory: &Path,
    remaining_depth: usize,
    models: &mut GrokTurnModelMap,
) {
    let Ok(entries) = fs::read_dir(directory) else {
        return;
    };

    for entry in entries.flatten() {
        let Ok(file_type) = entry.file_type() else {
            continue;
        };
        if !file_type.is_dir() {
            continue;
        }

        let path = entry.path();
        collect_turn_models_from_events(&path.join("events.jsonl"), models);

        if remaining_depth > 0 {
            collect_session_turn_models_in_dir(&path, remaining_depth - 1, models);
        }
    }
}

fn collect_turn_models_from_events(path: &Path, models: &mut GrokTurnModelMap) {
    let Ok(file) = fs::File::open(path) else {
        return;
    };

    for line in BufReader::new(file).lines().flatten() {
        let Ok(event) = serde_json::from_str::<serde_json::Value>(&line) else {
            continue;
        };
        if event.get("type").and_then(serde_json::Value::as_str) != Some("turn_started") {
            continue;
        }
        let session_id = event.get("session_id").and_then(serde_json::Value::as_str);
        let turn_number = event.get("turn_number").and_then(serde_json::Value::as_u64);
        let model = event.get("model_id").and_then(serde_json::Value::as_str);
        if let (Some(session_id), Some(turn_number), Some(model)) = (session_id, turn_number, model)
        {
            models.insert((session_id.to_string(), turn_number), model.to_string());
        }
    }
}

fn model_for_turn<'a>(models: &'a GrokTurnModelMap, turn: &GrokTurnUsage) -> Option<&'a str> {
    let turn_number = turn.turn_number?;
    models
        .get(&(turn.session_id.clone(), turn_number))
        .map(String::as_str)
}

fn grok_log_fingerprint(log_path: &Path) -> Result<i64, AppError> {
    let mut file = fs::File::open(log_path)
        .map_err(|error| AppError::Config(format!("无法打开 Grok unified 日志: {error}")))?;
    let mut buffer = [0_u8; 4096];
    let read = file
        .read(&mut buffer)
        .map_err(|error| AppError::Config(format!("无法读取 Grok unified 日志: {error}")))?;
    let mut hash = 0xcbf29ce484222325_u64;
    for byte in &buffer[..read] {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(0x100000001b3);
    }
    Ok((hash & i64::MAX as u64) as i64)
}

/// 用每轮 events.jsonl 的真实模型修复早期导入时未能识别模型的 Grok 记录。
fn reconcile_unknown_grok_models(
    db: &Database,
    log_path: &Path,
    turn_models: &GrokTurnModelMap,
) -> Result<u32, AppError> {
    if turn_models.is_empty() {
        return Ok(0);
    }

    let conn = lock_conn!(db.conn);
    let unresolved_logs = {
        let mut statement = conn.prepare(
            "SELECT request_id, session_id, input_tokens, output_tokens, cache_read_tokens
             FROM proxy_request_logs
             WHERE provider_id = ?1
               AND app_type = 'grok'
               AND data_source = ?2
               AND LOWER(TRIM(model)) IN ('', 'unknown', 'null', 'none')
               AND session_id IS NOT NULL",
        )?;
        let rows = statement.query_map(
            rusqlite::params![GROK_SESSION_PROVIDER_ID, GROK_SESSION_DATA_SOURCE],
            |row| {
                Ok(StoredGrokSessionLog {
                    request_id: row.get(0)?,
                    session_id: row.get(1)?,
                    input_tokens: row.get::<_, i64>(2)? as u32,
                    output_tokens: row.get::<_, i64>(3)? as u32,
                    cache_read_tokens: row.get::<_, i64>(4)? as u32,
                })
            },
        )?;
        rows.collect::<Result<Vec<_>, _>>()?
    };

    if unresolved_logs.is_empty() {
        return Ok(0);
    }

    let timestamp_models = collect_timestamp_models_from_log(log_path, turn_models)?;
    let mut updated = 0;
    for log in unresolved_logs {
        let Some(timestamp) = request_timestamp(&log.request_id, &log.session_id) else {
            continue;
        };
        let Some(model) = timestamp_models.get(&(log.session_id.clone(), timestamp.to_string()))
        else {
            continue;
        };
        let usage = TokenUsage {
            input_tokens: log.input_tokens,
            output_tokens: log.output_tokens,
            cache_read_tokens: log.cache_read_tokens,
            cache_creation_tokens: 0,
            model: Some(model.clone()),
            message_id: None,
        };
        let (input_cost, output_cost, cache_read_cost, total_cost) =
            calculate_grok_session_costs(&conn, model, &usage);
        let changed = conn.execute(
            "UPDATE proxy_request_logs
             SET model = ?1,
                 request_model = ?1,
                 pricing_model = ?1,
                 input_cost_usd = ?2,
                 output_cost_usd = ?3,
                 cache_read_cost_usd = ?4,
                 cache_creation_cost_usd = '0',
                 total_cost_usd = ?5
             WHERE request_id = ?6",
            rusqlite::params![
                model,
                input_cost,
                output_cost,
                cache_read_cost,
                total_cost,
                log.request_id,
            ],
        )?;
        updated += changed as u32;
    }

    if updated > 0 {
        log::info!("[GROK-SYNC] 已修复 {updated} 条缺失模型的 Grok 会话记录");
        crate::usage_events::notify_log_recorded();
    }

    Ok(updated)
}

fn collect_timestamp_models_from_log(
    log_path: &Path,
    turn_models: &GrokTurnModelMap,
) -> Result<GrokTimestampModelMap, AppError> {
    let file = fs::File::open(log_path)
        .map_err(|error| AppError::Config(format!("无法打开 Grok unified 日志: {error}")))?;
    let mut models = HashMap::new();
    for line in BufReader::new(file).lines().flatten() {
        if !line.contains("shell.turn.inference_done") {
            continue;
        }
        let Ok(value) = serde_json::from_str::<serde_json::Value>(&line) else {
            continue;
        };
        let Some(turn) = parse_grok_turn_usage(&value) else {
            continue;
        };
        if let Some(model) = model_for_turn(turn_models, &turn) {
            models.insert((turn.session_id, turn.timestamp), model.to_string());
        }
    }
    Ok(models)
}

fn request_timestamp<'a>(request_id: &'a str, session_id: &str) -> Option<&'a str> {
    request_id
        .strip_prefix("grok_session:")?
        .strip_prefix(session_id)?
        .strip_prefix(':')?
        .rsplit_once(':')
        .map(|(timestamp, _)| timestamp)
}

fn calculate_grok_session_costs(
    conn: &rusqlite::Connection,
    model: &str,
    usage: &TokenUsage,
) -> (String, String, String, String) {
    match find_model_pricing(conn, model) {
        Some(pricing) => {
            let cost = CostCalculator::calculate_for_app("grok", usage, &pricing, Decimal::ONE);
            (
                cost.input_cost.to_string(),
                cost.output_cost.to_string(),
                cost.cache_read_cost.to_string(),
                cost.total_cost.to_string(),
            )
        }
        None => (
            "0".to_string(),
            "0".to_string(),
            "0".to_string(),
            "0".to_string(),
        ),
    }
}

fn insert_grok_session_entry(
    db: &Database,
    request_id: &str,
    turn: &GrokTurnUsage,
    model: &str,
) -> Result<bool, AppError> {
    let created_at = chrono::DateTime::parse_from_rfc3339(&turn.timestamp)
        .map(|timestamp| timestamp.timestamp())
        .unwrap_or_else(|_| now_timestamp());
    let dedup_key = DedupKey {
        app_type: "grok",
        model,
        input_tokens: turn.input_tokens,
        output_tokens: turn.output_tokens,
        cache_read_tokens: turn.cache_read_tokens,
        cache_creation_tokens: 0,
        created_at,
    };
    let conn = lock_conn!(db.conn);
    if should_skip_session_insert(&conn, request_id, &dedup_key)? {
        return Ok(false);
    }

    let usage = TokenUsage {
        input_tokens: turn.input_tokens,
        output_tokens: turn.output_tokens,
        cache_read_tokens: turn.cache_read_tokens,
        cache_creation_tokens: 0,
        model: Some(model.to_string()),
        message_id: None,
    };
    let (input_cost, output_cost, cache_read_cost, total_cost) =
        calculate_grok_session_costs(&conn, model, &usage);

    let inserted = conn
        .execute(
            "INSERT OR IGNORE INTO proxy_request_logs (
                request_id, provider_id, app_type, model, request_model, pricing_model,
                input_tokens, output_tokens, cache_read_tokens, cache_creation_tokens,
                input_cost_usd, output_cost_usd, cache_read_cost_usd, cache_creation_cost_usd,
                total_cost_usd, latency_ms, first_token_ms, status_code, error_message,
                session_id, provider_type, is_streaming, cost_multiplier, created_at, data_source
            ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, 0, ?, ?, ?, '0', ?, 0, NULL, 200, NULL, ?, ?, 1, '1.0', ?, ?)",
            rusqlite::params![
                request_id,
                GROK_SESSION_PROVIDER_ID,
                "grok",
                model,
                model,
                model,
                turn.input_tokens,
                turn.output_tokens,
                turn.cache_read_tokens,
                input_cost,
                output_cost,
                cache_read_cost,
                total_cost,
                turn.session_id,
                GROK_SESSION_DATA_SOURCE,
                created_at,
                GROK_SESSION_DATA_SOURCE,
            ],
        )
        .map_err(|error| AppError::Database(format!("插入 Grok 会话日志失败: {error}")))?;
    if inserted > 0 {
        crate::usage_events::notify_log_recorded();
    }
    Ok(inserted > 0)
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
    fn parse_grok_turn_includes_cached_prompt_and_reasoning() {
        let value = serde_json::json!({
            "ts": "2026-07-10T14:08:40.304Z",
            "sid": "session-1",
            "msg": "shell.turn.inference_done",
            "ctx": {
                "prompt_tokens": 13967,
                "cached_prompt_tokens": 11136,
                "completion_tokens": 75,
                "reasoning_tokens": 35,
                "loop_index": 1
            }
        });

        let usage = parse_grok_turn_usage(&value).expect("valid Grok turn usage");
        assert_eq!(usage.input_tokens, 13_967);
        assert_eq!(usage.cache_read_tokens, 11_136);
        assert_eq!(usage.output_tokens, 110);
        assert_eq!(usage.turn_number, Some(0));
    }

    #[test]
    fn sync_grok_usage_imports_unified_log_turn() -> Result<(), AppError> {
        let temp = tempfile::tempdir().expect("temp dir should be created");
        let grok_dir = temp.path().join(".grok");
        let session_id = "session-1";
        fs::create_dir_all(grok_dir.join("logs")).expect("logs directory should be created");
        fs::create_dir_all(grok_dir.join("sessions").join("project-a").join(session_id))
            .expect("session directory should be created");
        fs::write(
            grok_dir
                .join("sessions")
                .join("project-a")
                .join(session_id)
                .join("events.jsonl"),
            serde_json::json!({
                "type": "turn_started",
                "session_id": session_id,
                "turn_number": 0,
                "model_id": "grok-4.5"
            })
            .to_string(),
        )
        .expect("session events should be written");
        fs::write(
            grok_dir.join("logs").join("unified.jsonl"),
            serde_json::json!({
                "ts": "2026-07-10T14:08:40.304Z",
                "sid": session_id,
                "msg": "shell.turn.inference_done",
                "ctx": {
                    "prompt_tokens": 13967,
                    "cached_prompt_tokens": 11136,
                    "completion_tokens": 75,
                    "reasoning_tokens": 35,
                    "loop_index": 1
                }
            })
            .to_string(),
        )
        .expect("unified log should be written");
        let db = Database::memory()?;

        let result = sync_grok_usage_from_dir(&db, &grok_dir)?;
        assert_eq!(result.imported, 1);
        let conn = lock_conn!(db.conn);
        let row = conn.query_row(
            "SELECT app_type, model, input_tokens, output_tokens, cache_read_tokens, data_source
             FROM proxy_request_logs",
            [],
            |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, i64>(2)?,
                    row.get::<_, i64>(3)?,
                    row.get::<_, i64>(4)?,
                    row.get::<_, String>(5)?,
                ))
            },
        )?;
        assert_eq!(
            row,
            (
                "grok".to_string(),
                "grok-4.5".to_string(),
                13_967,
                110,
                11_136,
                "grok_session".to_string()
            )
        );

        Ok(())
    }

    #[test]
    fn sync_grok_usage_repairs_unknown_models_without_new_log_lines() -> Result<(), AppError> {
        let temp = tempfile::tempdir().expect("temp dir should be created");
        let grok_dir = temp.path().join(".grok");
        let session_id = "session-1";
        fs::create_dir_all(grok_dir.join("logs")).expect("logs directory should be created");
        fs::write(
            grok_dir.join("logs").join("unified.jsonl"),
            serde_json::json!({
                "ts": "2026-07-10T14:08:40.304Z",
                "sid": session_id,
                "msg": "shell.turn.inference_done",
                "ctx": {
                    "prompt_tokens": 13967,
                    "cached_prompt_tokens": 11136,
                    "completion_tokens": 75,
                    "reasoning_tokens": 35,
                    "loop_index": 1
                }
            })
            .to_string(),
        )
        .expect("unified log should be written");
        let db = Database::memory()?;
        {
            let conn = lock_conn!(db.conn);
            conn.execute(
                "INSERT INTO model_pricing (
                    model_id, display_name, input_cost_per_million, output_cost_per_million,
                    cache_read_cost_per_million, cache_creation_cost_per_million
                ) VALUES ('grok-4.5', 'Grok 4.5', '2', '6', '0.5', '0')",
                [],
            )?;
        }

        let first_sync = sync_grok_usage_from_dir(&db, &grok_dir)?;
        assert_eq!(first_sync.imported, 1);
        assert_eq!(first_sync.updated, 0);

        fs::create_dir_all(grok_dir.join("sessions").join("project-a").join(session_id))
            .expect("session directory should be created");
        fs::write(
            grok_dir
                .join("sessions")
                .join("project-a")
                .join(session_id)
                .join("events.jsonl"),
            serde_json::json!({
                "type": "turn_started",
                "session_id": session_id,
                "turn_number": 0,
                "model_id": "grok-4.5"
            })
            .to_string(),
        )
        .expect("session events should be written");

        // unified.jsonl 未新增内容，仍应根据后到的 events.jsonl 修复历史 unknown 行。
        let repaired_sync = sync_grok_usage_from_dir(&db, &grok_dir)?;
        assert_eq!(repaired_sync.imported, 0);
        assert_eq!(repaired_sync.updated, 1);

        let conn = lock_conn!(db.conn);
        let (model, request_model, pricing_model, total_cost): (
            String,
            String,
            Option<String>,
            String,
        ) = conn.query_row(
            "SELECT model, request_model, pricing_model, total_cost_usd
             FROM proxy_request_logs",
            [],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?)),
        )?;
        assert_eq!(model, "grok-4.5");
        assert_eq!(request_model, "grok-4.5");
        assert_eq!(pricing_model.as_deref(), Some("grok-4.5"));
        assert_eq!(
            total_cost.parse::<Decimal>().expect("valid total cost"),
            Decimal::new(11_890, 6)
        );

        Ok(())
    }

    #[test]
    fn sync_grok_usage_prices_each_turn_with_its_own_model() -> Result<(), AppError> {
        let temp = tempfile::tempdir().expect("temp dir should be created");
        let grok_dir = temp.path().join(".grok");
        let session_id = "session-switch";
        let session_dir = grok_dir.join("sessions").join("project-a").join(session_id);
        fs::create_dir_all(grok_dir.join("logs")).expect("logs directory should be created");
        fs::create_dir_all(&session_dir).expect("session directory should be created");
        fs::write(
            session_dir.join("events.jsonl"),
            format!(
                "{}\n{}",
                serde_json::json!({
                    "type": "turn_started",
                    "session_id": session_id,
                    "turn_number": 0,
                    "model_id": "grok-4.3"
                }),
                serde_json::json!({
                    "type": "turn_started",
                    "session_id": session_id,
                    "turn_number": 1,
                    "model_id": "grok-4.5"
                })
            ),
        )
        .expect("session events should be written");
        fs::write(
            grok_dir.join("logs").join("unified.jsonl"),
            format!(
                "{}\n{}",
                serde_json::json!({
                    "ts": "2026-07-10T14:08:40.304Z",
                    "sid": session_id,
                    "msg": "shell.turn.inference_done",
                    "ctx": { "prompt_tokens": 100, "loop_index": 1 }
                }),
                serde_json::json!({
                    "ts": "2026-07-10T14:09:40.304Z",
                    "sid": session_id,
                    "msg": "shell.turn.inference_done",
                    "ctx": { "prompt_tokens": 200, "loop_index": 2 }
                })
            ),
        )
        .expect("unified log should be written");
        let db = Database::memory()?;

        let result = sync_grok_usage_from_dir(&db, &grok_dir)?;
        assert_eq!(result.imported, 2);
        let conn = lock_conn!(db.conn);
        let models = {
            let mut statement =
                conn.prepare("SELECT model FROM proxy_request_logs ORDER BY request_id")?;
            let rows = statement
                .query_map([], |row| row.get::<_, String>(0))?
                .collect::<Result<Vec<_>, _>>()?;
            rows
        };
        assert_eq!(models, ["grok-4.3", "grok-4.5"]);

        Ok(())
    }

    #[test]
    fn sync_grok_usage_rescans_rotated_unified_log() -> Result<(), AppError> {
        let temp = tempfile::tempdir().expect("temp dir should be created");
        let grok_dir = temp.path().join(".grok");
        let session_id = "session-rotation";
        let session_dir = grok_dir.join("sessions").join("project-a").join(session_id);
        fs::create_dir_all(grok_dir.join("logs")).expect("logs directory should be created");
        fs::create_dir_all(&session_dir).expect("session directory should be created");
        fs::write(
            session_dir.join("events.jsonl"),
            format!(
                "{}\n{}",
                serde_json::json!({
                    "type": "turn_started",
                    "session_id": session_id,
                    "turn_number": 0,
                    "model_id": "grok-4.5"
                }),
                serde_json::json!({
                    "type": "turn_started",
                    "session_id": session_id,
                    "turn_number": 1,
                    "model_id": "grok-4.5"
                })
            ),
        )
        .expect("session events should be written");
        let log_path = grok_dir.join("logs").join("unified.jsonl");
        fs::write(
            &log_path,
            serde_json::json!({
                "ts": "2026-07-10T14:08:40.304Z",
                "sid": session_id,
                "msg": "shell.turn.inference_done",
                "ctx": { "prompt_tokens": 100, "loop_index": 1 }
            })
            .to_string(),
        )
        .expect("initial unified log should be written");
        let db = Database::memory()?;
        assert_eq!(sync_grok_usage_from_dir(&db, &grok_dir)?.imported, 1);

        // 替换为更大的新文件，但新推理事件仍位于第一行，旧偏移不能继续沿用。
        fs::write(
            &log_path,
            format!(
                "{}\n{}",
                serde_json::json!({
                    "ts": "2026-07-10T14:09:40.304Z",
                    "sid": session_id,
                    "msg": "shell.turn.inference_done",
                    "ctx": { "prompt_tokens": 200, "loop_index": 2 }
                }),
                serde_json::json!({ "msg": "rotation-padding", "payload": "x".repeat(4096) })
            ),
        )
        .expect("rotated unified log should be written");

        let result = sync_grok_usage_from_dir(&db, &grok_dir)?;
        assert_eq!(result.imported, 1);
        let conn = lock_conn!(db.conn);
        let count: i64 = conn.query_row("SELECT COUNT(*) FROM proxy_request_logs", [], |row| {
            row.get(0)
        })?;
        assert_eq!(count, 2);

        Ok(())
    }
}
