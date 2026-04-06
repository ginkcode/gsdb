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
    Cancelled,
}

impl std::fmt::Display for DbError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            DbError::Sqlx(e) => write!(f, "{e}"),
            DbError::Config(e) => write!(f, "{e}"),
            DbError::Cancelled => write!(f, "Import cancelled"),
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
            DbError::Cancelled => false,
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

    async fn drop_database(&self, db_name: &str) -> Result<(), DbError>;

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

    /// Returns SQL DDL for custom types (ENUMs, DOMAINs, etc.) that must be emitted
    /// before CREATE TABLE statements. Default: empty (most drivers have no custom types).
    async fn get_custom_types_sql(&self) -> Result<String, DbError> {
        Ok(String::new())
    }

    /// Returns SQL to ensure custom types used by a specific table exist.
    /// Uses a safe "create if not exists" pattern so it can be run against a
    /// database that may already have the type. Default: empty.
    async fn get_table_custom_types_sql(&self, _table_name: &str) -> Result<String, DbError> {
        Ok(String::new())
    }

    /// Returns ALTER TABLE … ADD CONSTRAINT … FOREIGN KEY … statements for all FK
    /// constraints in the database. Emitted after all CREATE TABLE statements so that
    /// referenced tables are guaranteed to exist. Default: empty.
    async fn get_fk_constraints_sql(&self) -> Result<String, DbError> {
        Ok(String::new())
    }

    /// Execute all `main` statements on a **single database connection**, calling
    /// `on_stmt_done` after each one. `on_stmt_done` returns `true` to continue or
    /// `false` to cancel (triggers rollback via `on_error` then returns `DbError::Cancelled`).
    ///
    /// If any statement fails, runs each `on_error` statement on the same connection
    /// (ignoring errors) then returns the failure.
    ///
    /// This is required for imports: session variables (FK-check disable) and DDL
    /// visibility within a transaction are connection-scoped, not pool-scoped.
    ///
    /// The default delegates to `run_query` (no connection sharing). Each driver that
    /// uses a pool **must** override this with `pool.acquire()` to hold one connection.
    async fn import_all_statements(
        &self,
        main: Vec<String>,
        on_error: Vec<String>,
        on_stmt_done: Box<dyn FnMut() -> bool + Send>,
    ) -> Result<usize, DbError> {
        let mut on_stmt_done = on_stmt_done;
        let mut count = 0;
        for (idx, stmt) in main.iter().enumerate() {
            if let Err(e) = self.run_query(stmt).await {
                for s in &on_error {
                    let _ = self.run_query(s).await;
                }
                return Err(stmt_error(e, idx + 1, stmt));
            }
            count += 1;
            if !on_stmt_done() {
                for s in &on_error {
                    let _ = self.run_query(s).await;
                }
                return Err(DbError::Cancelled);
            }
        }
        Ok(count)
    }

    /// Close the connection and release resources.
    /// For connection pools (Postgres, MySQL, SQLite), this is a no-op.
    /// For SQL Server (single connection), this properly closes the TCP connection.
    async fn close(&self) -> Result<(), DbError> {
        // Default: no-op for connection pools that handle their own cleanup
        Ok(())
    }
}

// ── Export options ────────────────────────────────────────────────────────────

#[derive(Debug, Clone, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TableExportOptions {
    pub name: String,
    pub include_structure: bool,
    pub include_data: bool,
}

// ── Export / Import progress events ──────────────────────────────────────────

#[derive(Debug, Clone, serde::Serialize)]
#[serde(tag = "type", rename_all = "camelCase")]
#[allow(dead_code)]
pub enum ExportProgress {
    #[serde(rename_all = "camelCase")]
    Started { total_tables: usize },
    #[serde(rename_all = "camelCase")]
    Table { name: String, index: usize, total: usize },
    Done,
    Error { message: String },
}

#[derive(Debug, Clone, serde::Serialize)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum ImportProgress {
    #[serde(rename_all = "camelCase")]
    Progress { done: usize, total: usize },
    #[serde(rename_all = "camelCase")]
    Done { count: usize },
    Error { message: String },
    Cancelled,
}

/// Wraps a DbError with statement position and a SQL snippet for user-facing error messages.
pub fn stmt_error(e: DbError, stmt_num: usize, stmt: &str) -> DbError {
    const SNIPPET_LEN: usize = 120;
    let snippet = if stmt.len() > SNIPPET_LEN {
        format!("{}…", &stmt[..SNIPPET_LEN])
    } else {
        stmt.to_string()
    };
    DbError::Config(format!(
        "{}\n\nFailed at statement {}:\n{}",
        e, stmt_num, snippet
    ))
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
