use serde_json::Value;

use super::DbPool;

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
            out.push_str(&format!("-- Table: {}\n", table.name));
            out.push_str(&self.export_table_sql(&table.name).await?);
        }

        out.push_str("COMMIT;\n");
        Ok(out)
    }

    pub async fn import_sql(
        &self,
        sql: &str,
        disable_fk_checks: bool,
    ) -> Result<usize, sqlx::Error> {
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
