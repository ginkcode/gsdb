pub mod types;

mod ssh;
mod postgres;
mod mysql;
mod sqlite;
mod export;

pub use types::{Connection, QueryResult, TableInfo};
pub use ssh::SshTunnel;

use postgres::pg_query;
use mysql::mysql_query;
use sqlite::sqlite_query;

// ── Driver-specific pool enum ─────────────────────────────────────────────────

#[derive(Clone)]
pub enum DbPool {
    Postgres(sqlx::PgPool),
    Mysql(sqlx::MySqlPool),
    Sqlite(sqlx::SqlitePool),
}

impl DbPool {
    pub async fn connect(conn: &Connection) -> Result<Self, sqlx::Error> {
        let url = if let Some(ssh) = &conn.ssh {
            let target_host = conn.host.clone().unwrap_or_else(|| "localhost".to_string());
            let target_port = conn.port.unwrap_or(match conn.driver.as_str() {
                "postgres" => 5432,
                "mysql" => 3306,
                _ => 0,
            });

            let ssh_config = ssh.clone();
            let tunnel = tokio::task::spawn_blocking(move || {
                SshTunnel::create(&ssh_config, &target_host, target_port)
            })
            .await
            .map_err(|e| sqlx::Error::Configuration(e.to_string().into()))?
            .map_err(|e| sqlx::Error::Configuration(e.into()))?;

            let local_port = tunnel.local_port();
            let _ = tunnel; // keep alive
            conn.to_url_via_tunnel(local_port)
        } else {
            conn.to_url()
        };

        match conn.driver.as_str() {
            "postgres" => Ok(DbPool::Postgres(sqlx::PgPool::connect(&url).await?)),
            "mysql" => Ok(DbPool::Mysql(sqlx::MySqlPool::connect(&url).await?)),
            "sqlite" => Ok(DbPool::Sqlite(sqlx::SqlitePool::connect(&url).await?)),
            d => Err(sqlx::Error::Configuration(
                format!("unknown driver: {d}").into(),
            )),
        }
    }

    pub async fn run_query(&self, sql: &str) -> Result<QueryResult, sqlx::Error> {
        match self {
            DbPool::Postgres(pool) => pg_query(pool, sql).await,
            DbPool::Mysql(pool) => mysql_query(pool, sql).await,
            DbPool::Sqlite(pool) => sqlite_query(pool, sql).await,
        }
    }

    pub async fn list_tables(&self) -> Result<Vec<TableInfo>, sqlx::Error> {
        match self {
            DbPool::Postgres(pool) => {
                let rows = sqlx::query(
                    "SELECT table_name, table_type FROM information_schema.tables \
                     WHERE table_schema = 'public' AND table_type IN ('BASE TABLE', 'VIEW') \
                     ORDER BY table_name",
                )
                .fetch_all(pool)
                .await?;
                Ok(rows
                    .iter()
                    .map(|r| TableInfo {
                        name: r.try_get::<String, _>(0).unwrap_or_default(),
                        kind: if r.try_get::<String, _>(1).unwrap_or_default() == "VIEW" {
                            "view".to_string()
                        } else {
                            "table".to_string()
                        },
                    })
                    .collect())
            }
            DbPool::Mysql(pool) => {
                let rows = sqlx::query(
                    "SELECT TABLE_NAME, TABLE_TYPE FROM information_schema.TABLES \
                     WHERE TABLE_SCHEMA = DATABASE() AND TABLE_TYPE IN ('BASE TABLE', 'VIEW') \
                     ORDER BY TABLE_NAME",
                )
                .fetch_all(pool)
                .await?;
                Ok(rows
                    .iter()
                    .map(|r| TableInfo {
                        name: r.try_get::<String, _>(0).unwrap_or_else(|_| {
                            r.try_get::<Vec<u8>, _>(0)
                                .map(|b| String::from_utf8_lossy(&b).into_owned())
                                .unwrap_or_default()
                        }),
                        kind: if r.try_get::<String, _>(1).unwrap_or_default() == "VIEW" {
                            "view".to_string()
                        } else {
                            "table".to_string()
                        },
                    })
                    .collect())
            }
            DbPool::Sqlite(pool) => {
                let rows = sqlx::query(
                    "SELECT name, type FROM sqlite_master \
                     WHERE type IN ('table', 'view') ORDER BY name",
                )
                .fetch_all(pool)
                .await?;
                Ok(rows
                    .iter()
                    .map(|r| TableInfo {
                        name: r.try_get::<String, _>(0).unwrap_or_default(),
                        kind: r.try_get::<String, _>(1).unwrap_or_default(),
                    })
                    .collect())
            }
        }
    }

    pub async fn list_databases(&self) -> Result<Vec<String>, sqlx::Error> {
        match self {
            DbPool::Postgres(pool) => {
                let rows = sqlx::query(
                    "SELECT datname FROM pg_database \
                     WHERE datistemplate = false ORDER BY datname",
                )
                .fetch_all(pool)
                .await?;
                Ok(rows
                    .iter()
                    .map(|r| r.try_get::<String, _>(0).unwrap_or_default())
                    .collect())
            }
            DbPool::Mysql(pool) => {
                let rows = sqlx::query(
                    "SELECT SCHEMA_NAME FROM information_schema.SCHEMATA ORDER BY SCHEMA_NAME",
                )
                .fetch_all(pool)
                .await?;
                Ok(rows
                    .iter()
                    .map(|r| {
                        r.try_get::<String, _>(0).unwrap_or_else(|_| {
                            r.try_get::<Vec<u8>, _>(0)
                                .map(|b| String::from_utf8_lossy(&b).into_owned())
                                .unwrap_or_default()
                        })
                    })
                    .collect())
            }
            DbPool::Sqlite(_) => Ok(vec![]),
        }
    }

    /// Returns a map of column_name → is_nullable for a given table.
    /// Used to enrich QueryResult.column_nullable on the frontend.
    pub async fn get_column_nullable(
        &self,
        table_name: &str,
    ) -> Result<std::collections::HashMap<String, bool>, sqlx::Error> {
        match self {
            DbPool::Postgres(pool) => {
                let rows = sqlx::query(
                    "SELECT column_name, is_nullable \
                     FROM information_schema.columns \
                     WHERE table_schema = 'public' AND table_name = $1",
                )
                .bind(table_name)
                .fetch_all(pool)
                .await?;
                Ok(rows
                    .iter()
                    .filter_map(|r| {
                        let col: String = r.try_get(0).ok()?;
                        let nullable: String = r.try_get(1).ok()?;
                        Some((col, nullable == "YES"))
                    })
                    .collect())
            }
            DbPool::Mysql(pool) => {
                let rows = sqlx::query(
                    "SELECT COLUMN_NAME, IS_NULLABLE \
                     FROM information_schema.COLUMNS \
                     WHERE TABLE_SCHEMA = DATABASE() AND TABLE_NAME = ?",
                )
                .bind(table_name)
                .fetch_all(pool)
                .await?;
                Ok(rows
                    .iter()
                    .filter_map(|r| {
                        let col = r
                            .try_get::<String, _>(0)
                            .or_else(|_| {
                                r.try_get::<Vec<u8>, _>(0)
                                    .map(|b| String::from_utf8_lossy(&b).into_owned())
                            })
                            .ok()?;
                        let nullable = r
                            .try_get::<String, _>(1)
                            .or_else(|_| {
                                r.try_get::<Vec<u8>, _>(1)
                                    .map(|b| String::from_utf8_lossy(&b).into_owned())
                            })
                            .ok()?;
                        Some((col, nullable == "YES"))
                    })
                    .collect())
            }
            DbPool::Sqlite(pool) => {
                let rows =
                    sqlx::query(&format!("PRAGMA table_info(\"{}\")", table_name))
                        .fetch_all(pool)
                        .await?;
                // PRAGMA table_info columns: cid, name, type, notnull, dflt_value, pk
                Ok(rows
                    .iter()
                    .filter_map(|r| {
                        let col: String = r.try_get(1).ok()?;
                        let not_null: i64 = r.try_get(3).ok()?;
                        Some((col, not_null == 0))
                    })
                    .collect())
            }
        }
    }

    pub async fn create_database(&self, db_name: &str) -> Result<(), sqlx::Error> {
        // Validate name to prevent SQL injection (only allow alphanumeric, underscore, hyphen)
        if !db_name
            .chars()
            .all(|c| c.is_alphanumeric() || c == '_' || c == '-')
        {
            return Err(sqlx::Error::Protocol("Invalid database name".into()));
        }
        let sql = match self {
            DbPool::Postgres(_) => format!("CREATE DATABASE \"{}\"", db_name),
            DbPool::Mysql(_) => format!("CREATE DATABASE `{}`", db_name),
            DbPool::Sqlite(_) => return Ok(()), // SQLite creates on connect
        };
        self.run_query(&sql).await?;
        Ok(())
    }

    pub async fn get_table_definition(&self, table_name: &str) -> Result<String, sqlx::Error> {
        match self {
            DbPool::Postgres(pool) => {
                let rows = sqlx::query(
                    "SELECT column_name, udt_name, is_nullable, column_default, \
                     character_maximum_length, numeric_precision, numeric_scale \
                     FROM information_schema.columns \
                     WHERE table_schema = 'public' AND table_name = $1 \
                     ORDER BY ordinal_position",
                )
                .bind(table_name)
                .fetch_all(pool)
                .await?;

                let mut columns: Vec<String> = Vec::new();
                for row in rows {
                    let col_name: String = row.try_get(0)?;
                    let udt_name: String = row.try_get(1)?;
                    let is_nullable: String = row.try_get(2)?;
                    let column_default: Option<String> = row.try_get(3)?;
                    let char_max_len: Option<i32> = row.try_get(4)?;
                    let num_precision: Option<i32> = row.try_get(5)?;
                    let num_scale: Option<i32> = row.try_get(6)?;

                    // Build type string using udt_name (PostgreSQL internal aliases like
                    // int4, int8, bool, timestamp) — only append modifiers for types
                    // that genuinely require them (varchar length, numeric precision/scale).
                    let full_type = match udt_name.as_str() {
                        "varchar" | "bpchar" => {
                            if let Some(len) = char_max_len {
                                format!("{}({})", udt_name, len)
                            } else {
                                udt_name.clone()
                            }
                        }
                        "numeric" => match (num_precision, num_scale) {
                            (Some(prec), Some(scale)) if scale > 0 => {
                                format!("{}({}, {})", udt_name, prec, scale)
                            }
                            (Some(prec), _) => format!("{}({})", udt_name, prec),
                            _ => udt_name.clone(),
                        },
                        _ => udt_name.clone(),
                    };

                    let mut col_def = format!("    \"{}\" {}", col_name, full_type);
                    if is_nullable == "NO" {
                        col_def.push_str(" NOT NULL");
                    }
                    if let Some(default) = column_default {
                        col_def.push_str(&format!(" DEFAULT {}", default));
                    }
                    columns.push(col_def);
                }

                // Get primary key
                let pk_rows = sqlx::query(
                    "SELECT a.attname \
                     FROM pg_index i \
                     JOIN pg_attribute a ON a.attrelid = i.indrelid AND a.attnum = ANY(i.indkey) \
                     WHERE i.indrelid = $1::regclass AND i.indisprimary",
                )
                .bind(format!("\"public\".\"{}\"", table_name))
                .fetch_all(pool)
                .await?;

                let pk_columns: Vec<String> = pk_rows
                    .iter()
                    .filter_map(|r| r.try_get::<String, _>(0).ok())
                    .collect();

                let mut result = format!(
                    "-- Table Definition\nCREATE TABLE \"public\".\"{}\" (\n{}\n);",
                    table_name,
                    columns.join(",\n")
                );

                if !pk_columns.is_empty() {
                    let pk_def = format!(
                        ",\n    PRIMARY KEY ({})",
                        pk_columns
                            .iter()
                            .map(|c| format!("\"{}\"", c))
                            .collect::<Vec<_>>()
                            .join(", ")
                    );
                    result = format!(
                        "-- Table Definition\nCREATE TABLE \"public\".\"{}\" (\n{}{}\n);",
                        table_name,
                        columns.join(",\n"),
                        pk_def
                    );
                }

                Ok(result)
            }
            DbPool::Mysql(pool) => {
                let rows = sqlx::query(&format!("SHOW CREATE TABLE `{}`", table_name))
                    .fetch_all(pool)
                    .await?;

                if let Some(row) = rows.first() {
                    let create_stmt: String = row.try_get(1)?;
                    Ok(format!(
                        "-- Table Definition\n{};",
                        create_stmt.trim_end_matches(';')
                    ))
                } else {
                    Err(sqlx::Error::RowNotFound)
                }
            }
            DbPool::Sqlite(pool) => {
                let rows = sqlx::query(
                    "SELECT sql FROM sqlite_master WHERE type IN ('table', 'view') AND name=?",
                )
                .bind(table_name)
                .fetch_all(pool)
                .await?;

                if let Some(row) = rows.first() {
                    let create_stmt: String = row.try_get(0)?;
                    Ok(format!(
                        "-- Table Definition\n{};",
                        create_stmt.trim_end_matches(';')
                    ))
                } else {
                    Err(sqlx::Error::RowNotFound)
                }
            }
        }
    }
}

// sqlx traits needed for try_get in list_tables / get_table_definition
use sqlx::Row;
