use rusqlite::{params, Connection, OpenFlags};
use serde_json::Value;
use std::path::{Path, PathBuf};

pub const CODEX_PROVIDER_TOKEN_ARG: &str = "--codex-provider-token";
const DATABASE_ARG: &str = "--database";
const PROVIDER_ID_ARG: &str = "--provider-id";

#[derive(Debug, PartialEq, Eq)]
struct TokenRequest {
    database_path: PathBuf,
    provider_id: Option<String>,
}

fn parse_token_request(
    args: impl IntoIterator<Item = String>,
) -> Result<Option<TokenRequest>, String> {
    let mut args = args.into_iter();
    let Some(mode) = args.next() else {
        return Ok(None);
    };
    if mode != CODEX_PROVIDER_TOKEN_ARG {
        return Ok(None);
    }

    let mut database_path = None;
    let mut provider_id = None;
    while let Some(flag) = args.next() {
        let value = args
            .next()
            .ok_or_else(|| format!("missing value for {flag}"))?;
        match flag.as_str() {
            DATABASE_ARG => database_path = Some(PathBuf::from(value)),
            PROVIDER_ID_ARG => provider_id = Some(value),
            _ => return Err(format!("unknown Codex token helper argument: {flag}")),
        }
    }

    let database_path = database_path.ok_or_else(|| "missing --database path".to_string())?;
    Ok(Some(TokenRequest {
        database_path,
        provider_id,
    }))
}

fn read_provider_settings(
    database_path: &Path,
    provider_id: Option<&str>,
) -> Result<Value, String> {
    let connection = Connection::open_with_flags(database_path, OpenFlags::SQLITE_OPEN_READ_ONLY)
        .map_err(|error| format!("failed to open Agent Switch database: {error}"))?;

    let settings_json: String = if let Some(provider_id) = provider_id {
        connection
            .query_row(
                "SELECT settings_config FROM providers WHERE app_type = 'codex' AND id = ?1 LIMIT 1",
                params![provider_id],
                |row| row.get(0),
            )
            .map_err(|error| format!("failed to read Codex provider '{provider_id}': {error}"))?
    } else {
        connection
            .query_row(
                "SELECT settings_config FROM providers WHERE app_type = 'codex' AND is_current = 1 LIMIT 1",
                [],
                |row| row.get(0),
            )
            .map_err(|error| format!("failed to read the current Codex provider: {error}"))?
    };

    serde_json::from_str(&settings_json)
        .map_err(|error| format!("invalid Codex provider settings: {error}"))
}

fn read_provider_token(database_path: &Path, provider_id: Option<&str>) -> Result<String, String> {
    let settings = read_provider_settings(database_path, provider_id)?;
    let auth = settings.get("auth");
    let config_text = settings.get("config").and_then(Value::as_str);
    crate::codex_config::extract_codex_api_key(auth, config_text)
        .ok_or_else(|| "the Codex provider does not contain an API key".to_string())
}

/// Handle Codex's command-backed provider authentication before Tauri starts.
///
/// The helper prints only the bearer token to stdout, as required by Codex's
/// `[model_providers.<id>.auth]` contract. Returning `Some` tells the binary
/// entrypoint to exit without creating an app window or hitting single-instance
/// handling.
pub fn run_if_requested() -> Option<i32> {
    let request = match parse_token_request(std::env::args().skip(1)) {
        Ok(Some(request)) => request,
        Ok(None) => return None,
        Err(error) => {
            eprintln!("{error}");
            return Some(2);
        }
    };

    match read_provider_token(&request.database_path, request.provider_id.as_deref()) {
        Ok(token) => {
            println!("{token}");
            Some(0)
        }
        Err(error) => {
            eprintln!("{error}");
            Some(1)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_explicit_provider_request() {
        let request = parse_token_request([
            CODEX_PROVIDER_TOKEN_ARG.to_string(),
            DATABASE_ARG.to_string(),
            "/tmp/agentswitch.db".to_string(),
            PROVIDER_ID_ARG.to_string(),
            "deepseek".to_string(),
        ])
        .expect("parse request")
        .expect("token mode");

        assert_eq!(request.database_path, PathBuf::from("/tmp/agentswitch.db"));
        assert_eq!(request.provider_id.as_deref(), Some("deepseek"));
    }

    #[test]
    fn reads_provider_token_from_database() {
        let temp = tempfile::tempdir().expect("tempdir");
        let database_path = temp.path().join("agentswitch.db");
        let connection = Connection::open(&database_path).expect("open database");
        connection
            .execute_batch(
                "CREATE TABLE providers (id TEXT, app_type TEXT, settings_config TEXT, is_current INTEGER);",
            )
            .expect("create providers table");
        connection
            .execute(
                "INSERT INTO providers (id, app_type, settings_config, is_current) VALUES (?1, 'codex', ?2, 1)",
                params![
                    "deepseek",
                    r#"{"auth":{"OPENAI_API_KEY":"sk-test"},"config":"model_provider = \"custom\""}"#
                ],
            )
            .expect("insert provider");
        drop(connection);

        assert_eq!(
            read_provider_token(&database_path, Some("deepseek")).as_deref(),
            Ok("sk-test")
        );
    }
}
