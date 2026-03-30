use serde::{Deserialize, Serialize};
use serde_json::Value;
use sqlx::{Column, Row, TypeInfo};
use ssh2::Session;
use std::collections::HashMap;
use std::net::TcpStream;
use std::sync::Arc;
use tokio::sync::Mutex;

// ── SSH Tunnel config ────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SshConfig {
    pub host: String,
    pub port: u16,
    pub username: String,
    pub password: Option<String>,
    #[serde(rename = "privateKey")]
    pub private_key: Option<String>,
    #[serde(rename = "privateKeyPassphrase")]
    pub private_key_passphrase: Option<String>,
}

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
    pub ssh: Option<SshConfig>,
    #[serde(rename = "sslMode")]
    pub ssl_mode: Option<String>,
}

/// Active SSH tunnel that keeps the session alive
pub struct SshTunnel {
    #[allow(dead_code)]
    session: Arc<Mutex<Option<Session>>>,
    local_port: u16,
}

impl SshTunnel {
    /// Create an SSH tunnel and return the local port
    pub fn create(ssh: &SshConfig, target_host: &str, target_port: u16) -> Result<Self, String> {
        // Connect to SSH server
        let ssh_addr = format!("{}:{}", ssh.host, ssh.port);
        let tcp = TcpStream::connect(&ssh_addr)
            .map_err(|e| format!("Failed to connect to SSH server {}: {}", ssh_addr, e))?;
        tcp.set_nonblocking(false)
            .map_err(|e| format!("Failed to set blocking mode: {}", e))?;

        let mut session =
            Session::new().map_err(|e| format!("Failed to create SSH session: {}", e))?;
        session.set_tcp_stream(tcp);
        session
            .handshake()
            .map_err(|e| format!("SSH handshake failed: {}", e))?;

        // Authenticate
        if let Some(password) = &ssh.password {
            session
                .userauth_password(&ssh.username, password)
                .map_err(|e| format!("SSH password authentication failed: {}", e))?;
        } else if let Some(private_key) = &ssh.private_key {
            let passphrase = ssh.private_key_passphrase.as_deref();
            session
                .userauth_pubkey_memory(&ssh.username, None, private_key, passphrase)
                .map_err(|e| format!("SSH key authentication failed: {}", e))?;
        } else {
            // Try default SSH key from ssh-agent
            session
                .userauth_agent(&ssh.username)
                .map_err(|e| format!("SSH agent authentication failed: {}", e))?;
        }

        if !session.authenticated() {
            return Err("SSH authentication failed".to_string());
        }

        // Find an available local port
        let listener = std::net::TcpListener::bind("127.0.0.1:0")
            .map_err(|e| format!("Failed to bind local port: {}", e))?;
        let local_port = listener
            .local_addr()
            .map_err(|e| format!("Failed to get local address: {}", e))?
            .port();
        drop(listener);

        // Create port forwarding channel
        let channel = session
            .channel_direct_tcpip(target_host, target_port, None)
            .map_err(|e| format!("Failed to create SSH tunnel: {}", e))?;

        // Start local port forwarding in a background task
        let session_arc = Arc::new(Mutex::new(Some(session)));
        let session_clone = session_arc.clone();

        // Spawn a blocking task to handle the tunnel
        std::thread::spawn(move || {
            // Accept connections on local_port and forward through channel
            if let Ok(listener) = std::net::TcpListener::bind(format!("127.0.0.1:{}", local_port)) {
                for stream in listener.incoming() {
                    if let Ok(mut local_stream) = stream {
                        let mut channel_clone = channel.clone();
                        // Forward data bidirectionally
                        std::thread::spawn(move || {
                            let _ = std::io::copy(&mut local_stream, &mut channel_clone);
                        });
                    }
                }
            }
            let _ = session_clone;
        });

        Ok(SshTunnel {
            session: session_arc,
            local_port,
        })
    }

    pub fn local_port(&self) -> u16 {
        self.local_port
    }
}

impl Connection {
    pub fn to_url(&self) -> String {
        match self.driver.as_str() {
            "postgres" => format!(
                "postgres://{}:{}@{}:{}/{}?sslmode={}",
                self.username.as_deref().unwrap_or(""),
                self.password.as_deref().unwrap_or(""),
                self.host.as_deref().unwrap_or("localhost"),
                self.port.unwrap_or(5432),
                self.database,
                self.ssl_mode.as_deref().unwrap_or("prefer")
            ),
            "mysql" => format!(
                "mysql://{}:{}@{}:{}/{}?ssl-mode={}",
                self.username.as_deref().unwrap_or(""),
                self.password.as_deref().unwrap_or(""),
                self.host.as_deref().unwrap_or("localhost"),
                self.port.unwrap_or(3306),
                self.database,
                self.ssl_mode.as_deref().unwrap_or("preferred")
            ),
            "sqlite" => format!(
                "sqlite://{}",
                self.file_path.as_deref().unwrap_or(&self.database)
            ),
            _ => String::new(),
        }
    }

    pub fn to_url_via_tunnel(&self, local_port: u16) -> String {
        match self.driver.as_str() {
            "postgres" => format!(
                "postgres://{}:{}@127.0.0.1:{}/{}?sslmode={}",
                self.username.as_deref().unwrap_or(""),
                self.password.as_deref().unwrap_or(""),
                local_port,
                self.database,
                self.ssl_mode.as_deref().unwrap_or("prefer")
            ),
            "mysql" => format!(
                "mysql://{}:{}@127.0.0.1:{}/{}?ssl-mode={}",
                self.username.as_deref().unwrap_or(""),
                self.password.as_deref().unwrap_or(""),
                local_port,
                self.database,
                self.ssl_mode.as_deref().unwrap_or("preferred")
            ),
            _ => self.to_url(),
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
        let url = if let Some(ssh) = &conn.ssh {
            // Establish SSH tunnel and connect through it
            let target_host = conn.host.clone().unwrap_or_else(|| "localhost".to_string());
            let target_port = conn.port.unwrap_or(match conn.driver.as_str() {
                "postgres" => 5432,
                "mysql" => 3306,
                _ => 0,
            });

            // Clone SSH config to move into spawn_blocking
            let ssh_config = ssh.clone();

            // Create SSH tunnel (this runs synchronously)
            let tunnel = tokio::task::spawn_blocking(move || {
                SshTunnel::create(&ssh_config, &target_host, target_port)
            })
            .await
            .map_err(|e| sqlx::Error::Configuration(e.to_string().into()))?
            .map_err(|e| sqlx::Error::Configuration(e.into()))?;

            let local_port = tunnel.local_port();

            // Store the tunnel to keep it alive (it will be dropped when the pool is dropped)
            // For now, we'll just use the local port
            let _ = tunnel; // Keep alive

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

    pub async fn list_tables(&self) -> Result<Vec<String>, sqlx::Error> {
        match self {
            DbPool::Postgres(pool) => {
                let rows = sqlx::query(
                    "SELECT table_name FROM information_schema.tables \
                     WHERE table_schema = 'public' ORDER BY table_name",
                )
                .fetch_all(pool)
                .await?;
                Ok(rows
                    .iter()
                    .map(|r| r.try_get::<String, _>(0).unwrap_or_default())
                    .collect())
            }
            DbPool::Mysql(pool) => {
                let rows = sqlx::query("SHOW TABLES").fetch_all(pool).await?;
                Ok(rows
                    .iter()
                    .map(|r| r.try_get::<String, _>(0).unwrap_or_default())
                    .collect())
            }
            DbPool::Sqlite(pool) => {
                let rows =
                    sqlx::query("SELECT name FROM sqlite_master WHERE type='table' ORDER BY name")
                        .fetch_all(pool)
                        .await?;
                Ok(rows
                    .iter()
                    .map(|r| r.try_get::<String, _>(0).unwrap_or_default())
                    .collect())
            }
        }
    }

    pub async fn create_database(&self, db_name: &str) -> Result<(), sqlx::Error> {
        // Validate name to prevent SQL injection (only allow alphanumeric, underscore, hyphen)
        if !db_name.chars().all(|c| c.is_alphanumeric() || c == '_' || c == '-') {
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
                let rows = sqlx::query("SHOW DATABASES").fetch_all(pool).await?;
                Ok(rows
                    .iter()
                    .map(|r| r.try_get::<String, _>(0).unwrap_or_default())
                    .collect())
            }
            DbPool::Sqlite(_) => Ok(vec![]),
        }
    }

    pub async fn get_table_definition(&self, table_name: &str) -> Result<String, sqlx::Error> {
        match self {
            DbPool::Postgres(pool) => {
                // Get table schema
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
                        "numeric" => {
                            match (num_precision, num_scale) {
                                (Some(prec), Some(scale)) if scale > 0 => {
                                    format!("{}({}, {})", udt_name, prec, scale)
                                }
                                (Some(prec), _) => format!("{}({})", udt_name, prec),
                                _ => udt_name.clone(),
                            }
                        }
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
                    // Insert before the closing parenthesis
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
                let rows = sqlx::query(&format!(
                    "SELECT sql FROM sqlite_master WHERE type='table' AND name=?"
                ))
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

// ── PostgreSQL ────────────────────────────────────────────────────────────────

async fn pg_query(pool: &sqlx::PgPool, sql: &str) -> Result<QueryResult, sqlx::Error> {
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
                .map(|(i, col)| (col.clone(), pg_value(row, i)))
                .collect()
        })
        .collect();
    Ok(QueryResult {
        columns,
        rows: result_rows,
        rows_affected: None,
    })
}

fn pg_value(row: &sqlx::postgres::PgRow, idx: usize) -> Value {
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
    // fallback: everything else as string
    if let Ok(v) = row.try_get::<String, _>(idx) {
        return Value::String(v);
    }

    Value::Null
}

// ── MySQL ─────────────────────────────────────────────────────────────────────

async fn mysql_query(pool: &sqlx::MySqlPool, sql: &str) -> Result<QueryResult, sqlx::Error> {
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

fn mysql_value(row: &sqlx::mysql::MySqlRow, idx: usize) -> Value {
    let type_name = row.columns()[idx].type_info().name().to_lowercase();

    if matches!(
        type_name.as_str(),
        "tinyint" | "smallint" | "mediumint" | "int" | "bigint"
    ) {
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

    Value::Null
}

// ── SQLite ────────────────────────────────────────────────────────────────────

async fn sqlite_query(pool: &sqlx::SqlitePool, sql: &str) -> Result<QueryResult, sqlx::Error> {
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
                .map(|(i, col)| (col.clone(), sqlite_value(row, i)))
                .collect()
        })
        .collect();
    Ok(QueryResult {
        columns,
        rows: result_rows,
        rows_affected: None,
    })
}

fn sqlite_value(row: &sqlx::sqlite::SqliteRow, idx: usize) -> Value {
    // SQLite is dynamically typed — try in priority order
    if let Ok(v) = row.try_get::<bool, _>(idx) {
        return Value::Bool(v);
    }
    if let Ok(v) = row.try_get::<i64, _>(idx) {
        return Value::Number(v.into());
    }
    if let Ok(v) = row.try_get::<f64, _>(idx) {
        if let Some(n) = serde_json::Number::from_f64(v) {
            return Value::Number(n);
        }
    }
    if let Ok(v) = row.try_get::<String, _>(idx) {
        return Value::String(v);
    }
    Value::Null
}

// ── Export / Import ───────────────────────────────────────────────────────────

fn quote_ident(name: &str, backtick: bool) -> String {
    if backtick {
        format!("`{}`", name)
    } else {
        format!("\"{}\"", name)
    }
}

fn value_to_sql(value: &Value, is_mysql: bool) -> String {
    match value {
        Value::Null => "NULL".to_string(),
        Value::Bool(b) => {
            if is_mysql {
                if *b { "1" } else { "0" }.to_string()
            } else if *b {
                "TRUE".to_string()
            } else {
                "FALSE".to_string()
            }
        }
        Value::Number(n) => n.to_string(),
        Value::String(s) => format!("'{}'", s.replace('\'', "''")),
        other => format!(
            "'{}'",
            serde_json::to_string(other)
                .unwrap_or_default()
                .replace('\'', "''")
        ),
    }
}

/// Split a SQL string into individual statements, correctly handling single-quoted
/// string literals and line comments so that semicolons inside them are not treated
/// as statement boundaries.
fn split_sql_statements(sql: &str) -> Vec<String> {
    let mut statements: Vec<String> = Vec::new();
    let mut current = String::new();
    let mut in_string = false;
    let mut chars = sql.chars().peekable();

    while let Some(ch) = chars.next() {
        if in_string {
            current.push(ch);
            if ch == '\'' {
                if chars.peek() == Some(&'\'') {
                    current.push(chars.next().unwrap());
                } else {
                    in_string = false;
                }
            }
        } else {
            match ch {
                '\'' => {
                    in_string = true;
                    current.push(ch);
                }
                '-' if chars.peek() == Some(&'-') => {
                    // Line comment — consume to end of line, drop from output
                    for c in chars.by_ref() {
                        if c == '\n' {
                            break;
                        }
                    }
                }
                ';' => {
                    let stmt = current.trim().to_string();
                    if !stmt.is_empty() {
                        statements.push(stmt);
                    }
                    current.clear();
                }
                _ => current.push(ch),
            }
        }
    }

    let stmt = current.trim().to_string();
    if !stmt.is_empty() {
        statements.push(stmt);
    }

    statements
}

impl DbPool {
    pub async fn export_table_sql(&self, table_name: &str) -> Result<String, sqlx::Error> {
        let is_mysql = matches!(self, DbPool::Mysql(_));
        let q = quote_ident(table_name, is_mysql);
        let fq = match self {
            DbPool::Postgres(_) => format!("\"public\".\"{}\"", table_name),
            _ => q.clone(),
        };

        // Reuse get_table_definition; strip the leading comment line
        let ddl_raw = self.get_table_definition(table_name).await?;
        let ddl = ddl_raw
            .strip_prefix("-- Table Definition\n")
            .unwrap_or(&ddl_raw);

        let mut out = String::new();
        out.push_str(&format!("DROP TABLE IF EXISTS {};\n", fq));
        out.push_str(ddl); // already ends with ";"
        out.push_str("\n\n");

        // Fetch all rows and emit INSERT statements
        let result = self.run_query(&format!("SELECT * FROM {}", fq)).await?;
        if !result.rows.is_empty() {
            let col_list = result
                .columns
                .iter()
                .map(|c| quote_ident(c, is_mysql))
                .collect::<Vec<_>>()
                .join(", ");
            for row in &result.rows {
                let vals = result
                    .columns
                    .iter()
                    .map(|c| value_to_sql(row.get(c).unwrap_or(&Value::Null), is_mysql))
                    .collect::<Vec<_>>()
                    .join(", ");
                out.push_str(&format!(
                    "INSERT INTO {} ({}) VALUES ({});\n",
                    q, col_list, vals
                ));
            }
            out.push('\n');
        }

        Ok(out)
    }

    pub async fn export_database_sql(&self) -> Result<String, sqlx::Error> {
        let driver_name = match self {
            DbPool::Postgres(_) => "PostgreSQL",
            DbPool::Mysql(_) => "MySQL",
            DbPool::Sqlite(_) => "SQLite",
        };
        let timestamp = chrono::Utc::now().format("%Y-%m-%d %H:%M:%S UTC");
        let mut out = format!(
            "-- GSDB SQL Export\n-- Driver: {}\n-- Generated: {}\n\nBEGIN;\n\n",
            driver_name, timestamp
        );

        for table in self.list_tables().await? {
            out.push_str(&format!("-- Table: {}\n", table));
            out.push_str(&self.export_table_sql(&table).await?);
        }

        out.push_str("COMMIT;\n");
        Ok(out)
    }

    pub async fn import_sql(&self, sql: &str, disable_fk_checks: bool) -> Result<usize, sqlx::Error> {
        if disable_fk_checks {
            let stmt = match self {
                DbPool::Postgres(_) => "SET session_replication_role = 'replica'",
                DbPool::Mysql(_) => "SET FOREIGN_KEY_CHECKS = 0",
                DbPool::Sqlite(_) => "PRAGMA foreign_keys = OFF",
            };
            self.run_query(stmt).await?;
        }

        let mut count = 0;
        for stmt in split_sql_statements(sql) {
            self.run_query(&stmt).await?;
            count += 1;
        }

        if disable_fk_checks {
            let stmt = match self {
                DbPool::Postgres(_) => "SET session_replication_role = 'origin'",
                DbPool::Mysql(_) => "SET FOREIGN_KEY_CHECKS = 1",
                DbPool::Sqlite(_) => "PRAGMA foreign_keys = ON",
            };
            self.run_query(stmt).await?;
        }

        Ok(count)
    }
}
