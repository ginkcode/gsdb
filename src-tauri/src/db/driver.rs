use std::collections::HashMap;

use async_trait::async_trait;

use super::types::{QueryResult, SchemaGraph, TableInfo};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Dialect {
    Postgres,
    Mysql,
    Sqlite,
    SqlServer,
}

#[derive(Debug)]
pub enum DbError {
    Sqlx(sqlx::Error),
    Config(String),
}

impl std::fmt::Display for DbError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            DbError::Sqlx(e) => write!(f, "{e}"),
            DbError::Config(e) => write!(f, "{e}"),
        }
    }
}

impl std::error::Error for DbError {}

impl From<sqlx::Error> for DbError {
    fn from(e: sqlx::Error) -> Self {
        DbError::Sqlx(e)
    }
}

#[async_trait]
pub trait Driver: Send + Sync {
    fn dialect(&self) -> Dialect;

    async fn run_query(&self, sql: &str) -> Result<QueryResult, DbError>;

    async fn list_tables(&self) -> Result<Vec<TableInfo>, DbError>;

    async fn list_databases(&self) -> Result<Vec<String>, DbError>;

    async fn get_column_nullable(
        &self,
        table_name: &str,
    ) -> Result<HashMap<String, bool>, DbError>;

    async fn create_database(&self, db_name: &str) -> Result<(), DbError>;

    async fn get_table_definition(&self, table_name: &str) -> Result<String, DbError>;

    async fn get_schema(&self) -> Result<SchemaGraph, DbError>;
}
