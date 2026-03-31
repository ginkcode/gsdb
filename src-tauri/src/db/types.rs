use serde::{Deserialize, Serialize};
use serde_json::Value;
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
    pub fn to_url(&self) -> String {
        match self.driver.as_str() {
            "postgres" => format!(
                "postgres://{}:{}@{}:{}/{}?sslmode={}",
                self.username.as_deref().unwrap_or(""),
                self.password.as_deref().unwrap_or(""),
                self.host.as_deref().unwrap_or("localhost"),
                self.port.unwrap_or(5432),
                self.database,
                self.ssl_mode.as_deref().unwrap_or("prefer")
            ),
            "mysql" => format!(
                "mysql://{}:{}@{}:{}/{}?ssl-mode={}",
                self.username.as_deref().unwrap_or(""),
                self.password.as_deref().unwrap_or(""),
                self.host.as_deref().unwrap_or("localhost"),
                self.port.unwrap_or(3306),
                self.database,
                self.ssl_mode.as_deref().unwrap_or("preferred")
            ),
            "sqlite" => format!(
                "sqlite://{}",
                self.file_path.as_deref().unwrap_or(&self.database)
            ),
            _ => String::new(),
        }
    }

    pub fn to_url_via_tunnel(&self, local_port: u16) -> String {
        match self.driver.as_str() {
            "postgres" => format!(
                "postgres://{}:{}@127.0.0.1:{}/{}?sslmode={}",
                self.username.as_deref().unwrap_or(""),
                self.password.as_deref().unwrap_or(""),
                local_port,
                self.database,
                self.ssl_mode.as_deref().unwrap_or("prefer")
            ),
            "mysql" => format!(
                "mysql://{}:{}@127.0.0.1:{}/{}?ssl-mode={}",
                self.username.as_deref().unwrap_or(""),
                self.password.as_deref().unwrap_or(""),
                local_port,
                self.database,
                self.ssl_mode.as_deref().unwrap_or("preferred")
            ),
            _ => self.to_url(),
        }
    }
}

// ── Query result ──────────────────────────────────────────────────────────────

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct QueryResult {
    pub columns: Vec<String>,
    pub column_types: Vec<String>,
    pub rows: Vec<HashMap<String, Value>>,
    pub rows_affected: Option<u64>,
}
