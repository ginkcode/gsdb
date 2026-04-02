use std::collections::HashMap;

use async_trait::async_trait;
use serde_json::Value;
use sqlx::{Column, Row, TypeInfo};

use super::driver::{DbError, Dialect, Driver, ServerInfo};
use super::types::{
    QueryResult, SchemaColumn, SchemaForeignKey, SchemaGraph, SchemaTable, TableInfo,
};

pub struct MySqlDriver(pub sqlx::MySqlPool);

#[async_trait]
impl Driver for MySqlDriver {
    fn dialect(&self) -> Dialect {
        Dialect::Mysql
    }

    async fn run_query(&self, sql: &str) -> Result<QueryResult, DbError> {
        Ok(mysql_query(&self.0, sql).await?)
    }

    async fn list_tables(&self) -> Result<Vec<TableInfo>, DbError> {
        let rows = sqlx::query(
            "SELECT TABLE_NAME, TABLE_TYPE FROM information_schema.TABLES \
             WHERE TABLE_SCHEMA = DATABASE() AND TABLE_TYPE IN ('BASE TABLE', 'VIEW') \
             ORDER BY TABLE_NAME",
        )
        .fetch_all(&self.0)
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

    async fn list_databases(&self) -> Result<Vec<String>, DbError> {
        let rows =
            sqlx::query("SELECT SCHEMA_NAME FROM information_schema.SCHEMATA ORDER BY SCHEMA_NAME")
                .fetch_all(&self.0)
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

    async fn get_column_nullable(
        &self,
        table_name: &str,
    ) -> Result<HashMap<String, bool>, DbError> {
        let rows = sqlx::query(
            "SELECT COLUMN_NAME, IS_NULLABLE \
             FROM information_schema.COLUMNS \
             WHERE TABLE_SCHEMA = DATABASE() AND TABLE_NAME = ?",
        )
        .bind(table_name)
        .fetch_all(&self.0)
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

    async fn create_database(&self, db_name: &str) -> Result<(), DbError> {
        self.run_query(&format!("CREATE DATABASE `{}`", db_name))
            .await?;
        Ok(())
    }

    async fn get_schema(&self) -> Result<SchemaGraph, DbError> {
        fn get_str(row: &sqlx::mysql::MySqlRow, idx: usize) -> String {
            row.try_get::<String, _>(idx).unwrap_or_else(|_| {
                row.try_get::<Vec<u8>, _>(idx)
                    .map(|b| String::from_utf8_lossy(&b).into_owned())
                    .unwrap_or_default()
            })
        }

        let col_rows = sqlx::query(
            "SELECT TABLE_NAME, COLUMN_NAME, DATA_TYPE, IS_NULLABLE, \
             CASE WHEN COLUMN_KEY = 'PRI' THEN 1 ELSE 0 END AS is_pk \
             FROM information_schema.COLUMNS \
             WHERE TABLE_SCHEMA = DATABASE() \
             ORDER BY TABLE_NAME, ORDINAL_POSITION",
        )
        .fetch_all(&self.0)
        .await?;

        let mut tables: Vec<SchemaTable> = Vec::new();
        for row in &col_rows {
            let tbl = get_str(row, 0);
            let col = SchemaColumn {
                name: get_str(row, 1),
                col_type: get_str(row, 2),
                nullable: get_str(row, 3) == "YES",
                pk: row.try_get::<i64, _>(4).unwrap_or(0) == 1,
            };
            if tables.last().map(|t: &SchemaTable| t.name.as_str()) == Some(tbl.as_str()) {
                tables.last_mut().unwrap().columns.push(col);
            } else {
                tables.push(SchemaTable {
                    name: tbl,
                    columns: vec![col],
                });
            }
        }

        let fk_rows = sqlx::query(
            "SELECT CONSTRAINT_NAME, TABLE_NAME, COLUMN_NAME, \
             REFERENCED_TABLE_NAME, REFERENCED_COLUMN_NAME \
             FROM information_schema.KEY_COLUMN_USAGE \
             WHERE TABLE_SCHEMA = DATABASE() \
             AND REFERENCED_TABLE_NAME IS NOT NULL",
        )
        .fetch_all(&self.0)
        .await?;

        let foreign_keys = fk_rows
            .iter()
            .map(|r| SchemaForeignKey {
                name: get_str(r, 0),
                from_table: get_str(r, 1),
                from_col: get_str(r, 2),
                to_table: get_str(r, 3),
                to_col: get_str(r, 4),
            })
            .collect();

        Ok(SchemaGraph {
            tables,
            foreign_keys,
        })
    }

    async fn get_table_definition(&self, table_name: &str) -> Result<String, DbError> {
        // First check if this is a view
        let view_row = sqlx::query(
            "SELECT TABLE_TYPE FROM information_schema.TABLES \
             WHERE TABLE_SCHEMA = DATABASE() AND TABLE_NAME = ?",
        )
        .bind(table_name)
        .fetch_optional(&self.0)
        .await?;

        let is_view = view_row
            .as_ref()
            .map(|r| {
                r.try_get::<String, _>(1).unwrap_or_default() == "VIEW"
                    || r.try_get::<Vec<u8>, _>(1)
                        .map(|b| String::from_utf8_lossy(&b).into_owned() == "VIEW")
                        .unwrap_or(false)
            })
            .unwrap_or(false);

        if is_view {
            // Use SHOW CREATE VIEW for views
            let rows = sqlx::query(&format!(
                "SHOW CREATE VIEW `{}`",
                table_name.replace('`', "``")
            ))
            .fetch_all(&self.0)
            .await?;
            if let Some(row) = rows.first() {
                let create_stmt: String = row.try_get(1)?;
                Ok(format!(
                    "-- View Definition\n{};",
                    create_stmt.trim_end_matches(';')
                ))
            } else {
                Err(DbError::Sqlx(sqlx::Error::RowNotFound))
            }
        } else {
            // Use SHOW CREATE TABLE for tables
            let rows = sqlx::query(&format!(
                "SHOW CREATE TABLE `{}`",
                table_name.replace('`', "``")
            ))
            .fetch_all(&self.0)
            .await?;
            if let Some(row) = rows.first() {
                let create_stmt: String = row.try_get(1)?;
                Ok(format!(
                    "-- Table Definition\n{};",
                    create_stmt.trim_end_matches(';')
                ))
            } else {
                Err(DbError::Sqlx(sqlx::Error::RowNotFound))
            }
        }
    }

    async fn get_server_info(&self) -> Result<ServerInfo, DbError> {
        // Get version
        let version_row = sqlx::query("SELECT VERSION()")
            .fetch_one(&self.0)
            .await
            .ok();
        let version = version_row.and_then(|r| r.try_get::<String, _>(0).ok());

        // Get current database
        let db_row = sqlx::query("SELECT DATABASE()")
            .fetch_one(&self.0)
            .await
            .ok();
        let database_name = db_row.and_then(|r| r.try_get::<String, _>(0).ok());

        // Get connection count
        let conn_row = sqlx::query("SHOW STATUS LIKE 'Threads_connected'")
            .fetch_one(&self.0)
            .await
            .ok();
        let connections = conn_row.and_then(|r| {
            r.try_get::<String, _>(1).ok().and_then(|s| s.parse::<i64>().ok())
        });

        // Get database size
        let size_row = sqlx::query(
            "SELECT ROUND(SUM(data_length + index_length) / 1024 / 1024, 2) AS size_mb \
             FROM information_schema.tables WHERE table_schema = DATABASE()"
        )
        .fetch_one(&self.0)
        .await
        .ok();
        let size = size_row.and_then(|r| {
            r.try_get::<f64, _>(0)
                .ok()
                .map(|s| format!("{:.2} MB", s))
        });

        // Get uptime
        let uptime_row = sqlx::query("SHOW STATUS LIKE 'Uptime'")
            .fetch_one(&self.0)
            .await
            .ok();
        let uptime = uptime_row.and_then(|r| {
            r.try_get::<String, _>(1).ok().and_then(|s| {
                s.parse::<i64>().ok().map(|secs| {
                    let days = secs / 86400;
                    let hours = (secs % 86400) / 3600;
                    let mins = (secs % 3600) / 60;
                    if days > 0 {
                        format!("{}d {}h {}m", days, hours, mins)
                    } else if hours > 0 {
                        format!("{}h {}m", hours, mins)
                    } else {
                        format!("{}m", mins)
                    }
                })
            })
        });

        // Get max connections
        let max_conn_row = sqlx::query("SHOW VARIABLES LIKE 'max_connections'")
            .fetch_one(&self.0)
            .await
            .ok();
        let max_connections = max_conn_row.and_then(|r| r.try_get::<String, _>(1).ok());

        let mut extra = Vec::new();
        if let Some(max) = max_connections {
            extra.push(("Max Connections".to_string(), max));
        }

        Ok(ServerInfo {
            version,
            database_name,
            connections,
            size,
            host: None,
            port: None,
            uptime,
            extra,
        })
    }
}

// ── Low-level query helpers ───────────────────────────────────────────────────

pub async fn mysql_query(pool: &sqlx::MySqlPool, sql: &str) -> Result<QueryResult, sqlx::Error> {
    let rows = sqlx::query(sql).fetch_all(pool).await?;
    if rows.is_empty() {
        return Ok(QueryResult {
            columns: vec![],
            column_types: vec![],
            column_nullable: vec![],
            rows: vec![],
            rows_affected: Some(0),
        });
    }
    let columns: Vec<String> = rows[0]
        .columns()
        .iter()
        .map(|c| c.name().to_string())
        .collect();
    let column_types: Vec<String> = rows[0]
        .columns()
        .iter()
        .map(|c| c.type_info().name().to_lowercase())
        .collect();
    let column_nullable: Vec<bool> = vec![true; columns.len()];
    let result_rows = rows
        .iter()
        .map(|row| {
            columns
                .iter()
                .enumerate()
                .map(|(i, col)| (col.clone(), mysql_value(row, i)))
                .collect()
        })
        .collect();
    Ok(QueryResult {
        columns,
        column_types,
        column_nullable,
        rows: result_rows,
        rows_affected: None,
    })
}

pub fn mysql_value(row: &sqlx::mysql::MySqlRow, idx: usize) -> Value {
    let type_name = row.columns()[idx].type_info().name().to_lowercase();

    if type_name.contains("int") {
        if let Ok(v) = row.try_get::<u64, _>(idx) {
            return Value::Number(v.into());
        }
        if let Ok(v) = row.try_get::<i64, _>(idx) {
            return Value::Number(v.into());
        }
    }
    if matches!(
        type_name.as_str(),
        "float" | "double" | "decimal" | "numeric"
    ) {
        if let Ok(v) = row.try_get::<f64, _>(idx) {
            if let Some(n) = serde_json::Number::from_f64(v) {
                return Value::Number(n);
            }
        }
    }
    if type_name == "tinyint(1)" || type_name == "boolean" {
        if let Ok(v) = row.try_get::<bool, _>(idx) {
            return Value::Bool(v);
        }
    }
    if matches!(type_name.as_str(), "datetime" | "timestamp") {
        if let Ok(v) = row.try_get::<chrono::DateTime<chrono::Utc>, _>(idx) {
            return Value::String(v.format("%Y-%m-%d %H:%M:%S").to_string());
        }
        if let Ok(v) = row.try_get::<chrono::NaiveDateTime, _>(idx) {
            return Value::String(v.to_string());
        }
    }
    if type_name == "date" {
        if let Ok(v) = row.try_get::<chrono::NaiveDate, _>(idx) {
            return Value::String(v.to_string());
        }
    }
    if type_name == "time" {
        if let Ok(v) = row.try_get::<chrono::NaiveTime, _>(idx) {
            return Value::String(v.to_string());
        }
    }
    if type_name == "json" {
        if let Ok(v) = row.try_get::<Value, _>(idx) {
            return v;
        }
    }
    if let Ok(v) = row.try_get::<String, _>(idx) {
        return Value::String(v);
    }
    if let Ok(bytes) = row.try_get::<Vec<u8>, _>(idx) {
        return Value::String(String::from_utf8_lossy(&bytes).into_owned());
    }

    Value::Null
}
