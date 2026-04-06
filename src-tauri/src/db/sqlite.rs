use std::collections::HashMap;

use async_trait::async_trait;
use serde_json::Value;
use sqlx::{Column, Row, TypeInfo};

use super::driver::{stmt_error, DbError, Dialect, Driver, ServerInfo, StreamUpdate};
use super::types::{
    QueryResult, SchemaColumn, SchemaForeignKey, SchemaGraph, SchemaTable, TableInfo,
};

pub struct SqliteDriver(pub sqlx::SqlitePool);

#[async_trait]
impl Driver for SqliteDriver {
    fn dialect(&self) -> Dialect {
        Dialect::Sqlite
    }

    async fn run_query(&self, sql: &str) -> Result<QueryResult, DbError> {
        Ok(sqlite_query(&self.0, sql).await?)
    }

    async fn stream_query(
        &self,
        sql: &str,
        tx: tokio::sync::mpsc::Sender<StreamUpdate>,
    ) -> Result<(), DbError> {
        use futures::StreamExt;
        const BATCH: usize = 200;

        let mut stream = sqlx::query(sql).fetch(&self.0);
        let mut columns: Vec<String> = vec![];
        let mut header_sent = false;
        let mut batch: Vec<std::collections::HashMap<String, serde_json::Value>> =
            Vec::with_capacity(BATCH);

        while let Some(row) = stream.next().await {
            let row = row.map_err(DbError::Sqlx)?;

            if !header_sent {
                columns = row.columns().iter().map(|c| c.name().to_string()).collect();
                let column_types = row
                    .columns()
                    .iter()
                    .map(|c| c.type_info().name().to_lowercase())
                    .collect();
                if tx
                    .send(StreamUpdate::Header {
                        columns: columns.clone(),
                        column_types,
                        column_nullable: vec![true; columns.len()],
                    })
                    .await
                    .is_err()
                {
                    return Ok(());
                }
                header_sent = true;
            }

            let row_map = columns
                .iter()
                .enumerate()
                .map(|(i, col)| (col.clone(), sqlite_value(&row, i)))
                .collect();
            batch.push(row_map);

            if batch.len() >= BATCH {
                if tx
                    .send(StreamUpdate::Rows {
                        rows: std::mem::take(&mut batch),
                    })
                    .await
                    .is_err()
                {
                    return Ok(());
                }
            }
        }

        if !header_sent {
            let _ = tx
                .send(StreamUpdate::Header {
                    columns: vec![],
                    column_types: vec![],
                    column_nullable: vec![],
                })
                .await;
        } else if !batch.is_empty() {
            let _ = tx.send(StreamUpdate::Rows { rows: batch }).await;
        }

        let _ = tx.send(StreamUpdate::Done { rows_affected: None }).await;
        Ok(())
    }

    async fn list_tables(&self) -> Result<Vec<TableInfo>, DbError> {
        let rows = sqlx::query(
            "SELECT name, type FROM sqlite_master \
             WHERE type IN ('table', 'view') ORDER BY name",
        )
        .fetch_all(&self.0)
        .await?;
        Ok(rows
            .iter()
            .map(|r| TableInfo {
                name: r.try_get::<String, _>(0).unwrap_or_default(),
                kind: r.try_get::<String, _>(1).unwrap_or_default(),
            })
            .collect())
    }

    async fn list_databases(&self) -> Result<Vec<String>, DbError> {
        Ok(vec![])
    }

    async fn get_column_nullable(
        &self,
        table_name: &str,
    ) -> Result<HashMap<String, bool>, DbError> {
        let rows = sqlx::query(&format!(
            "PRAGMA table_info(\"{}\")",
            table_name.replace('"', "\"\"")
        ))
        .fetch_all(&self.0)
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

    async fn create_database(&self, _db_name: &str) -> Result<(), DbError> {
        // SQLite databases are created on connect; nothing to do here
        Ok(())
    }

    async fn get_schema(&self) -> Result<SchemaGraph, DbError> {
        // Get all tables (not views)
        let table_rows =
            sqlx::query("SELECT name FROM sqlite_master WHERE type = 'table' ORDER BY name")
                .fetch_all(&self.0)
                .await?;

        let table_names: Vec<String> = table_rows
            .iter()
            .filter_map(|r| r.try_get::<String, _>(0).ok())
            .collect();

        let mut tables: Vec<SchemaTable> = Vec::new();
        let mut foreign_keys: Vec<SchemaForeignKey> = Vec::new();

        for tbl in &table_names {
            // PRAGMA table_info: (cid, name, type, notnull, dflt_value, pk)
            let col_rows = sqlx::query(&format!(
                "PRAGMA table_info(\"{}\")",
                tbl.replace('"', "\"\"")
            ))
            .fetch_all(&self.0)
            .await?;

            let columns = col_rows
                .iter()
                .filter_map(|r| {
                    Some(SchemaColumn {
                        name: r.try_get::<String, _>(1).ok()?,
                        col_type: r.try_get::<String, _>(2).unwrap_or_default(),
                        nullable: r.try_get::<i64, _>(3).unwrap_or(0) == 0,
                        pk: r.try_get::<i64, _>(5).unwrap_or(0) != 0,
                    })
                })
                .collect();

            tables.push(SchemaTable {
                name: tbl.clone(),
                columns,
            });

            // PRAGMA foreign_key_list: (id, seq, table, from, to, ...)
            let fk_rows = sqlx::query(&format!(
                "PRAGMA foreign_key_list(\"{}\")",
                tbl.replace('"', "\"\"")
            ))
            .fetch_all(&self.0)
            .await?;

            for r in &fk_rows {
                let from_col = r.try_get::<String, _>(3).unwrap_or_default();
                let to_table = r.try_get::<String, _>(2).unwrap_or_default();
                let to_col = r.try_get::<String, _>(4).unwrap_or_default();
                foreign_keys.push(SchemaForeignKey {
                    name: format!("fk_{}_{}_{}", tbl, from_col, to_table),
                    from_table: tbl.clone(),
                    from_col,
                    to_table,
                    to_col,
                });
            }
        }

        Ok(SchemaGraph {
            tables,
            foreign_keys,
        })
    }

    async fn get_table_definition(&self, table_name: &str) -> Result<String, DbError> {
        let rows = sqlx::query(
            "SELECT type, sql FROM sqlite_master WHERE type IN ('table', 'view') AND name=?",
        )
        .bind(table_name)
        .fetch_all(&self.0)
        .await?;
        if let Some(row) = rows.first() {
            let obj_type: String = row.try_get(0)?;
            let create_stmt: String = row.try_get(1)?;
            let header = if obj_type == "view" {
                "-- View Definition"
            } else {
                "-- Table Definition"
            };
            Ok(format!(
                "{}\n{};",
                header,
                create_stmt.trim_end_matches(';')
            ))
        } else {
            Err(DbError::Sqlx(sqlx::Error::RowNotFound))
        }
    }

    async fn import_all_statements(
        &self,
        main: Vec<String>,
        on_error: Vec<String>,
        mut on_stmt_done: Box<dyn FnMut() -> bool + Send>,
    ) -> Result<usize, DbError> {
        let mut conn = self.0.acquire().await.map_err(DbError::Sqlx)?;
        let mut count = 0;
        for (idx, stmt) in main.iter().enumerate() {
            if let Err(e) = sqlx::query(stmt).execute(&mut *conn).await {
                for s in &on_error {
                    let _ = sqlx::query(s).execute(&mut *conn).await;
                }
                return Err(stmt_error(DbError::Sqlx(e), idx + 1, stmt));
            }
            count += 1;
            if !on_stmt_done() {
                for s in &on_error {
                    let _ = sqlx::query(s).execute(&mut *conn).await;
                }
                return Err(DbError::Cancelled);
            }
        }
        Ok(count)
    }

    async fn get_server_info(&self) -> Result<ServerInfo, DbError> {
        // Get SQLite version
        let version_row = sqlx::query("SELECT sqlite_version()")
            .fetch_one(&self.0)
            .await
            .ok();
        let version = version_row.and_then(|r| r.try_get::<String, _>(0).ok());

        // Get database file path
        let file_row = sqlx::query("PRAGMA database_list")
            .fetch_all(&self.0)
            .await
            .ok();
        let database_name =
            file_row.and_then(|rows| rows.first().and_then(|r| r.try_get::<String, _>(2).ok()));

        // Get page count and page size for size calculation
        let page_count_row = sqlx::query("PRAGMA page_count")
            .fetch_one(&self.0)
            .await
            .ok();
        let page_size_row = sqlx::query("PRAGMA page_size")
            .fetch_one(&self.0)
            .await
            .ok();
        let size = match (page_count_row, page_size_row) {
            (Some(pc), Some(ps)) => {
                let pages: i64 = pc.try_get(0).unwrap_or(0);
                let page_size: i64 = ps.try_get(0).unwrap_or(0);
                let bytes = pages * page_size;
                Some(if bytes < 1024 {
                    format!("{} B", bytes)
                } else if bytes < 1024 * 1024 {
                    format!("{} KB", bytes / 1024)
                } else if bytes < 1024 * 1024 * 1024 {
                    format!("{} MB", bytes / 1024 / 1024)
                } else {
                    format!("{} GB", bytes / 1024 / 1024 / 1024)
                })
            }
            _ => None,
        };

        // Get table count
        let table_count_row = sqlx::query("SELECT count(*) FROM sqlite_master WHERE type='table'")
            .fetch_one(&self.0)
            .await
            .ok();
        let table_count = table_count_row.and_then(|r| r.try_get::<i64, _>(0).ok());

        let mut extra = Vec::new();
        if let Some(count) = table_count {
            extra.push(("Tables".to_string(), count.to_string()));
        }

        Ok(ServerInfo {
            version,
            database_name,
            connections: None, // SQLite doesn't have connection tracking
            size,
            host: None,
            port: None,
            uptime: None,
            extra,
        })
    }
}

// ── Low-level query helpers ───────────────────────────────────────────────────

pub async fn sqlite_query(pool: &sqlx::SqlitePool, sql: &str) -> Result<QueryResult, sqlx::Error> {
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
                .map(|(i, col)| (col.clone(), sqlite_value(row, i)))
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

pub fn sqlite_value(row: &sqlx::sqlite::SqliteRow, idx: usize) -> Value {
    use sqlx::ValueRef;

    // Check for NULL first before attempting any type conversion
    if let Ok(raw) = row.try_get_raw(idx) {
        if raw.is_null() {
            return Value::Null;
        }
    }

    // SQLite is dynamically typed — try in priority order.
    // Check integers BEFORE booleans because sqlx's try_get::<bool> succeeds
    // for integer values (0 = false, non-zero = true).
    if let Ok(v) = row.try_get::<i64, _>(idx) {
        return Value::Number(v.into());
    }
    if let Ok(v) = row.try_get::<f64, _>(idx) {
        if let Some(n) = serde_json::Number::from_f64(v) {
            return Value::Number(n);
        }
    }
    if let Ok(v) = row.try_get::<bool, _>(idx) {
        return Value::Bool(v);
    }
    if let Ok(v) = row.try_get::<String, _>(idx) {
        return Value::String(v);
    }
    Value::Null
}
