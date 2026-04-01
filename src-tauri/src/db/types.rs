use serde::{Deserialize, Serialize};
use serde_json::Value;
use sqlx::mysql::{MySqlConnectOptions, MySqlSslMode};
use sqlx::postgres::{PgConnectOptions, PgSslMode};
use std::collections::HashMap;

// ── Table info (table vs view) ────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize)]
pub struct TableInfo {
    pub name: String,
    pub kind: String, // "table" or "view"
}

// ── SSH Tunnel config ─────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SshConfig {
    pub host: String,
    pub port: u16,
    pub username: String,
    pub password: Option<String>,
    #[serde(rename = "privateKey")]
    pub private_key: Option<String>,
    #[serde(rename = "privateKeyPassphrase")]
    pub private_key_passphrase: Option<String>,
}

// ── Connection config ─────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Connection {
    pub id: String,
    pub name: String,
    pub driver: String,
    pub host: Option<String>,
    pub port: Option<u16>,
    pub database: String,
    pub username: Option<String>,
    pub password: Option<String>,
    #[serde(rename = "filePath")]
    pub file_path: Option<String>,
    pub ssh: Option<SshConfig>,
    #[serde(rename = "sslMode")]
    pub ssl_mode: Option<String>,
}

impl Connection {
    pub fn to_sqlite_url(&self) -> String {
        // Use rwc mode (read-write-create) to create the database if it doesn't exist
        format!(
            "sqlite://{}?mode=rwc",
            self.file_path.as_deref().unwrap_or(&self.database)
        )
    }

    pub fn pg_options(&self, host: &str, port: u16) -> PgConnectOptions {
        let ssl_mode = match self.ssl_mode.as_deref().unwrap_or("prefer") {
            "disable" => PgSslMode::Disable,
            "allow" => PgSslMode::Allow,
            "require" => PgSslMode::Require,
            "verify-ca" => PgSslMode::VerifyCa,
            "verify-full" => PgSslMode::VerifyFull,
            _ => PgSslMode::Prefer,
        };
        PgConnectOptions::new()
            .host(host)
            .port(port)
            .database(&self.database)
            .username(self.username.as_deref().unwrap_or(""))
            .password(self.password.as_deref().unwrap_or(""))
            .ssl_mode(ssl_mode)
    }

    pub fn mysql_options(&self, host: &str, port: u16) -> MySqlConnectOptions {
        let ssl_mode = match self.ssl_mode.as_deref().unwrap_or("preferred") {
            "disabled" => MySqlSslMode::Disabled,
            "required" => MySqlSslMode::Required,
            "verify-ca" => MySqlSslMode::VerifyCa,
            "verify-identity" => MySqlSslMode::VerifyIdentity,
            _ => MySqlSslMode::Preferred,
        };
        MySqlConnectOptions::new()
            .host(host)
            .port(port)
            .database(&self.database)
            .username(self.username.as_deref().unwrap_or(""))
            .password(self.password.as_deref().unwrap_or(""))
            .ssl_mode(ssl_mode)
    }
}

// ── Schema diagram types ──────────────────────────────────────────────────────

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SchemaColumn {
    pub name: String,
    pub col_type: String,
    pub pk: bool,
    pub nullable: bool,
}

#[derive(Debug, Serialize)]
pub struct SchemaTable {
    pub name: String,
    pub columns: Vec<SchemaColumn>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SchemaForeignKey {
    pub name: String,
    pub from_table: String,
    pub from_col: String,
    pub to_table: String,
    pub to_col: String,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SchemaGraph {
    pub tables: Vec<SchemaTable>,
    pub foreign_keys: Vec<SchemaForeignKey>,
}

// ── Query result ──────────────────────────────────────────────────────────────

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct QueryResult {
    pub columns: Vec<String>,
    pub column_types: Vec<String>,
    /// Per-column nullability. `true` = nullable, `false` = NOT NULL.
    /// Defaults to `true` when the information is not available (e.g. ad-hoc queries).
    pub column_nullable: Vec<bool>,
    pub rows: Vec<HashMap<String, Value>>,
    pub rows_affected: Option<u64>,
}
