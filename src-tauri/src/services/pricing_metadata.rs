//! 模型定价远端元数据同步。
//!
//! `model_pricing` 始终是计价的唯一来源。本模块只从公开维护的元数据中增量更新
//! 仍等于内置基线或上一次远端值的模型，从而不会覆盖用户的手动价格。

use crate::database::{lock_conn, Database};
use crate::error::AppError;
use crate::services::usage_stats::clean_model_id_for_pricing;
use rusqlite::{params, Connection, OptionalExtension};
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use serde_json::Number;
use std::collections::BTreeMap;
use std::str::FromStr;

pub const PRICING_METADATA_SOURCE_URL: &str = crate::services::models_dev_cache::MODELS_DEV_API_URL;

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PricingMetadataSyncStatus {
    pub source_url: String,
    pub last_attempt_at: Option<i64>,
    pub last_success_at: Option<i64>,
    pub last_error: Option<String>,
    pub etag: Option<String>,
    pub last_added: u64,
    pub last_updated: u64,
    pub last_preserved: u64,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PricingMetadataSyncResult {
    /// `updated`、`not_modified` 或 `skipped`。
    pub outcome: String,
    pub added: u64,
    pub updated: u64,
    pub preserved: u64,
    pub backfilled_rows: u64,
    pub status: PricingMetadataSyncStatus,
}

#[derive(Debug, Deserialize)]
struct MetadataProvider {
    #[serde(default)]
    models: BTreeMap<String, MetadataModel>,
}

#[derive(Debug, Deserialize)]
struct MetadataModel {
    id: Option<String>,
    name: Option<String>,
    cost: Option<MetadataCost>,
}

#[derive(Debug, Deserialize)]
struct MetadataCost {
    input: Option<Number>,
    output: Option<Number>,
    cache_read: Option<Number>,
    cache_write: Option<Number>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct StoredPricing {
    display_name: String,
    input: String,
    output: String,
    cache_read: String,
    cache_creation: String,
}

#[derive(Debug, Clone, Default)]
struct SyncCounters {
    added: u64,
    updated: u64,
    preserved: u64,
}

type MetadataCatalog = BTreeMap<String, MetadataProvider>;

pub fn get_pricing_metadata_sync_status(
    db: &Database,
) -> Result<PricingMetadataSyncStatus, AppError> {
    let conn = lock_conn!(db.conn);
    get_pricing_metadata_sync_status_on_conn(&conn)
}

/// 启动和定时任务调用。成功同步后 24 小时内不会重复下载。
pub async fn refresh_pricing_metadata_if_due(
    db: &Database,
) -> Result<PricingMetadataSyncResult, AppError> {
    refresh_pricing_metadata(db, false).await
}

/// 手动刷新和后台刷新共用实现；手动刷新可跳过每日间隔。
pub async fn refresh_pricing_metadata(
    db: &Database,
    force: bool,
) -> Result<PricingMetadataSyncResult, AppError> {
    let current_status = get_pricing_metadata_sync_status(db)?;
    if force {
        crate::services::models_dev_cache::refresh_models_dev_cache(true).await?;
    }
    let cache = crate::services::models_dev_cache::get_models_dev_cache_for_use().await?;
    let cache_version = cache.fetched_at.to_string();
    if !force && current_status.etag.as_deref() == Some(cache_version.as_str()) {
        return Ok(PricingMetadataSyncResult {
            outcome: "not_modified".to_string(),
            added: 0,
            updated: 0,
            preserved: 0,
            backfilled_rows: 0,
            status: current_status,
        });
    }
    let catalog: MetadataCatalog = serde_json::from_value(cache.api.clone())
        .map_err(|error| AppError::Message(format!("解析 Models.dev 定价元数据失败: {error}")))?;
    let candidates = extract_pricing_candidates(&catalog, &cache.models);
    if candidates.is_empty() {
        return Err(AppError::Message(
            "Models.dev 缓存中没有可用的官方模型价格".to_string(),
        ));
    }

    let counters = {
        let conn = lock_conn!(db.conn);
        apply_pricing_candidates_on_conn(
            &conn,
            &candidates,
            Some(&cache_version),
            cache.fetched_at,
        )?
    };
    let backfilled_rows = db.backfill_missing_usage_costs()?;

    Ok(PricingMetadataSyncResult {
        outcome: "updated".to_string(),
        added: counters.added,
        updated: counters.updated,
        preserved: counters.preserved,
        backfilled_rows,
        status: get_pricing_metadata_sync_status(db)?,
    })
}

fn get_pricing_metadata_sync_status_on_conn(
    conn: &Connection,
) -> Result<PricingMetadataSyncStatus, AppError> {
    let status = conn
        .query_row(
            "SELECT etag, last_attempt_at, last_success_at, last_error,
                    last_added, last_updated, last_preserved
             FROM model_pricing_metadata_sync_state
             WHERE id = 1",
            [],
            |row| {
                Ok(PricingMetadataSyncStatus {
                    source_url: PRICING_METADATA_SOURCE_URL.to_string(),
                    etag: row.get(0)?,
                    last_attempt_at: row.get(1)?,
                    last_success_at: row.get(2)?,
                    last_error: row.get(3)?,
                    last_added: row.get::<_, i64>(4)? as u64,
                    last_updated: row.get::<_, i64>(5)? as u64,
                    last_preserved: row.get::<_, i64>(6)? as u64,
                })
            },
        )
        .optional()?;

    Ok(status.unwrap_or(PricingMetadataSyncStatus {
        source_url: PRICING_METADATA_SOURCE_URL.to_string(),
        last_attempt_at: None,
        last_success_at: None,
        last_error: None,
        etag: None,
        last_added: 0,
        last_updated: 0,
        last_preserved: 0,
    }))
}

fn extract_pricing_candidates(
    catalog: &MetadataCatalog,
    official_models: &serde_json::Value,
) -> BTreeMap<String, StoredPricing> {
    let mut candidates = BTreeMap::new();

    let Some(model_index) = official_models.as_object() else {
        return candidates;
    };
    for canonical_id in model_index.keys() {
        let Some((provider_id, official_model_id)) = canonical_id.split_once('/') else {
            continue;
        };
        let Some(provider) = catalog.get(provider_id) else {
            continue;
        };
        let Some(model) = provider.models.get(official_model_id) else {
            continue;
        };
        let model_id = clean_model_id_for_pricing(model.id.as_deref().unwrap_or(official_model_id));
        if model_id.is_empty() || candidates.contains_key(&model_id) {
            continue;
        }
        let Some(pricing) = pricing_from_metadata(model, &model_id) else {
            continue;
        };
        candidates.insert(model_id, pricing);
    }

    candidates
}

fn pricing_from_metadata(model: &MetadataModel, fallback_name: &str) -> Option<StoredPricing> {
    let cost = model.cost.as_ref()?;
    Some(StoredPricing {
        display_name: model
            .name
            .as_deref()
            .filter(|name| !name.trim().is_empty())
            .unwrap_or(fallback_name)
            .trim()
            .to_string(),
        input: required_non_negative_decimal(cost.input.as_ref())?,
        output: required_non_negative_decimal(cost.output.as_ref())?,
        cache_read: optional_non_negative_decimal(cost.cache_read.as_ref())?,
        cache_creation: optional_non_negative_decimal(cost.cache_write.as_ref())?,
    })
}

fn required_non_negative_decimal(value: Option<&Number>) -> Option<String> {
    value.and_then(non_negative_decimal)
}

fn optional_non_negative_decimal(value: Option<&Number>) -> Option<String> {
    match value {
        Some(value) => non_negative_decimal(value),
        None => Some("0".to_string()),
    }
}

fn non_negative_decimal(value: &Number) -> Option<String> {
    let value = value.to_string();
    let decimal = Decimal::from_str(&value).ok()?;
    (decimal >= Decimal::ZERO).then_some(value)
}

fn apply_pricing_candidates_on_conn(
    conn: &Connection,
    candidates: &BTreeMap<String, StoredPricing>,
    etag: Option<&str>,
    now: i64,
) -> Result<SyncCounters, AppError> {
    let tx = conn
        .unchecked_transaction()
        .map_err(|error| AppError::Database(format!("开启定价元数据同步事务失败: {error}")))?;
    let mut counters = SyncCounters::default();

    for (model_id, remote) in candidates {
        if is_remote_model_suppressed(&tx, model_id)? {
            continue;
        }

        let local = get_stored_pricing(&tx, "model_pricing", model_id)?;
        let builtin = get_stored_pricing(&tx, "model_pricing_builtin_baseline", model_id)?;
        let previous_remote = get_stored_pricing(&tx, "model_pricing_remote_values", model_id)?;

        match local {
            None => {
                insert_model_pricing(&tx, model_id, remote)?;
                counters.added += 1;
            }
            Some(local)
                if is_sync_managed_value(&local, builtin.as_ref(), previous_remote.as_ref()) =>
            {
                let next = StoredPricing {
                    display_name: select_display_name(
                        &local,
                        builtin.as_ref(),
                        previous_remote.as_ref(),
                        remote,
                    ),
                    ..remote.clone()
                };
                if local != next {
                    update_model_pricing(&tx, model_id, &next)?;
                    counters.updated += 1;
                }
            }
            Some(_) => counters.preserved += 1,
        }

        upsert_remote_pricing_value(&tx, model_id, remote, etag, now)?;
    }

    tx.execute(
        "INSERT INTO model_pricing_metadata_sync_state (
            id, etag, last_attempt_at, last_success_at, last_error,
            last_added, last_updated, last_preserved
         ) VALUES (1, ?1, ?2, ?2, NULL, ?3, ?4, ?5)
         ON CONFLICT(id) DO UPDATE SET
            etag = excluded.etag,
            last_attempt_at = excluded.last_attempt_at,
            last_success_at = excluded.last_success_at,
            last_error = NULL,
            last_added = excluded.last_added,
            last_updated = excluded.last_updated,
            last_preserved = excluded.last_preserved",
        params![
            etag,
            now,
            counters.added as i64,
            counters.updated as i64,
            counters.preserved as i64
        ],
    )?;
    tx.commit()
        .map_err(|error| AppError::Database(format!("提交定价元数据同步事务失败: {error}")))?;

    Ok(counters)
}

fn is_sync_managed_value(
    local: &StoredPricing,
    builtin: Option<&StoredPricing>,
    previous_remote: Option<&StoredPricing>,
) -> bool {
    builtin.is_some_and(|value| local.same_costs(value))
        || previous_remote.is_some_and(|value| local.same_costs(value))
}

fn select_display_name(
    local: &StoredPricing,
    builtin: Option<&StoredPricing>,
    previous_remote: Option<&StoredPricing>,
    remote: &StoredPricing,
) -> String {
    if builtin.is_some_and(|value| local.display_name == value.display_name)
        || previous_remote.is_some_and(|value| local.display_name == value.display_name)
    {
        remote.display_name.clone()
    } else {
        local.display_name.clone()
    }
}

impl StoredPricing {
    fn same_costs(&self, other: &Self) -> bool {
        self.input == other.input
            && self.output == other.output
            && self.cache_read == other.cache_read
            && self.cache_creation == other.cache_creation
    }
}

fn get_stored_pricing(
    conn: &Connection,
    table: &str,
    model_id: &str,
) -> Result<Option<StoredPricing>, AppError> {
    let query = format!(
        "SELECT display_name, input_cost_per_million, output_cost_per_million,
                cache_read_cost_per_million, cache_creation_cost_per_million
         FROM {table} WHERE model_id = ?1"
    );
    conn.query_row(&query, params![model_id], |row| {
        Ok(StoredPricing {
            display_name: row.get(0)?,
            input: row.get(1)?,
            output: row.get(2)?,
            cache_read: row.get(3)?,
            cache_creation: row.get(4)?,
        })
    })
    .optional()
    .map_err(AppError::from)
}

fn is_remote_model_suppressed(conn: &Connection, model_id: &str) -> Result<bool, AppError> {
    conn.query_row(
        "SELECT 1 FROM model_pricing_remote_suppressions WHERE model_id = ?1",
        params![model_id],
        |_| Ok(()),
    )
    .optional()
    .map(|value| value.is_some())
    .map_err(AppError::from)
}

fn insert_model_pricing(
    conn: &Connection,
    model_id: &str,
    pricing: &StoredPricing,
) -> Result<(), AppError> {
    conn.execute(
        "INSERT INTO model_pricing (
            model_id, display_name, input_cost_per_million, output_cost_per_million,
            cache_read_cost_per_million, cache_creation_cost_per_million
         ) VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
        params![
            model_id,
            pricing.display_name,
            pricing.input,
            pricing.output,
            pricing.cache_read,
            pricing.cache_creation
        ],
    )?;
    Ok(())
}

fn update_model_pricing(
    conn: &Connection,
    model_id: &str,
    pricing: &StoredPricing,
) -> Result<(), AppError> {
    conn.execute(
        "UPDATE model_pricing SET
            display_name = ?2,
            input_cost_per_million = ?3,
            output_cost_per_million = ?4,
            cache_read_cost_per_million = ?5,
            cache_creation_cost_per_million = ?6
         WHERE model_id = ?1",
        params![
            model_id,
            pricing.display_name,
            pricing.input,
            pricing.output,
            pricing.cache_read,
            pricing.cache_creation
        ],
    )?;
    Ok(())
}

fn upsert_remote_pricing_value(
    conn: &Connection,
    model_id: &str,
    pricing: &StoredPricing,
    etag: Option<&str>,
    now: i64,
) -> Result<(), AppError> {
    conn.execute(
        "INSERT INTO model_pricing_remote_values (
            model_id, display_name, input_cost_per_million, output_cost_per_million,
            cache_read_cost_per_million, cache_creation_cost_per_million, source_etag, updated_at
         ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)
         ON CONFLICT(model_id) DO UPDATE SET
            display_name = excluded.display_name,
            input_cost_per_million = excluded.input_cost_per_million,
            output_cost_per_million = excluded.output_cost_per_million,
            cache_read_cost_per_million = excluded.cache_read_cost_per_million,
            cache_creation_cost_per_million = excluded.cache_creation_cost_per_million,
            source_etag = excluded.source_etag,
            updated_at = excluded.updated_at",
        params![
            model_id,
            pricing.display_name,
            pricing.input,
            pricing.output,
            pricing.cache_read,
            pricing.cache_creation,
            etag,
            now
        ],
    )?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::database::Database;

    fn pricing(input: &str, output: &str) -> StoredPricing {
        StoredPricing {
            display_name: "Test Model".to_string(),
            input: input.to_string(),
            output: output.to_string(),
            cache_read: "0".to_string(),
            cache_creation: "0".to_string(),
        }
    }

    #[test]
    fn extracts_only_prices_from_the_official_model_index() {
        let catalog: MetadataCatalog = serde_json::from_str(
            r#"{
                "zhipuai": {"models": {"glm-5.2": {
                    "id": "glm-5.2", "name": "GLM-5.2",
                    "cost": {"input": 1.4, "output": 4.4, "cache_read": 0.26, "cache_write": 0}
                }}},
                "openrouter": {"models": {"glm-5.2": {
                    "cost": {"input": 99, "output": 99}
                }}}
            }"#,
        )
        .expect("valid metadata fixture");

        let models = serde_json::json!({
            "zhipuai/glm-5.2": { "name": "GLM-5.2" },
        });
        let candidates = extract_pricing_candidates(&catalog, &models);
        assert_eq!(candidates.len(), 1);
        assert_eq!(candidates["glm-5.2"].input, "1.4");
        assert_eq!(candidates["glm-5.2"].output, "4.4");
        assert_eq!(candidates["glm-5.2"].cache_read, "0.26");
    }

    #[test]
    fn updates_tracked_prices_and_preserves_manual_prices() {
        let conn = Connection::open_in_memory().expect("open database");
        Database::create_tables_on_conn(&conn).expect("create schema");
        conn.execute(
            "INSERT INTO model_pricing_builtin_baseline (
                model_id, display_name, input_cost_per_million, output_cost_per_million,
                cache_read_cost_per_million, cache_creation_cost_per_million
             ) VALUES ('tracked-model', 'Test Model', '1', '2', '0', '0')",
            [],
        )
        .expect("insert baseline");
        conn.execute(
            "INSERT INTO model_pricing (
                model_id, display_name, input_cost_per_million, output_cost_per_million,
                cache_read_cost_per_million, cache_creation_cost_per_million
             ) VALUES ('tracked-model', 'Test Model', '1', '2', '0', '0')",
            [],
        )
        .expect("insert current pricing");

        let candidates = BTreeMap::from([("tracked-model".to_string(), pricing("3", "4"))]);
        let first = apply_pricing_candidates_on_conn(&conn, &candidates, Some("etag-1"), 1)
            .expect("sync tracked price");
        assert_eq!(first.updated, 1);
        assert_eq!(
            get_stored_pricing(&conn, "model_pricing", "tracked-model")
                .expect("load pricing")
                .expect("pricing exists")
                .input,
            "3"
        );

        conn.execute(
            "UPDATE model_pricing SET input_cost_per_million = '99' WHERE model_id = 'tracked-model'",
            [],
        )
        .expect("apply manual price");
        let next = BTreeMap::from([("tracked-model".to_string(), pricing("5", "6"))]);
        let second = apply_pricing_candidates_on_conn(&conn, &next, Some("etag-2"), 2)
            .expect("sync manual price");
        assert_eq!(second.preserved, 1);
        assert_eq!(
            get_stored_pricing(&conn, "model_pricing", "tracked-model")
                .expect("load manual pricing")
                .expect("pricing exists")
                .input,
            "99"
        );
    }
}
