use std::collections::{HashMap, HashSet, VecDeque};
use std::sync::{Arc, atomic::{AtomicBool, Ordering}};
use serde_json::Value;

use super::{DbError, DbPool, Dialect, ExportProgress, TableExportOptions};

/// Topological sort of `tables` by FK dependencies (Kahn's BFS algorithm).
/// Returns tables ordered so that every FK target appears before the table
/// that references it. Tables in a cycle (or with self-references) are
/// appended at the end in their original order.
fn topological_sort(tables: &[String], fk_edges: &[(String, String)]) -> Vec<String> {
    // Build adjacency: dep_of[A] = set of tables that A must come before (A → B means B depends on A)
    // in_degree[B] = number of tables B depends on
    let table_set: HashSet<&str> = tables.iter().map(|s| s.as_str()).collect();

    let mut in_degree: HashMap<&str, usize> = tables.iter().map(|t| (t.as_str(), 0)).collect();
    // edges: prerequisite → dependent  (prerequisite must come first)
    let mut edges: HashMap<&str, Vec<&str>> = HashMap::new();

    for (from_table, to_table) in fk_edges {
        // `from_table` has a FK pointing to `to_table`, so `to_table` must be exported first.
        // Skip self-references and tables not in the export set.
        if from_table == to_table
            || !table_set.contains(from_table.as_str())
            || !table_set.contains(to_table.as_str())
        {
            continue;
        }
        // to_table → from_table: to_table is a prerequisite of from_table
        edges.entry(to_table.as_str()).or_default().push(from_table.as_str());
        *in_degree.entry(from_table.as_str()).or_insert(0) += 1;
    }

    let mut queue: VecDeque<&str> = in_degree
        .iter()
        .filter(|(_, &d)| d == 0)
        .map(|(&t, _)| t)
        .collect();
    // Sort queue for deterministic output
    let mut queue_vec: Vec<&str> = queue.drain(..).collect();
    queue_vec.sort_unstable();
    queue.extend(queue_vec);

    let mut sorted: Vec<String> = Vec::with_capacity(tables.len());
    while let Some(t) = queue.pop_front() {
        sorted.push(t.to_string());
        if let Some(dependents) = edges.get(t) {
            let mut deps: Vec<&str> = dependents.clone();
            deps.sort_unstable();
            for dep in deps {
                let d = in_degree.entry(dep).or_insert(0);
                *d -= 1;
                if *d == 0 {
                    queue.push_back(dep);
                }
            }
        }
    }

    // Append any tables not reached (cycles) in original order
    let sorted_set: HashSet<String> = sorted.iter().cloned().collect();
    for t in tables {
        if !sorted_set.contains(t.as_str()) {
            sorted.push(t.clone());
        }
    }

    sorted
}

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

    /// DROP + CREATE DDL for one table, no data.
    async fn table_ddl(&self, table_name: &str) -> Result<String, DbError> {
        let is_mysql = self.dialect() == Dialect::Mysql;
        let fq = if self.dialect() == Dialect::Postgres {
            format!("\"public\".\"{}\"", table_name)
        } else {
            quote_ident(table_name, is_mysql)
        };

        let ddl_raw = self.get_table_definition(table_name).await?;
        let ddl = ddl_raw
            .strip_prefix("-- Table Definition\n")
            .unwrap_or(&ddl_raw);

        let drop = match self.dialect() {
            Dialect::Postgres => format!("DROP TABLE IF EXISTS {} CASCADE;\n", fq),
            _ => format!("DROP TABLE IF EXISTS {};\n", fq),
        };

        Ok(format!("{}{}\n\n", drop, ddl))
    }

    /// INSERT statements for all rows in one table, no DDL.
    async fn table_inserts(&self, table_name: &str) -> Result<String, DbError> {
        let is_mysql = self.dialect() == Dialect::Mysql;
        let q = quote_ident(table_name, is_mysql);
        let fq = if self.dialect() == Dialect::Postgres {
            format!("\"public\".\"{}\"", table_name)
        } else {
            q.clone()
        };

        let result = self.run_query(&format!("SELECT * FROM {}", fq)).await?;
        if result.rows.is_empty() {
            return Ok(String::new());
        }

        let col_list = result
            .columns
            .iter()
            .map(|c| quote_ident(c, is_mysql))
            .collect::<Vec<_>>()
            .join(", ");

        let mut out = format!("-- Data: {}\n", table_name);
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
        Ok(out)
    }

    /// Standalone single-table export: DDL then data, wrapped in one transaction.
    pub async fn export_table_sql(&self, table_name: &str) -> Result<String, DbError> {
        let timestamp = chrono::Utc::now().format("%Y-%m-%d %H:%M:%S UTC");
        let mut out = format!(
            "-- GSDB SQL Export\n-- Driver: {}\n-- Table: {}\n-- Generated: {}\n\nBEGIN;\n\n",
            self.driver_name(), table_name, timestamp
        );

        // Custom types used by this table (safe idempotent variant)
        let types_sql = self.get_table_custom_types_sql(table_name).await?;
        if !types_sql.is_empty() {
            out.push_str(&types_sql);
        }

        out.push_str(&self.table_ddl(table_name).await?);
        out.push_str(&self.table_inserts(table_name).await?);
        out.push_str("COMMIT;\n");
        Ok(out)
    }

    /// Export selected tables with per-table structure/data options.
    /// Tables are ordered by FK dependencies (topological sort).
    /// Each entry in `tables` specifies which of structure and data to include.
    pub async fn export_tables_sql<F>(
        &self,
        tables: &[TableExportOptions],
        mut on_progress: F,
    ) -> Result<String, DbError>
    where
        F: FnMut(ExportProgress),
    {
        if tables.is_empty() {
            return Err(DbError::Config("No tables selected for export".into()));
        }

        let timestamp = chrono::Utc::now().format("%Y-%m-%d %H:%M:%S UTC");

        // Get FK edges for topological sort using all selected table names
        let all_names: Vec<String> = tables.iter().map(|t| t.name.clone()).collect();
        let schema = self.get_schema().await?;
        let fk_edges: Vec<(String, String)> = schema.foreign_keys
            .iter()
            .map(|fk| (fk.from_table.clone(), fk.to_table.clone()))
            .collect();

        // Sort all selected tables by FK dependencies, then remap back to options
        let sorted_names = topological_sort(&all_names, &fk_edges);
        let opts_map: std::collections::HashMap<&str, &TableExportOptions> =
            tables.iter().map(|t| (t.name.as_str(), t)).collect();
        let sorted_tables: Vec<&TableExportOptions> = sorted_names
            .iter()
            .filter_map(|n| opts_map.get(n.as_str()).copied())
            .collect();

        on_progress(ExportProgress::Started { total_tables: sorted_tables.len() });

        let mut out = format!(
            "-- GSDB SQL Export\n-- Driver: {}\n-- Generated: {}\n\n",
            self.driver_name(), timestamp
        );

        let any_structure = sorted_tables.iter().any(|t| t.include_structure);
        let any_data = sorted_tables.iter().any(|t| t.include_data);

        // Custom types (for PostgreSQL) — emit if any table includes structure
        if any_structure {
            let types_sql = self.get_custom_types_sql().await?;
            if !types_sql.is_empty() {
                out.push_str(&types_sql);
            }
        }

        out.push_str("BEGIN;\n\n");

        // Section 1: All DDL first (per-table structure flag)
        if any_structure {
            out.push_str("-- Schema\n");
            for opts in &sorted_tables {
                if opts.include_structure {
                    out.push_str(&self.table_ddl(&opts.name).await?);
                }
            }

            // FK constraints after all CREATE TABLEs
            let fk_sql = self.get_fk_constraints_sql().await?;
            if !fk_sql.is_empty() {
                out.push_str(&fk_sql);
            }
        }

        // Section 2: All data after all DDL (per-table data flag)
        if any_data {
            out.push_str("-- Data\n");
            for (i, opts) in sorted_tables.iter().enumerate() {
                if opts.include_data {
                    on_progress(ExportProgress::Table {
                        name: opts.name.clone(),
                        index: i,
                        total: sorted_tables.len(),
                    });
                    out.push_str(&self.table_inserts(&opts.name).await?);
                }
            }
        }

        out.push_str("COMMIT;\n");
        Ok(out)
    }

    pub async fn export_database_sql<F>(&self, mut on_progress: F) -> Result<String, DbError>
    where
        F: FnMut(ExportProgress),
    {
        let timestamp = chrono::Utc::now().format("%Y-%m-%d %H:%M:%S UTC");

        let all_items = self.list_tables().await?;
        let table_names: Vec<String> = all_items.iter().filter(|t| t.kind == "table").map(|t| t.name.clone()).collect();
        let views: Vec<_> = all_items.iter().filter(|t| t.kind == "view").collect();

        // Topological sort — best effort; cycles are appended at end.
        let schema = self.get_schema().await?;
        let fk_edges: Vec<(String, String)> = schema.foreign_keys
            .iter()
            .map(|fk| (fk.from_table.clone(), fk.to_table.clone()))
            .collect();
        let sorted_table_names = topological_sort(&table_names, &fk_edges);

        on_progress(ExportProgress::Started { total_tables: sorted_table_names.len() });

        let mut out = format!(
            "-- GSDB SQL Export\n-- Driver: {}\n-- Generated: {}\n\nBEGIN;\n\n",
            self.driver_name(), timestamp
        );

        // ── Section 1: Custom types ──────────────────────────────────────────
        let types_sql = self.get_custom_types_sql().await?;
        if !types_sql.is_empty() {
            out.push_str(&types_sql);
        }

        // ── Section 2: All DDL (DROP + CREATE TABLE) ─────────────────────────
        // All tables are created before any data is inserted, so circular FK
        // references between tables never cause "no such table" errors.
        out.push_str("-- Schema\n");
        for table_name in &sorted_table_names {
            out.push_str(&self.table_ddl(table_name).await?);
        }

        // FK constraints after all CREATE TABLEs (for drivers that defer them)
        let fk_sql = self.get_fk_constraints_sql().await?;
        if !fk_sql.is_empty() {
            out.push_str(&fk_sql);
        }

        // ── Section 3: All data (INSERT) ─────────────────────────────────────
        // All tables exist now, so inserts succeed regardless of FK order.
        out.push_str("-- Data\n");
        for (i, table_name) in sorted_table_names.iter().enumerate() {
            on_progress(ExportProgress::Table {
                name: table_name.clone(),
                index: i,
                total: sorted_table_names.len(),
            });
            out.push_str(&self.table_inserts(table_name).await?);
        }

        // ── Section 4: Views ─────────────────────────────────────────────────
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
