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
pub async fn reconnect_connection(
    connection_id: String,
    state: State<'_, AppState>,
) -> Result<(), String> {
    // Get the connection config
    let connection = {
        let connections = state.connections.lock().await;
        connections
            .get(&connection_id)
            .cloned()
            .ok_or_else(|| "Connection not found".to_string())?
    };

    // Create a new pool
    let pool = DbPool::connect(&connection)
        .await
        .map_err(|e| format!("Failed to reconnect: {}", e))?;

    // Replace the old pool
    state.pools.lock().await.insert(connection_id, pool);
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

#[tauri::command]
pub async fn list_tables(
    connection_id: String,
    state: State<'_, AppState>,
) -> Result<Vec<String>, String> {
    let pool = {
        let pools = state.pools.lock().await;
        pools
            .get(&connection_id)
            .ok_or_else(|| "Connection not found".to_string())?
            .clone()
    };

    pool.list_tables().await.map_err(|e| e.to_string())
}

#[tauri::command]
pub fn get_system_theme() -> String {
    match dark_light::detect() {
        Ok(dark_light::Mode::Dark) => "dark".to_string(),
        Ok(dark_light::Mode::Light) => "light".to_string(),
        Ok(dark_light::Mode::Unspecified) => "light".to_string(),
        Err(_) => "light".to_string(),
    }
}

#[tauri::command]
pub async fn get_table_definition(
    connection_id: String,
    table_name: String,
    state: State<'_, AppState>,
) -> Result<String, String> {
    let pool = {
        let pools = state.pools.lock().await;
        pools
            .get(&connection_id)
            .ok_or_else(|| "Connection not found".to_string())?
            .clone()
    };

    pool.get_table_definition(&table_name)
        .await
        .map_err(|e| e.to_string())
}
