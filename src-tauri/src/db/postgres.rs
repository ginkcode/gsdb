use serde_json::Value;
use sqlx::{Column, Row, TypeInfo};

use super::types::QueryResult;

pub async fn pg_query(pool: &sqlx::PgPool, sql: &str) -> Result<QueryResult, sqlx::Error> {
    let rows = sqlx::query(sql).fetch_all(pool).await?;
    if rows.is_empty() {
        return Ok(QueryResult {
            columns: vec![],
            column_types: vec![],
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
        rows: result_rows,
        rows_affected: None,
    })
}

pub fn pg_value(row: &sqlx::postgres::PgRow, idx: usize) -> Value {
    let type_name = row.columns()[idx].type_info().name().to_lowercase();

    // integers — each pg integer type maps to a specific Rust type in sqlx
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
    // floats
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
    // bool
    if type_name == "bool" {
        if let Ok(v) = row.try_get::<bool, _>(idx) {
            return Value::Bool(v);
        }
    }
    // timestamps / dates / times → ISO string
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
        // timetz (time with time zone) needs PgTimeTz, not NaiveTime
        if type_name == "timetz" {
            if let Ok(v) = row.try_get::<sqlx::postgres::types::PgTimeTz, _>(idx) {
                return Value::String(format!("{}{}", v.time, v.offset));
            }
        }
        if let Ok(v) = row.try_get::<chrono::NaiveTime, _>(idx) {
            return Value::String(v.to_string());
        }
    }
    // uuid
    if type_name == "uuid" {
        if let Ok(v) = row.try_get::<uuid::Uuid, _>(idx) {
            return Value::String(v.to_string());
        }
    }
    // json / jsonb
    if type_name == "json" || type_name == "jsonb" {
        if let Ok(v) = row.try_get::<Value, _>(idx) {
            return v;
        }
    }
    // bytes
    if type_name == "bytea" {
        if let Ok(v) = row.try_get::<Vec<u8>, _>(idx) {
            return Value::String(format!("\\x{}", hex::encode(v)));
        }
    }
    // oid and oid-based types — sqlx represents these as Oid(u32), not u32 or String
    if matches!(
        type_name.as_str(),
        "oid" | "xid" | "cid" | "regproc" | "regclass" | "regtype"
    ) {
        if let Ok(sqlx::postgres::types::Oid(v)) =
            row.try_get::<sqlx::postgres::types::Oid, _>(idx)
        {
            return Value::Number(u64::from(v).into());
        }
    }
    // fallback: try String first, then read raw bytes to handle enums and
    // other custom types whose OID doesn't match TEXT/VARCHAR
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
