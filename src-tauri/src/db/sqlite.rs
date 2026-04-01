use std::collections::HashMap;

use async_trait::async_trait;
use serde_json::Value;
use sqlx::{Column, Row, TypeInfo};

use super::driver::{DbError, Dialect, Driver};
use super::types::{QueryResult, SchemaColumn, SchemaForeignKey, SchemaGraph, SchemaTable, TableInfo};

pub struct SqliteDriver(pub sqlx::SqlitePool);

#[async_trait]
impl Driver for SqliteDriver {
    fn dialect(&self) -> Dialect {
        Dialect::Sqlite
    }

    async fn run_query(&self, sql: &str) -> Result<QueryResult, DbError> {
        Ok(sqlite_query(&self.0, sql).await?)
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
        let table_rows = sqlx::query(
            "SELECT name FROM sqlite_master WHERE type = 'table' ORDER BY name",
        )
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

            tables.push(SchemaTable { name: tbl.clone(), columns });

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

        Ok(SchemaGraph { tables, foreign_keys })
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
}

// ── Low-level query helpers ───────────────────────────────────────────────────

pub async fn sqlite_query(
    pool: &sqlx::SqlitePool,
    sql: &str,
) -> Result<QueryResult, sqlx::Error> {
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
