use serde_json::Value;
use sqlx::{Column, Row, TypeInfo};

use super::types::QueryResult;

pub async fn mysql_query(pool: &sqlx::MySqlPool, sql: &str) -> Result<QueryResult, sqlx::Error> {
    let rows = sqlx::query(sql).fetch_all(pool).await?;
    if rows.is_empty() {
        return Ok(QueryResult {
            columns: vec![],
            rows: vec![],
            rows_affected: Some(0),
        });
    }
    let columns: Vec<String> = rows[0]
        .columns()
        .iter()
        .map(|c| c.name().to_string())
        .collect();
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
        rows: result_rows,
        rows_affected: None,
    })
}

pub fn mysql_value(row: &sqlx::mysql::MySqlRow, idx: usize) -> Value {
    let type_name = row.columns()[idx].type_info().name().to_lowercase();

    if type_name.contains("int") {
        // Try unsigned first for "int unsigned", "bigint unsigned", etc.
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
        // MySQL TIMESTAMP is UTC-aware; DATETIME is naive
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
