use std::collections::HashMap;
use std::sync::Arc;

use async_trait::async_trait;
use serde_json::Value;
use tiberius::{AuthMethod, Client, ColumnType, Config, EncryptionLevel, Row};
use tokio::net::TcpStream;
use tokio::sync::Mutex;
use tokio_util::compat::{Compat, TokioAsyncWriteCompatExt};

use super::driver::{DbError, Dialect, Driver};
use super::types::{
    QueryResult, SchemaColumn, SchemaForeignKey, SchemaGraph, SchemaTable, TableInfo,
};

// ── Driver ────────────────────────────────────────────────────────────────────

pub struct SqlServerDriver {
    client: Arc<Mutex<Client<Compat<TcpStream>>>>,
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

        // Configure SSL/TLS based on ssl_mode
        // SQL Server SSL modes:
        // - "disable" / "allow" -> No encryption (or encrypt if server supports)
        // - "prefer" / "preferred" -> Encrypt but don't verify certificate (default)
        // - "require" -> Require encryption, don't verify certificate
        // - "verify" / "verify-ca" -> Require encryption and verify certificate
        match ssl_mode.unwrap_or("prefer") {
            "disable" => {
                // No encryption - DANGER_PLAINTEXT
                config.encryption(EncryptionLevel::NotSupported);
            }
            "allow" => {
                // Encrypt if server supports it, allow unencrypted
                // tiberius doesn't have an "allow" equivalent, use NotSupported
                config.encryption(EncryptionLevel::NotSupported);
            }
            "prefer" | "preferred" => {
                // Default: encrypt but trust any certificate
                config.encryption(EncryptionLevel::Required);
                config.trust_cert();
            }
            "require" => {
                // Require encryption, trust any certificate
                config.encryption(EncryptionLevel::Required);
                config.trust_cert();
            }
            "verify" | "verify-ca" | "verify-full" => {
                // Require encryption and verify certificate against system trust store
                config.encryption(EncryptionLevel::Required);
                // Don't call trust_cert() - verify against system certificates
            }
            _ => {
                // Unknown mode, use default (encrypt, trust any cert)
                config.encryption(EncryptionLevel::Required);
                config.trust_cert();
            }
        }

        let tcp = TcpStream::connect(config.get_addr())
            .await
            .map_err(|e| DbError::Config(e.to_string()))?;
        tcp.set_nodelay(true)
            .map_err(|e| DbError::Config(e.to_string()))?;

        let client = Client::connect(config, tcp.compat_write())
            .await
            .map_err(|e| DbError::Config(e.to_string()))?;

        Ok(Self {
            client: Arc::new(Mutex::new(client)),
        })
    }
}

#[async_trait]
impl Driver for SqlServerDriver {
    fn dialect(&self) -> Dialect {
        Dialect::SqlServer
    }

    async fn run_query(&self, sql: &str) -> Result<QueryResult, DbError> {
        let mut client = self.client.lock().await;
        let stream = client
            .simple_query(sql)
            .await
            .map_err(|e| DbError::Config(e.to_string()))?;

        let rows = stream
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

    async fn list_tables(&self) -> Result<Vec<TableInfo>, DbError> {
        let mut client = self.client.lock().await;
        let rows = client
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
        let mut client = self.client.lock().await;
        let rows = client
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
        let mut client = self.client.lock().await;
        let sql = format!(
            "SELECT COLUMN_NAME, IS_NULLABLE \
             FROM INFORMATION_SCHEMA.COLUMNS \
             WHERE TABLE_NAME = '{}' \
             ORDER BY ORDINAL_POSITION",
            table_name.replace('\'', "''")
        );
        let rows = client
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

    async fn get_schema(&self) -> Result<SchemaGraph, DbError> {
        let mut client = self.client.lock().await;

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

        let col_rows = client
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

        let fk_rows = client
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
        let mut client = self.client.lock().await;

        // First check if this is a view
        let view_sql = format!(
            "SELECT OBJECT_DEFINITION(OBJECT_ID('{}', 'V'))",
            table_name.replace('\'', "''")
        );
        let view_rows = client
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
        let col_rows = client
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
        let pk_rows = client
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
        ColumnType::Bit => "bit",
        ColumnType::Datetime => "datetime",
        ColumnType::Datetime2 => "datetime2",
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
        ColumnType::Bit => {
            if let Some(v) = row.get::<bool, _>(idx) {
                return Value::Bool(v);
            }
            return Value::Null;
        }
        // Dates and times — Daten/Timen/DatetimeOffsetn fall through to &str fallback
        // (tiberius FromSql is not implemented for NaiveDate, NaiveTime, DateTime<FixedOffset>)
        ColumnType::Datetime | ColumnType::Datetime2 => {
            if let Some(v) = row.get::<chrono::NaiveDateTime, _>(idx) {
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
        // All string types
        _ => {}
    }

    // String fallback — covers varchar, nvarchar, char, nchar, text, ntext, xml, etc.
    if let Some(v) = row.get::<&str, _>(idx) {
        return Value::String(v.to_string());
    }

    Value::Null
}
