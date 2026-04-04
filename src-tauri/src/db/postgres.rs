use std::collections::HashMap;

use async_trait::async_trait;
use serde_json::Value;
use sqlx::{Column, Row, TypeInfo};

use super::driver::{DbError, Dialect, Driver, ServerInfo, StreamUpdate};
use super::types::{
    QueryResult, SchemaColumn, SchemaForeignKey, SchemaGraph, SchemaTable, TableInfo,
};

pub struct PostgresDriver(pub sqlx::PgPool);

#[async_trait]
impl Driver for PostgresDriver {
    fn dialect(&self) -> Dialect {
        Dialect::Postgres
    }

    async fn run_query(&self, sql: &str) -> Result<QueryResult, DbError> {
        Ok(pg_query(&self.0, sql).await?)
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
                .map(|(i, col)| (col.clone(), pg_value(&row, i)))
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
            "SELECT table_name, table_type FROM information_schema.tables \
             WHERE table_schema = 'public' AND table_type IN ('BASE TABLE', 'VIEW') \
             ORDER BY table_name",
        )
        .fetch_all(&self.0)
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

    async fn list_databases(&self) -> Result<Vec<String>, DbError> {
        let rows = sqlx::query(
            "SELECT datname FROM pg_database \
             WHERE datistemplate = false ORDER BY datname",
        )
        .fetch_all(&self.0)
        .await?;
        Ok(rows
            .iter()
            .map(|r| r.try_get::<String, _>(0).unwrap_or_default())
            .collect())
    }

    async fn get_column_nullable(
        &self,
        table_name: &str,
    ) -> Result<HashMap<String, bool>, DbError> {
        let rows = sqlx::query(
            "SELECT column_name, is_nullable \
             FROM information_schema.columns \
             WHERE table_schema = 'public' AND table_name = $1",
        )
        .bind(table_name)
        .fetch_all(&self.0)
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

    async fn create_database(&self, db_name: &str) -> Result<(), DbError> {
        self.run_query(&format!("CREATE DATABASE \"{}\"", db_name))
            .await?;
        Ok(())
    }

    async fn get_schema(&self) -> Result<SchemaGraph, DbError> {
        // All columns with PK flag in one query
        let col_rows = sqlx::query(
            "SELECT c.table_name, c.column_name, c.udt_name, c.is_nullable, \
             CASE WHEN pk.column_name IS NOT NULL THEN true ELSE false END AS is_pk \
             FROM information_schema.columns c \
             LEFT JOIN ( \
               SELECT kcu.table_name, kcu.column_name \
               FROM information_schema.table_constraints tc \
               JOIN information_schema.key_column_usage kcu \
                 ON tc.constraint_name = kcu.constraint_name \
                 AND tc.table_schema = kcu.table_schema \
               WHERE tc.constraint_type = 'PRIMARY KEY' AND tc.table_schema = 'public' \
             ) pk ON c.table_name = pk.table_name AND c.column_name = pk.column_name \
             WHERE c.table_schema = 'public' \
             ORDER BY c.table_name, c.ordinal_position",
        )
        .fetch_all(&self.0)
        .await?;

        // Rows are ordered by table_name so we can group by sequential scan
        let mut tables: Vec<SchemaTable> = Vec::new();
        for row in &col_rows {
            let tbl: String = row.try_get(0)?;
            let col = SchemaColumn {
                name: row.try_get(1)?,
                col_type: row.try_get(2)?,
                nullable: row.try_get::<String, _>(3)? == "YES",
                pk: row.try_get(4)?,
            };
            if tables.last().map(|t: &SchemaTable| t.name.as_str()) == Some(tbl.as_str()) {
                if let Some(last) = tables.last_mut() { last.columns.push(col); }
            } else {
                tables.push(SchemaTable {
                    name: tbl,
                    columns: vec![col],
                });
            }
        }

        // All foreign keys
        let fk_rows = sqlx::query(
            "SELECT tc.constraint_name, kcu.table_name, kcu.column_name, \
             ccu.table_name, ccu.column_name \
             FROM information_schema.table_constraints tc \
             JOIN information_schema.key_column_usage kcu \
               ON tc.constraint_name = kcu.constraint_name \
               AND tc.table_schema = kcu.table_schema \
             JOIN information_schema.constraint_column_usage ccu \
               ON tc.constraint_name = ccu.constraint_name \
               AND tc.table_schema = ccu.table_schema \
             WHERE tc.constraint_type = 'FOREIGN KEY' AND tc.table_schema = 'public'",
        )
        .fetch_all(&self.0)
        .await?;

        let foreign_keys = fk_rows
            .iter()
            .map(|r| SchemaForeignKey {
                name: r.try_get(0).unwrap_or_default(),
                from_table: r.try_get(1).unwrap_or_default(),
                from_col: r.try_get(2).unwrap_or_default(),
                to_table: r.try_get(3).unwrap_or_default(),
                to_col: r.try_get(4).unwrap_or_default(),
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
            "SELECT definition FROM pg_views \
             WHERE schemaname = 'public' AND viewname = $1",
        )
        .bind(table_name)
        .fetch_optional(&self.0)
        .await?;

        if let Some(row) = view_row {
            let definition: String = row.try_get(0)?;
            // pg_views.definition returns the SELECT part without CREATE VIEW
            // Trim any trailing semicolon from the definition
            let def = definition.trim().trim_end_matches(';');
            return Ok(format!(
                "-- View Definition\nCREATE OR REPLACE VIEW \"public\".\"{}\" AS\n{};",
                table_name, def
            ));
        }

        // Not a view, treat as table
        let rows = sqlx::query(
            "SELECT column_name, udt_name, is_nullable, column_default, \
             character_maximum_length, numeric_precision, numeric_scale \
             FROM information_schema.columns \
             WHERE table_schema = 'public' AND table_name = $1 \
             ORDER BY ordinal_position",
        )
        .bind(table_name)
        .fetch_all(&self.0)
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

        let pk_rows = sqlx::query(
            "SELECT a.attname \
             FROM pg_index i \
             JOIN pg_attribute a ON a.attrelid = i.indrelid AND a.attnum = ANY(i.indkey) \
             WHERE i.indrelid = $1::regclass AND i.indisprimary",
        )
        .bind(format!("\"public\".\"{}\"", table_name))
        .fetch_all(&self.0)
        .await?;

        let pk_columns: Vec<String> = pk_rows
            .iter()
            .filter_map(|r| r.try_get::<String, _>(0).ok())
            .collect();

        let col_block = columns.join(",\n");
        if pk_columns.is_empty() {
            Ok(format!(
                "-- Table Definition\nCREATE TABLE \"public\".\"{}\" (\n{}\n);",
                table_name, col_block
            ))
        } else {
            let pk_def = format!(
                ",\n    PRIMARY KEY ({})",
                pk_columns
                    .iter()
                    .map(|c| format!("\"{}\"", c))
                    .collect::<Vec<_>>()
                    .join(", ")
            );
            Ok(format!(
                "-- Table Definition\nCREATE TABLE \"public\".\"{}\" (\n{}{}\n);",
                table_name, col_block, pk_def
            ))
        }
    }

    async fn get_server_info(&self) -> Result<ServerInfo, DbError> {
        // Get version
        let version_row = sqlx::query("SELECT version()")
            .fetch_one(&self.0)
            .await
            .ok();
        let version = version_row.and_then(|r| r.try_get::<String, _>(0).ok());

        // Get current database
        let db_row = sqlx::query("SELECT current_database()")
            .fetch_one(&self.0)
            .await
            .ok();
        let database_name = db_row.and_then(|r| r.try_get::<String, _>(0).ok());

        // Get connection count
        let conn_row = sqlx::query("SELECT count(*) FROM pg_stat_activity")
            .fetch_one(&self.0)
            .await
            .ok();
        let connections = conn_row.and_then(|r| r.try_get::<i64, _>(0).ok());

        // Get database size
        let size_row = sqlx::query("SELECT pg_size_pretty(pg_database_size(current_database()))")
            .fetch_one(&self.0)
            .await
            .ok();
        let size = size_row.and_then(|r| r.try_get::<String, _>(0).ok());

        // Get server start time (uptime)
        let uptime_row = sqlx::query("SELECT pg_postmaster_start_time()")
            .fetch_one(&self.0)
            .await
            .ok();
        let uptime = uptime_row.and_then(|r| {
            r.try_get::<chrono::DateTime<chrono::Utc>, _>(0)
                .ok()
                .map(|dt| dt.format("%Y-%m-%d %H:%M:%S UTC").to_string())
        });

        // Get max connections
        let max_conn_row =
            sqlx::query("SELECT setting FROM pg_settings WHERE name = 'max_connections'")
                .fetch_one(&self.0)
                .await
                .ok();
        let max_connections = max_conn_row.and_then(|r| r.try_get::<String, _>(0).ok());

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

// ── Low-level query helpers (also used by export) ────────────────────────────

pub async fn pg_query(pool: &sqlx::PgPool, sql: &str) -> Result<QueryResult, sqlx::Error> {
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
                .map(|(i, col)| (col.clone(), pg_value(row, i)))
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

pub fn pg_value(row: &sqlx::postgres::PgRow, idx: usize) -> Value {
    let type_name = row.columns()[idx].type_info().name().to_lowercase();

    match type_name.as_str() {
        "int2" => {
            if let Ok(v) = row.try_get::<i16, _>(idx) {
                return Value::Number(i64::from(v).into());
            }
        }
        "int4" | "serial" => {
            if let Ok(v) = row.try_get::<i32, _>(idx) {
                return Value::Number(i64::from(v).into());
            }
        }
        "int8" | "bigserial" => {
            if let Ok(v) = row.try_get::<i64, _>(idx) {
                return Value::Number(v.into());
            }
        }
        _ => {}
    }
    if matches!(
        type_name.as_str(),
        "float4" | "float8" | "numeric" | "decimal"
    ) {
        if let Ok(v) = row.try_get::<f64, _>(idx) {
            if let Some(n) = serde_json::Number::from_f64(v) {
                return Value::Number(n);
            }
        }
        if let Ok(v) = row.try_get::<bigdecimal::BigDecimal, _>(idx) {
            return Value::String(v.to_string());
        }
    }
    if type_name == "bool" {
        if let Ok(v) = row.try_get::<bool, _>(idx) {
            return Value::Bool(v);
        }
    }
    if type_name.starts_with("timestamp") {
        if let Ok(v) = row.try_get::<chrono::DateTime<chrono::Utc>, _>(idx) {
            return Value::String(v.to_rfc3339());
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
    if type_name.starts_with("time") {
        if type_name == "timetz" {
            if let Ok(v) = row.try_get::<sqlx::postgres::types::PgTimeTz, _>(idx) {
                return Value::String(format!("{}{}", v.time, v.offset));
            }
        }
        if let Ok(v) = row.try_get::<chrono::NaiveTime, _>(idx) {
            return Value::String(v.to_string());
        }
    }
    if type_name == "uuid" {
        if let Ok(v) = row.try_get::<uuid::Uuid, _>(idx) {
            return Value::String(v.to_string());
        }
    }
    if type_name == "json" || type_name == "jsonb" {
        if let Ok(v) = row.try_get::<Value, _>(idx) {
            return v;
        }
    }
    if type_name == "bytea" {
        if let Ok(v) = row.try_get::<Vec<u8>, _>(idx) {
            return Value::String(format!("\\x{}", hex::encode(v)));
        }
    }
    if matches!(
        type_name.as_str(),
        "oid" | "xid" | "cid" | "regproc" | "regclass" | "regtype"
    ) {
        if let Ok(sqlx::postgres::types::Oid(v)) = row.try_get::<sqlx::postgres::types::Oid, _>(idx)
        {
            return Value::Number(u64::from(v).into());
        }
    }
    if let Ok(v) = row.try_get::<String, _>(idx) {
        return Value::String(v);
    }
    {
        use sqlx::ValueRef;
        if let Ok(raw) = row.try_get_raw(idx) {
            if !raw.is_null() {
                if let Ok(bytes) = raw.as_bytes() {
                    if let Ok(s) = std::str::from_utf8(bytes) {
                        return Value::String(s.to_owned());
                    }
                }
            }
        }
    }

    Value::Null
}
