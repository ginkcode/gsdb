use std::sync::{Arc, atomic::{AtomicBool, Ordering}};
use serde_json::Value;

use super::{DbError, DbPool, Dialect, ExportProgress};

fn quote_ident(name: &str, backtick: bool) -> String {
    if backtick {
        format!("`{}`", name)
    } else {
        format!("\"{}\"", name)
    }
}

fn value_to_sql(value: &Value, dialect: Dialect) -> String {
    let is_mysql = dialect == Dialect::Mysql;
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
        Value::String(s) => {
            // PostgreSQL bytea hex literal: \x... → E'\\xDEADBEEF'::bytea
            if dialect == Dialect::Postgres && s.starts_with("\\x") {
                return format!("E'{}'::bytea", s.replace('\'', "''"));
            }
            // MySQL / SQLite binary blobs encoded as 0x...
            if (dialect == Dialect::Mysql || dialect == Dialect::Sqlite || dialect == Dialect::SqlServer)
                && s.starts_with("0x")
            {
                return s.clone(); // hex literal, no quoting
            }
            format!("'{}'", s.replace('\'', "''"))
        }
        other => format!(
            "'{}'",
            serde_json::to_string(other)
                .unwrap_or_default()
                .replace('\'', "''")
        ),
    }
}

/// Split a SQL string into individual statements, correctly handling single-quoted
/// string literals, dollar-quoted blocks (PostgreSQL), and line comments so that
/// semicolons inside them are not treated as statement boundaries.
fn split_sql_statements(sql: &str) -> Vec<String> {
    let mut statements: Vec<String> = Vec::new();
    let mut current = String::new();
    let mut in_string = false;
    // Dollar-quoting (PostgreSQL): $$...$$  or  $tag$...$tag$
    let mut dollar_tag: Option<String> = None;
    let mut chars = sql.chars().peekable();

    while let Some(ch) = chars.next() {
        // ── Inside a dollar-quoted block ─────────────────────────────────────
        if let Some(ref tag) = dollar_tag.clone() {
            current.push(ch);
            if ch == '$' {
                // Check if the closing tag starts here
                let rest: String = chars.clone().take(tag.len()).collect();
                if rest.starts_with(tag.trim_start_matches('$').trim_end_matches('$')) {
                    // Consume the rest of the closing tag
                    for _ in 0..rest.len() {
                        if let Some(c) = chars.next() {
                            current.push(c);
                        }
                    }
                    dollar_tag = None;
                }
            }
            continue;
        }

        // ── Inside a single-quoted string ────────────────────────────────────
        if in_string {
            current.push(ch);
            if ch == '\'' {
                if chars.peek() == Some(&'\'') {
                    current.push(chars.next().unwrap()); // escaped ''
                } else {
                    in_string = false;
                }
            }
            continue;
        }

        // ── Normal mode ──────────────────────────────────────────────────────
        match ch {
            '\'' => {
                in_string = true;
                current.push(ch);
            }
            // Line comment: skip to end of line
            '-' if chars.peek() == Some(&'-') => {
                for c in chars.by_ref() {
                    if c == '\n' {
                        break;
                    }
                }
            }
            // Dollar-quote opening (PostgreSQL): $$ or $tag$
            '$' => {
                current.push(ch);
                // Peek ahead to find the matching $
                let mut tag = String::from("$");
                let mut found = false;
                let snapshot: Vec<char> = chars.clone().collect();
                let mut consumed = 0;
                for &c in &snapshot {
                    consumed += 1;
                    tag.push(c);
                    if c == '$' {
                        found = true;
                        break;
                    }
                    if !c.is_alphanumeric() && c != '_' {
                        break; // not a valid dollar tag
                    }
                }
                if found {
                    for _ in 0..consumed {
                        if let Some(c) = chars.next() {
                            current.push(c);
                        }
                    }
                    dollar_tag = Some(tag);
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

    let stmt = current.trim().to_string();
    if !stmt.is_empty() {
        statements.push(stmt);
    }

    statements
}

impl DbPool {
    fn driver_name(&self) -> &'static str {
        match self.dialect() {
            Dialect::Postgres => "PostgreSQL",
            Dialect::Mysql => "MySQL",
            Dialect::Sqlite => "SQLite",
            Dialect::SqlServer => "SQL Server",
        }
    }

    /// DDL + INSERTs for one table, no transaction wrapper.
    /// Used as a building block by both single-table and database exports.
    async fn table_sql_body(&self, table_name: &str) -> Result<String, DbError> {
        let is_mysql = self.dialect() == Dialect::Mysql;
        let q = quote_ident(table_name, is_mysql);
        let fq = if self.dialect() == Dialect::Postgres {
            format!("\"public\".\"{}\"", table_name)
        } else {
            q.clone()
        };

        let ddl_raw = self.get_table_definition(table_name).await?;
        let ddl = ddl_raw
            .strip_prefix("-- Table Definition\n")
            .unwrap_or(&ddl_raw);

        let drop = match self.dialect() {
            Dialect::Postgres => format!("DROP TABLE IF EXISTS {} CASCADE;\n", fq),
            _ => format!("DROP TABLE IF EXISTS {};\n", fq),
        };

        let mut out = String::new();
        out.push_str(&drop);
        out.push_str(ddl);
        out.push_str("\n\n");

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
                    .map(|c| value_to_sql(row.get(c).unwrap_or(&Value::Null), self.dialect()))
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

    /// Standalone single-table export.
    /// One BEGIN/COMMIT wrapping custom types + DDL + INSERTs.
    pub async fn export_table_sql(&self, table_name: &str) -> Result<String, DbError> {
        let timestamp = chrono::Utc::now().format("%Y-%m-%d %H:%M:%S UTC");
        let mut out = format!(
            "-- GSDB SQL Export\n-- Driver: {}\n-- Table: {}\n-- Generated: {}\n\nBEGIN;\n\n",
            self.driver_name(), table_name, timestamp
        );

        let types_sql = self.get_table_custom_types_sql(table_name).await?;
        if !types_sql.is_empty() {
            out.push_str(&types_sql);
        }

        out.push_str(&self.table_sql_body(table_name).await?);
        out.push_str("COMMIT;\n");
        Ok(out)
    }

    pub async fn export_database_sql<F>(&self, mut on_progress: F) -> Result<String, DbError>
    where
        F: FnMut(ExportProgress),
    {
        let timestamp = chrono::Utc::now().format("%Y-%m-%d %H:%M:%S UTC");

        // Collect tables (exclude views from INSERT export)
        let all_items = self.list_tables().await?;
        let tables: Vec<_> = all_items.iter().filter(|t| t.kind == "table").collect();
        let views: Vec<_> = all_items.iter().filter(|t| t.kind == "view").collect();

        on_progress(ExportProgress::Started { total_tables: tables.len() });

        let mut out = format!(
            "-- GSDB SQL Export\n-- Driver: {}\n-- Generated: {}\n\nBEGIN;\n\n",
            self.driver_name(), timestamp
        );

        // Emit custom types (ENUMs, DOMAINs) before any table DDL
        let types_sql = self.get_custom_types_sql().await?;
        if !types_sql.is_empty() {
            out.push_str(&types_sql);
        }

        for (i, table) in tables.iter().enumerate() {
            on_progress(ExportProgress::Table {
                name: table.name.clone(),
                index: i,
                total: tables.len(),
            });
            out.push_str(&format!("-- Table: {}\n", table.name));
            // Use table_sql_body (no BEGIN/COMMIT) — the database export has its own
            // single transaction wrapping everything. Calling export_table_sql here
            // would create nested transactions which no database supports.
            out.push_str(&self.table_sql_body(&table.name).await?);
        }

        // FK constraints: emitted after all tables so referenced tables exist
        let fk_sql = self.get_fk_constraints_sql().await?;
        if !fk_sql.is_empty() {
            out.push_str(&fk_sql);
        }

        // Views at the end (they may reference tables)
        for view in &views {
            let ddl_raw = self.get_table_definition(&view.name).await?;
            let ddl = ddl_raw
                .strip_prefix("-- View Definition\n")
                .unwrap_or(&ddl_raw);
            out.push_str(&format!("-- View: {}\n", view.name));
            out.push_str(ddl);
            out.push_str("\n\n");
        }

        out.push_str("COMMIT;\n");
        Ok(out)
    }

    pub async fn import_sql<F>(
        &self,
        sql: &str,
        disable_fk_checks: bool,
        cancel: Arc<AtomicBool>,
        mut on_progress: F,
    ) -> Result<usize, DbError>
    where
        F: FnMut(usize, usize) + Send + 'static,
    {
        let user_stmts = split_sql_statements(sql);
        let user_total = user_stmts.len();

        let fk_disable = match self.dialect() {
            Dialect::Postgres => "SET session_replication_role = 'replica'",
            Dialect::Mysql => "SET FOREIGN_KEY_CHECKS = 0",
            Dialect::Sqlite => "PRAGMA foreign_keys = OFF",
            Dialect::SqlServer => "EXEC sp_MSforeachtable 'ALTER TABLE ? NOCHECK CONSTRAINT ALL'",
        };
        let fk_enable = match self.dialect() {
            Dialect::Postgres => "SET session_replication_role = 'origin'",
            Dialect::Mysql => "SET FOREIGN_KEY_CHECKS = 1",
            Dialect::Sqlite => "PRAGMA foreign_keys = ON",
            Dialect::SqlServer => "EXEC sp_MSforeachtable 'ALTER TABLE ? CHECK CONSTRAINT ALL'",
        };

        let has_own_tx = user_stmts
            .iter()
            .any(|s| s.trim_start().to_uppercase().starts_with("BEGIN"));

        // Build the full statement list to run on a single connection.
        // Prefix (FK disable, BEGIN) → user statements → suffix (COMMIT, FK enable).
        let mut main: Vec<String> = Vec::new();
        if disable_fk_checks {
            main.push(fk_disable.to_string());
        }
        if !has_own_tx {
            main.push("BEGIN".to_string());
        }
        let prefix = main.len(); // how many non-user statements are at the front

        main.extend(user_stmts);

        if !has_own_tx {
            main.push("COMMIT".to_string());
        }
        if disable_fk_checks {
            main.push(fk_enable.to_string());
        }

        // On error: ROLLBACK (if we added BEGIN) + FK re-enable.
        // These run on the SAME connection so they actually take effect.
        let mut on_error: Vec<String> = Vec::new();
        if !has_own_tx {
            on_error.push("ROLLBACK".to_string());
        }
        if disable_fk_checks {
            on_error.push(fk_enable.to_string());
        }

        // Progress callback counts only user statements (skips prefix/suffix).
        // Returns false (cancel) if the cancel flag is set.
        let user_end = prefix + user_total;
        let mut call_count = 0usize;

        self.import_all_statements(
            main,
            on_error,
            Box::new(move || {
                if cancel.load(Ordering::Relaxed) {
                    return false;
                }
                call_count += 1;
                if call_count > prefix && call_count <= user_end {
                    on_progress(call_count - prefix, user_total);
                }
                true
            }),
        )
        .await
    }
}
