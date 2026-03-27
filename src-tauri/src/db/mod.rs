use std::collections::HashMap;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sqlx::{Column, Row, TypeInfo};

// ── Connection config ────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Connection {
    pub id: String,
    pub name: String,
    pub driver: String,
    pub host: Option<String>,
    pub port: Option<u16>,
    pub database: String,
    pub username: Option<String>,
    pub password: Option<String>,
    #[serde(rename = "filePath")]
    pub file_path: Option<String>,
}

impl Connection {
    pub fn to_url(&self) -> String {
        match self.driver.as_str() {
            "postgres" => format!(
                "postgres://{}:{}@{}:{}/{}",
                self.username.as_deref().unwrap_or(""),
                self.password.as_deref().unwrap_or(""),
                self.host.as_deref().unwrap_or("localhost"),
                self.port.unwrap_or(5432),
                self.database
            ),
            "mysql" => format!(
                "mysql://{}:{}@{}:{}/{}",
                self.username.as_deref().unwrap_or(""),
                self.password.as_deref().unwrap_or(""),
                self.host.as_deref().unwrap_or("localhost"),
                self.port.unwrap_or(3306),
                self.database
            ),
            "sqlite" => format!(
                "sqlite://{}",
                self.file_path.as_deref().unwrap_or(&self.database)
            ),
            _ => String::new(),
        }
    }
}

// ── Query result ─────────────────────────────────────────────────────────────

#[derive(Debug, Serialize)]
pub struct QueryResult {
    pub columns: Vec<String>,
    pub rows: Vec<HashMap<String, Value>>,
    pub rows_affected: Option<u64>,
}

// ── Driver-specific pool enum ─────────────────────────────────────────────────

#[derive(Clone)]
pub enum DbPool {
    Postgres(sqlx::PgPool),
    Mysql(sqlx::MySqlPool),
    Sqlite(sqlx::SqlitePool),
}

impl DbPool {
    pub async fn connect(conn: &Connection) -> Result<Self, sqlx::Error> {
        let url = conn.to_url();
        match conn.driver.as_str() {
            "postgres" => Ok(DbPool::Postgres(sqlx::PgPool::connect(&url).await?)),
            "mysql"    => Ok(DbPool::Mysql(sqlx::MySqlPool::connect(&url).await?)),
            "sqlite"   => Ok(DbPool::Sqlite(sqlx::SqlitePool::connect(&url).await?)),
            d => Err(sqlx::Error::Configuration(format!("unknown driver: {d}").into())),
        }
    }

    pub async fn run_query(&self, sql: &str) -> Result<QueryResult, sqlx::Error> {
        match self {
            DbPool::Postgres(pool) => pg_query(pool, sql).await,
            DbPool::Mysql(pool)    => mysql_query(pool, sql).await,
            DbPool::Sqlite(pool)   => sqlite_query(pool, sql).await,
        }
    }
}

// ── PostgreSQL ────────────────────────────────────────────────────────────────

async fn pg_query(pool: &sqlx::PgPool, sql: &str) -> Result<QueryResult, sqlx::Error> {
    let rows = sqlx::query(sql).fetch_all(pool).await?;
    if rows.is_empty() {
        return Ok(QueryResult { columns: vec![], rows: vec![], rows_affected: Some(0) });
    }
    let columns: Vec<String> = rows[0].columns().iter().map(|c| c.name().to_string()).collect();
    let result_rows = rows.iter().map(|row| {
        columns.iter().enumerate().map(|(i, col)| {
            (col.clone(), pg_value(row, i))
        }).collect()
    }).collect();
    Ok(QueryResult { columns, rows: result_rows, rows_affected: None })
}

fn pg_value(row: &sqlx::postgres::PgRow, idx: usize) -> Value {
    let type_name = row.columns()[idx].type_info().name().to_lowercase();

    // integers
    if matches!(type_name.as_str(), "int2" | "int4" | "int8" | "serial" | "bigserial") {
        if let Ok(v) = row.try_get::<i64, _>(idx) { return Value::Number(v.into()); }
    }
    // floats
    if matches!(type_name.as_str(), "float4" | "float8" | "numeric" | "decimal") {
        if let Ok(v) = row.try_get::<f64, _>(idx) {
            if let Some(n) = serde_json::Number::from_f64(v) { return Value::Number(n); }
        }
        if let Ok(v) = row.try_get::<bigdecimal::BigDecimal, _>(idx) {
            return Value::String(v.to_string());
        }
    }
    // bool
    if type_name == "bool" {
        if let Ok(v) = row.try_get::<bool, _>(idx) { return Value::Bool(v); }
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
        if let Ok(v) = row.try_get::<Value, _>(idx) { return v; }
    }
    // bytes
    if type_name == "bytea" {
        if let Ok(v) = row.try_get::<Vec<u8>, _>(idx) {
            return Value::String(format!("\\x{}", hex::encode(v)));
        }
    }
    // fallback: everything else as string
    if let Ok(v) = row.try_get::<String, _>(idx) { return Value::String(v); }

    Value::Null
}

// ── MySQL ─────────────────────────────────────────────────────────────────────

async fn mysql_query(pool: &sqlx::MySqlPool, sql: &str) -> Result<QueryResult, sqlx::Error> {
    let rows = sqlx::query(sql).fetch_all(pool).await?;
    if rows.is_empty() {
        return Ok(QueryResult { columns: vec![], rows: vec![], rows_affected: Some(0) });
    }
    let columns: Vec<String> = rows[0].columns().iter().map(|c| c.name().to_string()).collect();
    let result_rows = rows.iter().map(|row| {
        columns.iter().enumerate().map(|(i, col)| {
            (col.clone(), mysql_value(row, i))
        }).collect()
    }).collect();
    Ok(QueryResult { columns, rows: result_rows, rows_affected: None })
}

fn mysql_value(row: &sqlx::mysql::MySqlRow, idx: usize) -> Value {
    let type_name = row.columns()[idx].type_info().name().to_lowercase();

    if matches!(type_name.as_str(), "tinyint" | "smallint" | "mediumint" | "int" | "bigint") {
        if let Ok(v) = row.try_get::<i64, _>(idx) { return Value::Number(v.into()); }
    }
    if matches!(type_name.as_str(), "float" | "double" | "decimal" | "numeric") {
        if let Ok(v) = row.try_get::<f64, _>(idx) {
            if let Some(n) = serde_json::Number::from_f64(v) { return Value::Number(n); }
        }
    }
    if type_name == "tinyint(1)" || type_name == "boolean" {
        if let Ok(v) = row.try_get::<bool, _>(idx) { return Value::Bool(v); }
    }
    if matches!(type_name.as_str(), "datetime" | "timestamp") {
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
        if let Ok(v) = row.try_get::<Value, _>(idx) { return v; }
    }
    if let Ok(v) = row.try_get::<String, _>(idx) { return Value::String(v); }

    Value::Null
}

// ── SQLite ────────────────────────────────────────────────────────────────────

async fn sqlite_query(pool: &sqlx::SqlitePool, sql: &str) -> Result<QueryResult, sqlx::Error> {
    let rows = sqlx::query(sql).fetch_all(pool).await?;
    if rows.is_empty() {
        return Ok(QueryResult { columns: vec![], rows: vec![], rows_affected: Some(0) });
    }
    let columns: Vec<String> = rows[0].columns().iter().map(|c| c.name().to_string()).collect();
    let result_rows = rows.iter().map(|row| {
        columns.iter().enumerate().map(|(i, col)| {
            (col.clone(), sqlite_value(row, i))
        }).collect()
    }).collect();
    Ok(QueryResult { columns, rows: result_rows, rows_affected: None })
}

fn sqlite_value(row: &sqlx::sqlite::SqliteRow, idx: usize) -> Value {
    // SQLite is dynamically typed — try in priority order
    if let Ok(v) = row.try_get::<bool, _>(idx)   { return Value::Bool(v); }
    if let Ok(v) = row.try_get::<i64, _>(idx)    { return Value::Number(v.into()); }
    if let Ok(v) = row.try_get::<f64, _>(idx) {
        if let Some(n) = serde_json::Number::from_f64(v) { return Value::Number(n); }
    }
    if let Ok(v) = row.try_get::<String, _>(idx) { return Value::String(v); }
    Value::Null
}
