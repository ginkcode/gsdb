use std::collections::HashMap;

use async_trait::async_trait;
use serde_json::Value;
use tiberius::xml::XmlData;
use tiberius::{AuthMethod, ColumnType, Config, EncryptionLevel, Row};

use super::driver::{stmt_error, DbError, Dialect, Driver, ServerInfo, StreamUpdate};
use super::types::{
    QueryResult, SchemaColumn, SchemaForeignKey, SchemaGraph, SchemaTable, TableInfo,
};

// ── Driver ────────────────────────────────────────────────────────────────────

pub struct SqlServerDriver {
    pool: bb8::Pool<bb8_tiberius::ConnectionManager>,
}

impl SqlServerDriver {
    pub async fn connect(
        host: &str,
        port: u16,
        database: &str,
        username: &str,
        password: &str,
        ssl_mode: Option<&str>,
    ) -> Result<Self, DbError> {
        let mut config = Config::new();
        config.host(host);
        config.port(port);
        if !database.is_empty() {
            config.database(database);
        }
        config.authentication(AuthMethod::sql_server(username, password));

        match ssl_mode.unwrap_or("prefer") {
            "disable" | "allow" => {
                config.encryption(EncryptionLevel::NotSupported);
            }
            "prefer" | "preferred" | "require" => {
                config.encryption(EncryptionLevel::Required);
                config.trust_cert();
            }
            "verify" | "verify-ca" | "verify-full" => {
                config.encryption(EncryptionLevel::Required);
            }
            _ => {
                config.encryption(EncryptionLevel::Required);
                config.trust_cert();
            }
        }

        let manager = bb8_tiberius::ConnectionManager::new(config);
        let pool = bb8::Pool::builder()
            .max_size(3)
            .build(manager)
            .await
            .map_err(|e| DbError::Config(e.to_string()))?;

        Ok(Self { pool })
    }
}

impl SqlServerDriver {
    /// Returns a rewritten query that CASTs sql_variant columns to NVARCHAR(MAX),
    /// or None if no sql_variant columns exist / detection fails.
    async fn maybe_wrap_variant_sql(&self, sql: &str) -> Option<String> {
        let escaped = sql.replace('\'', "''");
        let meta_sql = format!("EXEC sp_describe_first_result_set N'{escaped}'");

        let mut conn = self.pool.get().await.ok()?;
        let rows = conn
            .simple_query(&meta_sql)
            .await
            .ok()?
            .into_first_result()
            .await
            .ok()?;

        if rows.is_empty() {
            return None;
        }

        // Collect (ordinal, name, is_variant) sorted by ordinal
        let mut cols: Vec<(i32, String, bool)> = rows
            .iter()
            .filter_map(|row| {
                let ordinal: Option<i32> = row.get("column_ordinal");
                let type_id: Option<i32> = row.get("system_type_id");
                let name: Option<&str> = row.get("name");
                Some((ordinal?, name.unwrap_or("").to_string(), type_id == Some(98)))
            })
            .collect();
        cols.sort_by_key(|(ord, _, _)| *ord);

        if !cols.iter().any(|(_, _, is_variant)| *is_variant) {
            return None;
        }

        let select_list: String = cols
            .iter()
            .map(|(_, name, is_variant)| {
                let quoted = format!("[{}]", name.replace(']', "]]"));
                if *is_variant {
                    format!("CAST({quoted} AS NVARCHAR(MAX)) AS {quoted}")
                } else {
                    quoted
                }
            })
            .collect::<Vec<_>>()
            .join(", ");

        Some(format!(
            "SELECT {select_list} FROM ({sql}) AS [_gsdb_wrap]"
        ))
    }
}

#[async_trait]
impl Driver for SqlServerDriver {
    fn dialect(&self) -> Dialect {
        Dialect::SqlServer
    }

    async fn run_query(&self, sql: &str) -> Result<QueryResult, DbError> {
        let sql = self
            .maybe_wrap_variant_sql(sql)
            .await
            .unwrap_or_else(|| sql.to_string());

        let mut conn = self.pool.get().await.map_err(|e| DbError::Config(e.to_string()))?;
        let rows = conn
            .simple_query(&sql)
            .await
            .map_err(|e| DbError::Config(e.to_string()))?
            .into_first_result()
            .await
            .map_err(|e| DbError::Config(e.to_string()))?;

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
            .map(|c| col_type_name(c.column_type()))
            .collect();
        let column_nullable = vec![true; columns.len()];

        let result_rows = rows
            .iter()
            .map(|row| {
                columns
                    .iter()
                    .enumerate()
                    .map(|(i, col)| (col.clone(), mssql_value(row, i)))
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

    async fn stream_query(
        &self,
        sql: &str,
        tx: tokio::sync::mpsc::Sender<StreamUpdate>,
    ) -> Result<(), DbError> {
        const BATCH: usize = 200;

        let sql = self
            .maybe_wrap_variant_sql(sql)
            .await
            .unwrap_or_else(|| sql.to_string());
        use futures::TryStreamExt;

        let mut conn = self.pool.get().await.map_err(|e| DbError::Config(e.to_string()))?;
        let mut tib_stream = conn
            .simple_query(&sql)
            .await
            .map_err(|e| DbError::Config(e.to_string()))?;

        let mut columns: Vec<String> = vec![];
        let mut header_sent = false;
        let mut batch: Vec<std::collections::HashMap<String, serde_json::Value>> =
            Vec::with_capacity(BATCH);

        while let Some(item) = tib_stream
            .try_next()
            .await
            .map_err(|e: tiberius::error::Error| DbError::Config(e.to_string()))?
        {
            match item {
                tiberius::QueryItem::Metadata(meta) if !header_sent => {
                    columns = meta.columns().iter().map(|c| c.name().to_string()).collect();
                    let column_types = meta
                        .columns()
                        .iter()
                        .map(|c| col_type_name(c.column_type()))
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
                tiberius::QueryItem::Row(row) => {
                    let row_map = columns
                        .iter()
                        .enumerate()
                        .map(|(i, col)| (col.clone(), mssql_value(&row, i)))
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
                _ => {}
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
        let mut conn = self.pool.get().await.map_err(|e| DbError::Config(e.to_string()))?;
        let rows = conn
            .simple_query(
                "SELECT TABLE_NAME, TABLE_TYPE \
                 FROM INFORMATION_SCHEMA.TABLES \
                 WHERE TABLE_TYPE IN ('BASE TABLE', 'VIEW') \
                 ORDER BY TABLE_NAME",
            )
            .await
            .map_err(|e| DbError::Config(e.to_string()))?
            .into_first_result()
            .await
            .map_err(|e| DbError::Config(e.to_string()))?;

        Ok(rows
            .iter()
            .map(|r| TableInfo {
                name: r.get::<&str, _>(0).unwrap_or("").to_string(),
                kind: if r.get::<&str, _>(1).unwrap_or("") == "VIEW" {
                    "view".to_string()
                } else {
                    "table".to_string()
                },
            })
            .collect())
    }

    async fn list_databases(&self) -> Result<Vec<String>, DbError> {
        let mut conn = self.pool.get().await.map_err(|e| DbError::Config(e.to_string()))?;
        let rows = conn
            .simple_query("SELECT name FROM sys.databases ORDER BY name")
            .await
            .map_err(|e| DbError::Config(e.to_string()))?
            .into_first_result()
            .await
            .map_err(|e| DbError::Config(e.to_string()))?;

        Ok(rows
            .iter()
            .map(|r| r.get::<&str, _>(0).unwrap_or("").to_string())
            .collect())
    }

    async fn get_column_nullable(
        &self,
        table_name: &str,
    ) -> Result<HashMap<String, bool>, DbError> {
        let mut conn = self.pool.get().await.map_err(|e| DbError::Config(e.to_string()))?;
        let sql = format!(
            "SELECT COLUMN_NAME, IS_NULLABLE \
             FROM INFORMATION_SCHEMA.COLUMNS \
             WHERE TABLE_NAME = '{}' \
             ORDER BY ORDINAL_POSITION",
            table_name.replace('\'', "''")
        );
        let rows = conn
            .simple_query(&sql)
            .await
            .map_err(|e| DbError::Config(e.to_string()))?
            .into_first_result()
            .await
            .map_err(|e| DbError::Config(e.to_string()))?;

        Ok(rows
            .iter()
            .filter_map(|r| {
                let col = r.get::<&str, _>(0)?.to_string();
                let nullable = r.get::<&str, _>(1).unwrap_or("YES") == "YES";
                Some((col, nullable))
            })
            .collect())
    }

    async fn create_database(&self, db_name: &str) -> Result<(), DbError> {
        let sql = format!("CREATE DATABASE [{}]", db_name.replace(']', "]]"));
        self.run_query(&sql).await?;
        Ok(())
    }

    async fn drop_database(&self, db_name: &str) -> Result<(), DbError> {
        let sql = format!("DROP DATABASE [{}]", db_name.replace(']', "]]"));
        self.run_query(&sql).await?;
        Ok(())
    }

    async fn get_schema(&self) -> Result<SchemaGraph, DbError> {
        let mut conn = self.pool.get().await.map_err(|e| DbError::Config(e.to_string()))?;

        let col_sql = "SELECT c.TABLE_NAME, c.COLUMN_NAME, c.DATA_TYPE, c.IS_NULLABLE, \
            CASE WHEN pk.COLUMN_NAME IS NOT NULL THEN 1 ELSE 0 END AS is_pk \
            FROM INFORMATION_SCHEMA.COLUMNS c \
            LEFT JOIN ( \
              SELECT ku.TABLE_NAME, ku.COLUMN_NAME \
              FROM INFORMATION_SCHEMA.TABLE_CONSTRAINTS tc \
              JOIN INFORMATION_SCHEMA.KEY_COLUMN_USAGE ku \
                ON tc.CONSTRAINT_NAME = ku.CONSTRAINT_NAME \
              WHERE tc.CONSTRAINT_TYPE = 'PRIMARY KEY' \
            ) pk ON c.TABLE_NAME = pk.TABLE_NAME AND c.COLUMN_NAME = pk.COLUMN_NAME \
            ORDER BY c.TABLE_NAME, c.ORDINAL_POSITION";

        let col_rows = conn
            .simple_query(col_sql)
            .await
            .map_err(|e| DbError::Config(e.to_string()))?
            .into_first_result()
            .await
            .map_err(|e| DbError::Config(e.to_string()))?;

        let mut tables: Vec<SchemaTable> = Vec::new();
        for row in &col_rows {
            let tbl = row.get::<&str, _>(0).unwrap_or("").to_string();
            let col = SchemaColumn {
                name: row.get::<&str, _>(1).unwrap_or("").to_string(),
                col_type: row.get::<&str, _>(2).unwrap_or("").to_string(),
                nullable: row.get::<&str, _>(3).unwrap_or("YES") == "YES",
                pk: row.get::<i32, _>(4).unwrap_or(0) == 1,
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

        let fk_sql = "SELECT fk.name, tp.name, cp.name, tr.name, cr.name \
            FROM sys.foreign_keys fk \
            JOIN sys.foreign_key_columns fkc ON fk.object_id = fkc.constraint_object_id \
            JOIN sys.tables tp ON fkc.parent_object_id = tp.object_id \
            JOIN sys.columns cp ON fkc.parent_object_id = cp.object_id \
              AND fkc.parent_column_id = cp.column_id \
            JOIN sys.tables tr ON fkc.referenced_object_id = tr.object_id \
            JOIN sys.columns cr ON fkc.referenced_object_id = cr.object_id \
              AND fkc.referenced_column_id = cr.column_id";

        let fk_rows = conn
            .simple_query(fk_sql)
            .await
            .map_err(|e| DbError::Config(e.to_string()))?
            .into_first_result()
            .await
            .map_err(|e| DbError::Config(e.to_string()))?;

        let foreign_keys = fk_rows
            .iter()
            .map(|r| SchemaForeignKey {
                name: r.get::<&str, _>(0).unwrap_or("").to_string(),
                from_table: r.get::<&str, _>(1).unwrap_or("").to_string(),
                from_col: r.get::<&str, _>(2).unwrap_or("").to_string(),
                to_table: r.get::<&str, _>(3).unwrap_or("").to_string(),
                to_col: r.get::<&str, _>(4).unwrap_or("").to_string(),
            })
            .collect();

        Ok(SchemaGraph {
            tables,
            foreign_keys,
        })
    }

    async fn get_table_definition(&self, table_name: &str) -> Result<String, DbError> {
        let mut conn = self.pool.get().await.map_err(|e| DbError::Config(e.to_string()))?;

        // First check if this is a view
        let view_sql = format!(
            "SELECT OBJECT_DEFINITION(OBJECT_ID('{}', 'V'))",
            table_name.replace('\'', "''")
        );
        let view_rows = conn
            .simple_query(&view_sql)
            .await
            .map_err(|e| DbError::Config(e.to_string()))?
            .into_first_result()
            .await
            .map_err(|e| DbError::Config(e.to_string()))?;

        // If it's a view, return the CREATE VIEW statement
        if let Some(row) = view_rows.first() {
            if let Some(def) = row.get::<&str, _>(0) {
                return Ok(format!(
                    "-- View Definition\n{};",
                    def.trim().trim_end_matches(';')
                ));
            }
        }

        // Otherwise, it's a table - get column definitions
        // Columns
        let col_sql = format!(
            "SELECT c.COLUMN_NAME, c.DATA_TYPE, c.IS_NULLABLE, c.COLUMN_DEFAULT, \
             c.CHARACTER_MAXIMUM_LENGTH, c.NUMERIC_PRECISION, c.NUMERIC_SCALE \
             FROM INFORMATION_SCHEMA.COLUMNS c \
             WHERE c.TABLE_NAME = '{}' \
             ORDER BY c.ORDINAL_POSITION",
            table_name.replace('\'', "''")
        );
        let col_rows = conn
            .simple_query(&col_sql)
            .await
            .map_err(|e| DbError::Config(e.to_string()))?
            .into_first_result()
            .await
            .map_err(|e| DbError::Config(e.to_string()))?;

        let mut columns: Vec<String> = Vec::new();
        for row in &col_rows {
            let col_name = row.get::<&str, _>(0).unwrap_or("").to_string();
            let data_type = row.get::<&str, _>(1).unwrap_or("").to_string();
            let is_nullable = row.get::<&str, _>(2).unwrap_or("YES");
            let col_default: Option<&str> = row.get(3);
            let char_max: Option<i32> = row.get(4);
            let num_prec: Option<u8> = row.get(5);
            let num_scale: Option<i32> = row.get(6);

            let full_type = match data_type.as_str() {
                "varchar" | "nvarchar" | "char" | "nchar" | "binary" | "varbinary" => {
                    match char_max {
                        Some(-1) => format!("{}(MAX)", data_type),
                        Some(n) => format!("{}({})", data_type, n),
                        None => data_type.clone(),
                    }
                }
                "decimal" | "numeric" => match (num_prec, num_scale) {
                    (Some(p), Some(s)) => format!("{}({}, {})", data_type, p, s),
                    _ => data_type.clone(),
                },
                _ => data_type.clone(),
            };

            let mut col_def = format!("    [{}] {}", col_name, full_type);
            if is_nullable == "NO" {
                col_def.push_str(" NOT NULL");
            }
            if let Some(default) = col_default {
                col_def.push_str(&format!(" DEFAULT {}", default));
            }
            columns.push(col_def);
        }

        // Primary key
        let pk_sql = format!(
            "SELECT ku.COLUMN_NAME \
             FROM INFORMATION_SCHEMA.TABLE_CONSTRAINTS tc \
             JOIN INFORMATION_SCHEMA.KEY_COLUMN_USAGE ku \
               ON tc.CONSTRAINT_NAME = ku.CONSTRAINT_NAME \
             WHERE tc.TABLE_NAME = '{}' AND tc.CONSTRAINT_TYPE = 'PRIMARY KEY' \
             ORDER BY ku.ORDINAL_POSITION",
            table_name.replace('\'', "''")
        );
        let pk_rows = conn
            .simple_query(&pk_sql)
            .await
            .map_err(|e| DbError::Config(e.to_string()))?
            .into_first_result()
            .await
            .map_err(|e| DbError::Config(e.to_string()))?;

        let pk_columns: Vec<String> = pk_rows
            .iter()
            .filter_map(|r| r.get::<&str, _>(0).map(|s| format!("[{}]", s)))
            .collect();

        let col_block = columns.join(",\n");
        if pk_columns.is_empty() {
            Ok(format!(
                "-- Table Definition\nCREATE TABLE [{}] (\n{}\n);",
                table_name, col_block
            ))
        } else {
            Ok(format!(
                "-- Table Definition\nCREATE TABLE [{}] (\n{},\n    PRIMARY KEY ({})\n);",
                table_name,
                col_block,
                pk_columns.join(", ")
            ))
        }
    }

    async fn import_all_statements(
        &self,
        main: Vec<String>,
        on_error: Vec<String>,
        mut on_stmt_done: Box<dyn FnMut() -> bool + Send>,
    ) -> Result<usize, DbError> {
        let mut conn = self.pool.get().await.map_err(|e| DbError::Config(e.to_string()))?;
        let mut count = 0;
        for (idx, stmt) in main.iter().enumerate() {
            // Collect the result into an owned error string so the QueryStream
            // borrow on conn is fully dropped before we issue cleanup queries.
            let err_msg: Option<String> = match conn.simple_query(stmt.as_str()).await {
                Ok(stream) => {
                    use futures::TryStreamExt;
                    match stream.try_collect::<Vec<_>>().await {
                        Ok(_) => None,
                        Err(e) => Some(e.to_string()),
                    }
                }
                Err(e) => Some(e.to_string()),
            };
            if let Some(msg) = err_msg {
                for s in &on_error {
                    if let Ok(stream) = conn.simple_query(s.as_str()).await {
                        use futures::TryStreamExt;
                        let _ = stream.try_collect::<Vec<_>>().await;
                    }
                }
                return Err(stmt_error(DbError::Config(msg), idx + 1, stmt));
            }
            count += 1;
            if !on_stmt_done() {
                for s in &on_error {
                    if let Ok(stream) = conn.simple_query(s.as_str()).await {
                        use futures::TryStreamExt;
                        let _ = stream.try_collect::<Vec<_>>().await;
                    }
                }
                return Err(DbError::Cancelled);
            }
        }
        Ok(count)
    }

    async fn get_server_info(&self) -> Result<ServerInfo, DbError> {
        let mut conn = self.pool.get().await.map_err(|e| DbError::Config(e.to_string()))?;

        // Get version - use @@VERSION which returns nvarchar
        let version = match conn
            .simple_query("SELECT CAST(@@VERSION AS NVARCHAR(500))")
            .await
        {
            Ok(result) => match result.into_first_result().await {
                Ok(rows) => rows
                    .first()
                    .and_then(|r| r.get::<&str, _>(0))
                    .map(|s| s.lines().next().unwrap_or(s).to_string()),
                Err(_) => None,
            },
            Err(_) => None,
        };

        // Get current database
        let database_name = match conn.simple_query("SELECT DB_NAME()").await {
            Ok(result) => match result.into_first_result().await {
                Ok(rows) => rows
                    .first()
                    .and_then(|r| r.get::<&str, _>(0))
                    .map(|s| s.to_string()),
                Err(_) => None,
            },
            Err(_) => None,
        };

        // Get connection count - may fail without permissions
        let connections = match conn
            .simple_query("SELECT COUNT(*) FROM sys.dm_exec_connections")
            .await
        {
            Ok(result) => match result.into_first_result().await {
                Ok(rows) => rows
                    .first()
                    .and_then(|r| r.get::<i32, _>(0))
                    .map(|n| n as i64),
                Err(_) => None,
            },
            Err(_) => None,
        };

        // Get database size - use VARCHAR to avoid Numeric conversion issues
        let size = match conn.simple_query(
            "SELECT CAST(SUM(size * 8.0 / 1024) AS VARCHAR(20)) FROM sys.master_files WHERE database_id = DB_ID()"
        ).await {
            Ok(result) => match result.into_first_result().await {
                Ok(rows) => rows.first().and_then(|r| r.get::<&str, _>(0)).map(|s| format!("{} MB", s.trim())),
                Err(_) => None,
            },
            Err(_) => None,
        };

        // Get server name - use SERVERPROPERTY with CAST to avoid sql_variant
        let host = match conn
            .simple_query("SELECT CAST(SERVERPROPERTY('ServerName') AS NVARCHAR(128))")
            .await
        {
            Ok(result) => match result.into_first_result().await {
                Ok(rows) => rows
                    .first()
                    .and_then(|r| r.get::<&str, _>(0))
                    .map(|s| s.to_string()),
                Err(_) => None,
            },
            Err(_) => None,
        };

        // Get max connections
        let max_connections = match conn.simple_query("SELECT @@MAX_CONNECTIONS").await {
            Ok(result) => match result.into_first_result().await {
                Ok(rows) => rows
                    .first()
                    .and_then(|r| r.get::<i32, _>(0))
                    .map(|n| n.to_string()),
                Err(_) => None,
            },
            Err(_) => None,
        };

        let mut extra = Vec::new();
        if let Some(max) = max_connections {
            extra.push(("Max Connections".to_string(), max));
        }

        Ok(ServerInfo {
            version,
            database_name,
            connections,
            size,
            host,
            port: None,
            uptime: None,
            extra,
        })
    }

}

// ── Helpers ───────────────────────────────────────────────────────────────────

fn col_type_name(t: ColumnType) -> String {
    match t {
        ColumnType::Int1 => "tinyint",
        ColumnType::Int2 => "smallint",
        ColumnType::Int4 => "int",
        ColumnType::Int8 => "bigint",
        ColumnType::Float4 => "real",
        ColumnType::Float8 => "float",
        ColumnType::Bit | ColumnType::Bitn => "bit",
        ColumnType::Datetime => "datetime",
        ColumnType::Datetime2 => "datetime2",
        ColumnType::Datetimen => "datetime",
        ColumnType::DatetimeOffsetn => "datetimeoffset",
        ColumnType::Daten => "date",
        ColumnType::Timen => "time",
        ColumnType::Money4 => "smallmoney",
        ColumnType::Money => "money",
        ColumnType::Numericn | ColumnType::Decimaln => "decimal",
        ColumnType::Guid => "uniqueidentifier",
        ColumnType::NChar => "nchar",
        ColumnType::NVarchar => "nvarchar",
        ColumnType::BigChar => "char",
        ColumnType::BigVarChar => "varchar",
        ColumnType::BigBinary | ColumnType::BigVarBin => "binary",
        ColumnType::Text => "text",
        ColumnType::NText => "ntext",
        ColumnType::Image => "image",
        ColumnType::Xml => "xml",
        ColumnType::SSVariant => "sql_variant",
        _ => "unknown",
    }
    .to_string()
}

fn mssql_value(row: &Row, idx: usize) -> Value {
    let col_type = row.columns()[idx].column_type();

    match col_type {
        // Integers
        ColumnType::Int1 => {
            if let Some(v) = row.get::<u8, _>(idx) {
                return Value::Number((v as i64).into());
            }
            return Value::Null;
        }
        ColumnType::Int2 => {
            if let Some(v) = row.get::<i16, _>(idx) {
                return Value::Number(i64::from(v).into());
            }
            return Value::Null;
        }
        ColumnType::Int4 => {
            if let Some(v) = row.get::<i32, _>(idx) {
                return Value::Number(i64::from(v).into());
            }
            return Value::Null;
        }
        ColumnType::Int8 => {
            if let Some(v) = row.get::<i64, _>(idx) {
                return Value::Number(v.into());
            }
            return Value::Null;
        }
        // Floats
        ColumnType::Float4 => {
            if let Some(v) = row.get::<f32, _>(idx) {
                if let Some(n) = serde_json::Number::from_f64(v as f64) {
                    return Value::Number(n);
                }
            }
            return Value::Null;
        }
        ColumnType::Float8 => {
            if let Some(v) = row.get::<f64, _>(idx) {
                if let Some(n) = serde_json::Number::from_f64(v) {
                    return Value::Number(n);
                }
            }
            return Value::Null;
        }
        // Decimal / Numeric / Money
        ColumnType::Numericn | ColumnType::Decimaln | ColumnType::Money4 | ColumnType::Money => {
            if let Some(v) = row.get::<f64, _>(idx) {
                if let Some(n) = serde_json::Number::from_f64(v) {
                    return Value::Number(n);
                }
            }
            return Value::Null;
        }
        // Boolean
        ColumnType::Bit | ColumnType::Bitn => {
            if let Some(v) = row.get::<bool, _>(idx) {
                return Value::Bool(v);
            }
            return Value::Null;
        }
        // GUID / uniqueidentifier
        ColumnType::Guid => {
            if let Some(v) = row.get::<uuid::Uuid, _>(idx) {
                return Value::String(v.to_string());
            }
            return Value::Null;
        }
        // Dates and times - Datetime, Datetime2, and Datetimen (legacy datetime)
        ColumnType::Datetime | ColumnType::Datetime2 | ColumnType::Datetimen => {
            if let Some(v) = row.get::<chrono::NaiveDateTime, _>(idx) {
                return Value::String(v.to_string());
            }
            return Value::Null;
        }
        // DateTimeOffset - use chrono::DateTime<FixedOffset>
        ColumnType::DatetimeOffsetn => {
            if let Some(v) = row.get::<chrono::DateTime<chrono::FixedOffset>, _>(idx) {
                return Value::String(v.to_string());
            }
            return Value::Null;
        }
        // Date
        ColumnType::Daten => {
            if let Some(v) = row.get::<chrono::NaiveDate, _>(idx) {
                return Value::String(v.to_string());
            }
            return Value::Null;
        }
        // Time
        ColumnType::Timen => {
            if let Some(v) = row.get::<chrono::NaiveTime, _>(idx) {
                return Value::String(v.to_string());
            }
            return Value::Null;
        }
        // Binary
        ColumnType::BigBinary | ColumnType::BigVarBin | ColumnType::Image => {
            if let Some(v) = row.get::<&[u8], _>(idx) {
                return Value::String(format!("0x{}", hex::encode(v)));
            }
            return Value::Null;
        }
        // XML
        ColumnType::Xml => {
            if let Some(v) = row.get::<&XmlData, _>(idx) {
                return Value::String(v.as_ref().to_string());
            }
            return Value::Null;
        }
        // SQL_VARIANT — try different types since variant can hold anything
        ColumnType::SSVariant => {
            // Try bool first (for Bit values)
            if let Some(v) = row.get::<bool, _>(idx) {
                return Value::Bool(v);
            }
            // Try UUID (for uniqueidentifier values)
            if let Some(v) = row.get::<uuid::Uuid, _>(idx) {
                return Value::String(v.to_string());
            }
            // Try datetime (for DateTime values)
            if let Some(v) = row.get::<chrono::NaiveDateTime, _>(idx) {
                return Value::String(v.to_string());
            }
            // Try integers
            if let Some(v) = row.get::<i64, _>(idx) {
                return Value::Number(v.into());
            }
            if let Some(v) = row.get::<i32, _>(idx) {
                return Value::Number(i64::from(v).into());
            }
            if let Some(v) = row.get::<i16, _>(idx) {
                return Value::Number(i64::from(v).into());
            }
            // Try float
            if let Some(v) = row.get::<f64, _>(idx) {
                if let Some(n) = serde_json::Number::from_f64(v) {
                    return Value::Number(n);
                }
            }
            // Try string
            if let Some(v) = row.get::<&str, _>(idx) {
                return Value::String(v.to_string());
            }
            return Value::Null;
        }
        // All string types — explicit arm so NULL returns Value::Null, not "unsupported type"
        ColumnType::BigVarChar
        | ColumnType::BigChar
        | ColumnType::Text
        | ColumnType::NVarchar
        | ColumnType::NChar
        | ColumnType::NText => {
            return match row.get::<&str, _>(idx) {
                Some(v) => Value::String(v.to_string()),
                None => Value::Null,
            };
        }
        _ => {}
    }

    // Fallback for any remaining type
    if let Some(v) = row.get::<&str, _>(idx) {
        return Value::String(v.to_string());
    }

    // Unknown column type - return type name as placeholder
    Value::String(format!("<unsupported type: {}>", col_type_name(col_type)))
}
