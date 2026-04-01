use serde_json::Value;
use sqlx::{Column, Row, TypeInfo};

use super::types::QueryResult;

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
    // sqlx's try_get can return default values (0 for integers, "" for strings)
    // when the column is NULL, so we must check explicitly.
    if let Ok(raw) = row.try_get_raw(idx) {
        if raw.is_null() {
            return Value::Null;
        }
    }

    // SQLite is dynamically typed — try in priority order
    // IMPORTANT: Check integers BEFORE booleans because sqlx's try_get::<bool>
    // succeeds for integer values (0 = false, non-zero = true), which would
    // incorrectly convert INT columns to booleans.
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
