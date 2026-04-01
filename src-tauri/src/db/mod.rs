pub mod types;
pub mod driver;

mod postgres;
mod mysql;
mod sqlite;
mod sqlserver;
mod ssh;
mod export;

pub use types::{Connection, QueryResult, SchemaGraph, TableInfo};
pub use driver::{DbError, Dialect, Driver};
pub use ssh::SshTunnel;

use std::collections::HashMap;
use std::sync::Arc;

use postgres::PostgresDriver;
use mysql::MySqlDriver;
use sqlite::SqliteDriver;
use sqlserver::SqlServerDriver;

// ── Public handle ─────────────────────────────────────────────────────────────

/// Cheaply cloneable handle to a database connection.
/// Wraps `Arc<dyn Driver>` so the enum variant is invisible to callers.
#[derive(Clone)]
pub struct DbPool(Arc<dyn Driver>);

impl DbPool {
    pub async fn connect(conn: &Connection) -> Result<Self, DbError> {
        // Resolve SSH tunnel if configured, yielding an optional local port.
        // For sqlx drivers we build a URL; for SQL Server we use host/port directly.
        let tunnel_port: Option<u16> = if let Some(ssh) = &conn.ssh {
            let target_host = conn.host.clone().unwrap_or_else(|| "localhost".to_string());
            let target_port = conn.port.unwrap_or(match conn.driver.as_str() {
                "postgres" => 5432,
                "mysql" => 3306,
                "sqlserver" => 1433,
                _ => 0,
            });
            let ssh_config = ssh.clone();
            let tunnel = tokio::task::spawn_blocking(move || {
                SshTunnel::create(&ssh_config, &target_host, target_port)
            })
            .await
            .map_err(|e| DbError::Config(e.to_string()))?
            .map_err(DbError::Config)?;

            let local_port = tunnel.local_port();
            let _ = tunnel;
            Some(local_port)
        } else {
            None
        };

        let driver: Arc<dyn Driver> = match conn.driver.as_str() {
            "postgres" => {
                let host = if tunnel_port.is_some() {
                    "127.0.0.1"
                } else {
                    conn.host.as_deref().unwrap_or("localhost")
                };
                let port = tunnel_port.unwrap_or_else(|| conn.port.unwrap_or(5432));
                Arc::new(PostgresDriver(
                    sqlx::PgPool::connect_with(conn.pg_options(host, port))
                        .await
                        .map_err(DbError::Sqlx)?,
                ))
            }
            "mysql" => {
                let host = if tunnel_port.is_some() {
                    "127.0.0.1"
                } else {
                    conn.host.as_deref().unwrap_or("localhost")
                };
                let port = tunnel_port.unwrap_or_else(|| conn.port.unwrap_or(3306));
                Arc::new(MySqlDriver(
                    sqlx::MySqlPool::connect_with(conn.mysql_options(host, port))
                        .await
                        .map_err(DbError::Sqlx)?,
                ))
            }
            "sqlite" => Arc::new(SqliteDriver(
                sqlx::SqlitePool::connect(&conn.to_sqlite_url())
                    .await
                    .map_err(DbError::Sqlx)?,
            )),
            "sqlserver" => {
                let host = if tunnel_port.is_some() {
                    "localhost"
                } else {
                    conn.host.as_deref().unwrap_or("localhost")
                };
                let port = tunnel_port.unwrap_or_else(|| conn.port.unwrap_or(1433));
                let trust_cert = conn.ssl_mode.as_deref() != Some("verify");
                Arc::new(
                    SqlServerDriver::connect(
                        host,
                        port,
                        &conn.database,
                        conn.username.as_deref().unwrap_or(""),
                        conn.password.as_deref().unwrap_or(""),
                        trust_cert,
                    )
                    .await?,
                )
            }
            d => return Err(DbError::Config(format!("unknown driver: {d}"))),
        };

        Ok(DbPool(driver))
    }

    pub fn dialect(&self) -> Dialect {
        self.0.dialect()
    }

    pub async fn run_query(&self, sql: &str) -> Result<QueryResult, DbError> {
        self.0.run_query(sql).await
    }

    pub async fn list_tables(&self) -> Result<Vec<TableInfo>, DbError> {
        self.0.list_tables().await
    }

    pub async fn list_databases(&self) -> Result<Vec<String>, DbError> {
        self.0.list_databases().await
    }

    pub async fn get_column_nullable(
        &self,
        table_name: &str,
    ) -> Result<HashMap<String, bool>, DbError> {
        self.0.get_column_nullable(table_name).await
    }

    pub async fn create_database(&self, db_name: &str) -> Result<(), DbError> {
        if !db_name
            .chars()
            .all(|c| c.is_alphanumeric() || c == '_' || c == '-')
        {
            return Err(DbError::Config("Invalid database name".into()));
        }
        self.0.create_database(db_name).await
    }

    pub async fn get_table_definition(&self, table_name: &str) -> Result<String, DbError> {
        self.0.get_table_definition(table_name).await
    }

    pub async fn get_schema(&self) -> Result<SchemaGraph, DbError> {
        self.0.get_schema().await
    }
}
