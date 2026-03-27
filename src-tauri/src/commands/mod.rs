use std::collections::HashMap;
use tauri::State;
use tokio::sync::Mutex;

use crate::db::{Connection, DbPool, QueryResult};

pub struct AppState {
    pub connections: Mutex<HashMap<String, Connection>>,
    pub pools: Mutex<HashMap<String, DbPool>>,
}

impl AppState {
    pub fn new() -> Self {
        Self {
            connections: Mutex::new(HashMap::new()),
            pools: Mutex::new(HashMap::new()),
        }
    }
}

#[tauri::command]
pub async fn add_connection(
    connection: Connection,
    state: State<'_, AppState>,
) -> Result<(), String> {
    let pool = DbPool::connect(&connection)
        .await
        .map_err(|e| e.to_string())?;

    let id = connection.id.clone();
    state
        .connections
        .lock()
        .await
        .insert(id.clone(), connection);
    state.pools.lock().await.insert(id, pool);
    Ok(())
}

#[tauri::command]
pub async fn run_query(
    connection_id: String,
    sql: String,
    state: State<'_, AppState>,
) -> Result<QueryResult, String> {
    // Clone the pool (AnyPool is Arc-backed) so we don't hold the lock across await
    let pool = {
        let pools = state.pools.lock().await;
        pools
            .get(&connection_id)
            .ok_or_else(|| "Connection not found".to_string())?
            .clone()
    };

    pool.run_query(&sql).await.map_err(|e| e.to_string())
}
