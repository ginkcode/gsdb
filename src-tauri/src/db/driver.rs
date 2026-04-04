use std::collections::HashMap;

use async_trait::async_trait;
use serde_json::Value;
use tokio::sync::mpsc;

use super::types::{QueryResult, SchemaGraph, TableInfo};

// ── Streaming update sent from driver → Tauri channel ────────────────────────

/// Tagged-union streamed from `stream_query` to the frontend via Tauri Channel.
/// Variants arrive in this order: Header → Rows* → Done | Error | Cancelled.
#[derive(Debug, Clone, serde::Serialize)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum StreamUpdate {
    #[serde(rename_all = "camelCase")]
    Header {
        columns: Vec<String>,
        column_types: Vec<String>,
        column_nullable: Vec<bool>,
    },
    Rows {
        rows: Vec<HashMap<String, Value>>,
    },
    #[serde(rename_all = "camelCase")]
    Done {
        rows_affected: Option<u64>,
    },
    /// Sent by the command layer when cancel_query fires; never by the driver.
    Cancelled,
    Error {
        message: String,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Dialect {
    Postgres,
    Mysql,
    Sqlite,
    SqlServer,
}

#[derive(Debug)]
pub enum DbError {
    Sqlx(sqlx::Error),
    Config(String),
}

impl std::fmt::Display for DbError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            DbError::Sqlx(e) => write!(f, "{e}"),
            DbError::Config(e) => write!(f, "{e}"),
        }
    }
}

impl std::error::Error for DbError {}

impl From<sqlx::Error> for DbError {
    fn from(e: sqlx::Error) -> Self {
        DbError::Sqlx(e)
    }
}

impl DbError {
    /// Returns true for network/transport failures where reconnecting may help.
    /// Returns false for database-level errors (syntax, constraints, permissions, etc.).
    pub fn is_connection_error(&self) -> bool {
        match self {
            // sqlx surfaces IO failures as Io; PoolClosed means the pool was explicitly shut down.
            // PoolTimedOut means all connections are busy — NOT a dead connection, don't retry.
            DbError::Sqlx(e) => matches!(e, sqlx::Error::Io(_) | sqlx::Error::PoolClosed),
            // Tiberius (SQL Server) and SSH tunnel errors are all wrapped as Config strings.
            // Only match OS/transport-level patterns; SQL server errors (syntax, constraints, etc.)
            // look nothing like these.
            DbError::Config(s) => {
                let m = s.to_lowercase();
                m.contains("broken pipe")
                    || m.contains("connection reset")
                    || m.contains("connection refused")
                    || m.contains("connection aborted")
                    || m.contains("eof")
                    || m.contains("i/o error")
                    || m.contains("transport error")
                    || m.contains("channel open failed")
            }
        }
    }
}

#[async_trait]
pub trait Driver: Send + Sync {
    fn dialect(&self) -> Dialect;

    async fn run_query(&self, sql: &str) -> Result<QueryResult, DbError>;

    async fn list_tables(&self) -> Result<Vec<TableInfo>, DbError>;

    async fn list_databases(&self) -> Result<Vec<String>, DbError>;

    async fn get_column_nullable(&self, table_name: &str)
        -> Result<HashMap<String, bool>, DbError>;

    async fn create_database(&self, db_name: &str) -> Result<(), DbError>;

    async fn get_table_definition(&self, table_name: &str) -> Result<String, DbError>;

    async fn get_schema(&self) -> Result<SchemaGraph, DbError>;

    async fn get_server_info(&self) -> Result<ServerInfo, DbError>;

    /// Stream query results row-by-row via an mpsc sender.
    /// Send Header first, then one or more Rows batches, then Done (or Error).
    /// The default implementation collects everything first and sends as one batch —
    /// override in each driver for true incremental streaming.
    async fn stream_query(
        &self,
        sql: &str,
        tx: mpsc::Sender<StreamUpdate>,
    ) -> Result<(), DbError> {
        let result = self.run_query(sql).await?;
        tx.send(StreamUpdate::Header {
            columns: result.columns.clone(),
            column_types: result.column_types.clone(),
            column_nullable: result.column_nullable.clone(),
        })
        .await
        .ok();
        if !result.rows.is_empty() {
            tx.send(StreamUpdate::Rows { rows: result.rows }).await.ok();
        }
        tx.send(StreamUpdate::Done {
            rows_affected: result.rows_affected,
        })
        .await
        .ok();
        Ok(())
    }

    /// Close the connection and release resources.
    /// For connection pools (Postgres, MySQL, SQLite), this is a no-op.
    /// For SQL Server (single connection), this properly closes the TCP connection.
    async fn close(&self) -> Result<(), DbError> {
        // Default: no-op for connection pools that handle their own cleanup
        Ok(())
    }
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct ServerInfo {
    pub version: Option<String>,
    pub database_name: Option<String>,
    pub connections: Option<i64>,
    pub size: Option<String>,
    pub host: Option<String>,
    pub port: Option<i32>,
    pub uptime: Option<String>,
    pub extra: Vec<(String, String)>,
}
