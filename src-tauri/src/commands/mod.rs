use std::collections::HashMap;
use tauri::State;
use tokio::sync::{oneshot, Mutex};

use crate::db::{Connection, DbPool, QueryResult, SchemaGraph, TableInfo};

#[derive(serde::Serialize)]
pub struct FilePreview {
    pub content: String,
    pub truncated: bool,
    pub total_bytes: u64,
}

pub struct AppState {
    pub connections: Mutex<HashMap<String, Connection>>,
    pub pools: Mutex<HashMap<String, DbPool>>,
    pub running_queries: Mutex<HashMap<String, oneshot::Sender<()>>>,
}

impl AppState {
    pub fn new() -> Self {
        Self {
            connections: Mutex::new(HashMap::new()),
            pools: Mutex::new(HashMap::new()),
            running_queries: Mutex::new(HashMap::new()),
        }
    }
}

#[tauri::command]
pub async fn add_connection(
    connection: Connection,
    state: State<'_, AppState>,
) -> Result<(), String> {
    let id = connection.id.clone();

    // Store the connection config first (before attempting to connect)
    // This ensures we can reconnect later even if the initial connection fails
    state
        .connections
        .lock()
        .await
        .insert(id.clone(), connection.clone());

    // Now attempt to connect
    match DbPool::connect(&connection).await {
        Ok(pool) => {
            state.pools.lock().await.insert(id, pool);
            Ok(())
        }
        Err(e) => {
            // Connection failed, but we keep the config stored for future reconnects
            // Remove any existing pool entry
            state.pools.lock().await.remove(&id);
            Err(e.to_string())
        }
    }
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

    // Close the old pool if it exists (important for SQL Server single connections)
    {
        let pools = state.pools.lock().await;
        if let Some(old_pool) = pools.get(&connection_id) {
            // Close the old connection - ignore errors since we're reconnecting anyway
            let _ = old_pool.close().await;
        }
    }

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
    tab_id: String,
    sql: String,
    state: State<'_, AppState>,
) -> Result<QueryResult, String> {
    let pool = {
        let pools = state.pools.lock().await;
        pools
            .get(&connection_id)
            .ok_or_else(|| "Connection not found".to_string())?
            .clone()
    };

    let (cancel_tx, cancel_rx) = oneshot::channel::<()>();
    state
        .running_queries
        .lock()
        .await
        .insert(tab_id.clone(), cancel_tx);

    let result = tokio::select! {
        result = pool.run_query(&sql) => result.map_err(|e| e.to_string()),
        _ = cancel_rx => Err("__cancelled__".to_string()),
    };

    state.running_queries.lock().await.remove(&tab_id);
    result
}

#[tauri::command]
pub async fn cancel_query(tab_id: String, state: State<'_, AppState>) -> Result<(), String> {
    if let Some(tx) = state.running_queries.lock().await.remove(&tab_id) {
        let _ = tx.send(());
    }
    Ok(())
}

#[tauri::command]
pub async fn list_tables(
    connection_id: String,
    state: State<'_, AppState>,
) -> Result<Vec<TableInfo>, String> {
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
pub async fn list_databases(
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

    pool.list_databases().await.map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn create_database(
    connection_id: String,
    db_name: String,
    state: State<'_, AppState>,
) -> Result<(), String> {
    let pool = {
        let pools = state.pools.lock().await;
        pools
            .get(&connection_id)
            .ok_or_else(|| "Connection not found".to_string())?
            .clone()
    };

    pool.create_database(&db_name)
        .await
        .map_err(|e| e.to_string())
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
pub async fn export_table(
    connection_id: String,
    table_name: String,
    file_path: String,
    state: State<'_, AppState>,
) -> Result<(), String> {
    let pool = {
        let pools = state.pools.lock().await;
        pools
            .get(&connection_id)
            .ok_or_else(|| "Connection not found".to_string())?
            .clone()
    };
    let sql = pool
        .export_table_sql(&table_name)
        .await
        .map_err(|e| e.to_string())?;
    std::fs::write(&file_path, sql).map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn export_database(
    connection_id: String,
    file_path: String,
    state: State<'_, AppState>,
) -> Result<(), String> {
    let pool = {
        let pools = state.pools.lock().await;
        pools
            .get(&connection_id)
            .ok_or_else(|| "Connection not found".to_string())?
            .clone()
    };
    let sql = pool
        .export_database_sql()
        .await
        .map_err(|e| e.to_string())?;
    std::fs::write(&file_path, sql).map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn import_sql(
    connection_id: String,
    file_path: String,
    disable_fk_checks: bool,
    state: State<'_, AppState>,
) -> Result<String, String> {
    let pool = {
        let pools = state.pools.lock().await;
        pools
            .get(&connection_id)
            .ok_or_else(|| "Connection not found".to_string())?
            .clone()
    };
    let sql = std::fs::read_to_string(&file_path).map_err(|e| e.to_string())?;
    let count = pool
        .import_sql(&sql, disable_fk_checks)
        .await
        .map_err(|e| e.to_string())?;
    Ok(format!("{} statement(s) executed", count))
}

#[tauri::command]
pub async fn get_schema(
    connection_id: String,
    state: State<'_, AppState>,
) -> Result<SchemaGraph, String> {
    let pool = {
        let pools = state.pools.lock().await;
        pools
            .get(&connection_id)
            .ok_or_else(|| "Connection not found".to_string())?
            .clone()
    };

    pool.get_schema().await.map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn read_file_preview(file_path: String) -> Result<FilePreview, String> {
    let bytes = std::fs::read(&file_path).map_err(|e| e.to_string())?;
    let total_bytes = bytes.len() as u64;
    const MAX_BYTES: usize = 16_384; // 16 KB preview
    let (slice, truncated) = if bytes.len() > MAX_BYTES {
        (&bytes[..MAX_BYTES], true)
    } else {
        (bytes.as_slice(), false)
    };
    let content = String::from_utf8_lossy(slice).into_owned();
    Ok(FilePreview {
        content,
        truncated,
        total_bytes,
    })
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

#[tauri::command]
pub async fn get_column_nullable(
    connection_id: String,
    table_name: String,
    state: State<'_, AppState>,
) -> Result<std::collections::HashMap<String, bool>, String> {
    let pool = {
        let pools = state.pools.lock().await;
        pools
            .get(&connection_id)
            .ok_or_else(|| "Connection not found".to_string())?
            .clone()
    };

    pool.get_column_nullable(&table_name)
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn get_server_info(
    connection_id: String,
    state: State<'_, AppState>,
) -> Result<crate::db::ServerInfo, String> {
    let pool = {
        let pools = state.pools.lock().await;
        pools
            .get(&connection_id)
            .ok_or_else(|| "Connection not found".to_string())?
            .clone()
    };

    pool.get_server_info().await.map_err(|e| e.to_string())
}
