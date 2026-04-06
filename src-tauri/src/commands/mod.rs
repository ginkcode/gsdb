use std::collections::HashMap;
use std::sync::{Arc, atomic::{AtomicBool, Ordering}};
use tauri::State;
use tokio::sync::{oneshot, Mutex};

use crate::db::{Connection, DbPool, ExportProgress, ImportProgress, QueryResult, SchemaGraph, StreamUpdate, TableExportOptions, TableInfo};

// ── Reconnect helpers ─────────────────────────────────────────────────────────

/// Closes the stale pool and establishes a fresh one (including a new SSH tunnel
/// when the connection is configured with SSH). Stores the new pool in AppState.
async fn try_reconnect(connection_id: &str, state: &AppState) -> Result<DbPool, String> {
    let connection = {
        state
            .connections
            .lock()
            .await
            .get(connection_id)
            .cloned()
            .ok_or_else(|| "Connection not found".to_string())?
    };
    if let Some(old) = state.pools.lock().await.get(connection_id) {
        let _ = old.close().await;
    }
    let pool = DbPool::connect(&connection)
        .await
        .map_err(|e| format!("Auto-reconnect failed: {}", e))?;
    state
        .pools
        .lock()
        .await
        .insert(connection_id.to_string(), pool.clone());
    Ok(pool)
}

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
    pub import_cancels: Mutex<HashMap<String, Arc<AtomicBool>>>,
}

impl AppState {
    pub fn new() -> Self {
        Self {
            connections: Mutex::new(HashMap::new()),
            pools: Mutex::new(HashMap::new()),
            running_queries: Mutex::new(HashMap::new()),
            import_cancels: Mutex::new(HashMap::new()),
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

    // Keep the typed DbError so we can inspect it before converting to String.
    let db_result = tokio::select! {
        result = pool.run_query(&sql) => result,
        _ = cancel_rx => {
            state.running_queries.lock().await.remove(&tab_id);
            return Err("__cancelled__".to_string());
        }
    };

    // On a connection error (e.g. after system sleep / SSH tunnel drop), reconnect
    // and retry once. DB errors (syntax, constraints, permissions) are NOT retried.
    let result = match db_result {
        Err(ref e) if e.is_connection_error() => {
            match try_reconnect(&connection_id, &state).await {
                Ok(new_pool) => new_pool.run_query(&sql).await.map_err(|e| e.to_string()),
                Err(_) => db_result.map_err(|e| e.to_string()),
            }
        }
        other => other.map_err(|e| e.to_string()),
    };

    state.running_queries.lock().await.remove(&tab_id);
    result
}

/// Streams query results to the frontend via a Tauri IPC channel.
/// Sends: Header → Rows* → Done | Error | Cancelled
#[tauri::command]
pub async fn run_query_stream(
    connection_id: String,
    tab_id: String,
    sql: String,
    on_event: tauri::ipc::Channel<StreamUpdate>,
    state: State<'_, AppState>,
) -> Result<(), String> {
    let pool = {
        let pools = state.pools.lock().await;
        pools
            .get(&connection_id)
            .ok_or_else(|| "Connection not found".to_string())?
            .clone()
    };

    let (cancel_tx, mut cancel_rx) = oneshot::channel::<()>();
    state
        .running_queries
        .lock()
        .await
        .insert(tab_id.clone(), cancel_tx);

    // Inner streaming loop. Returns:
    //   Ok(true)   — completed normally or cancelled
    //   Ok(false)  — connection error before any data was sent; safe to retry
    async fn do_stream(
        pool: &DbPool,
        sql: &str,
        on_event: &tauri::ipc::Channel<StreamUpdate>,
        cancel_rx: &mut oneshot::Receiver<()>,
        cancelled: &mut bool,
    ) -> Result<bool, String> {
        let (row_tx, mut row_rx) = tokio::sync::mpsc::channel::<StreamUpdate>(64);
        let stream_handle = {
            let pool = pool.clone();
            let sql = sql.to_string();
            tokio::spawn(async move { pool.stream_query(&sql, row_tx).await })
        };

        let mut header_sent = false;

        loop {
            tokio::select! {
                biased;
                _ = &mut *cancel_rx, if !*cancelled => {
                    *cancelled = true;
                    stream_handle.abort();
                    on_event.send(StreamUpdate::Cancelled).ok();
                    return Ok(true);
                }
                msg = row_rx.recv() => {
                    match msg {
                        Some(update) => {
                            if matches!(update, StreamUpdate::Header { .. }) {
                                header_sent = true;
                            }
                            let is_final = matches!(
                                update,
                                StreamUpdate::Done { .. } | StreamUpdate::Error { .. }
                            );
                            on_event.send(update).ok();
                            if is_final { return Ok(true); }
                        }
                        // row_tx was dropped — stream_query returned (Ok or Err).
                        // Await the task to get the typed result; do NOT treat this as success.
                        None => {
                            match stream_handle.await {
                                Ok(Ok(())) => {
                                    // stream_query finished without sending Done — shouldn't
                                    // happen with well-behaved drivers, but handle gracefully.
                                    on_event.send(StreamUpdate::Done { rows_affected: None }).ok();
                                }
                                Ok(Err(e)) => {
                                    // Connection errors before any data → signal retry.
                                    // DB errors (syntax, constraints, etc.) → forward to frontend.
                                    if !header_sent && e.is_connection_error() {
                                        return Ok(false);
                                    }
                                    on_event.send(StreamUpdate::Error { message: e.to_string() }).ok();
                                }
                                Err(join_err) if join_err.is_cancelled() => { /* aborted above */ }
                                Err(join_err) => {
                                    on_event.send(StreamUpdate::Error { message: join_err.to_string() }).ok();
                                }
                            }
                            return Ok(true);
                        }
                    }
                }
            }
        }
    }

    let mut cancelled = false;
    let done = do_stream(&pool, &sql, &on_event, &mut cancel_rx, &mut cancelled).await?;

    // If the stream failed before any data was sent, try reconnecting once.
    if !done && !cancelled {
        match try_reconnect(&connection_id, &state).await {
            Ok(new_pool) => {
                do_stream(&new_pool, &sql, &on_event, &mut cancel_rx, &mut cancelled).await?;
            }
            Err(e) => {
                on_event
                    .send(StreamUpdate::Error { message: e })
                    .ok();
            }
        }
    }

    state.running_queries.lock().await.remove(&tab_id);
    Ok(())
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
pub async fn drop_database(
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

    pool.drop_database(&db_name)
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
    on_event: tauri::ipc::Channel<ExportProgress>,
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
    std::fs::write(&file_path, sql).map_err(|e| e.to_string())?;
    on_event.send(ExportProgress::Done).ok();
    Ok(())
}

#[tauri::command]
pub async fn export_database(
    connection_id: String,
    file_path: String,
    on_event: tauri::ipc::Channel<ExportProgress>,
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
        .export_database_sql(|progress| {
            on_event.send(progress).ok();
        })
        .await
        .map_err(|e| e.to_string())?;
    std::fs::write(&file_path, sql).map_err(|e| e.to_string())?;
    on_event.send(ExportProgress::Done).ok();
    Ok(())
}

#[tauri::command]
pub async fn export_tables(
    connection_id: String,
    tables: Vec<TableExportOptions>,
    file_path: String,
    on_event: tauri::ipc::Channel<ExportProgress>,
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
        .export_tables_sql(&tables, |progress| {
            on_event.send(progress).ok();
        })
        .await
        .map_err(|e| e.to_string())?;
    std::fs::write(&file_path, sql).map_err(|e| e.to_string())?;
    on_event.send(ExportProgress::Done).ok();
    Ok(())
}

#[tauri::command]
pub async fn import_sql(
    connection_id: String,
    file_path: String,
    disable_fk_checks: bool,
    on_event: tauri::ipc::Channel<ImportProgress>,
    state: State<'_, AppState>,
) -> Result<String, String> {
    let pool = {
        let pools = state.pools.lock().await;
        pools
            .get(&connection_id)
            .ok_or_else(|| "Connection not found".to_string())?
            .clone()
    };

    // Register a cancellation flag for this connection's import
    let cancel = Arc::new(AtomicBool::new(false));
    state.import_cancels.lock().await.insert(connection_id.clone(), cancel.clone());

    let sql = std::fs::read_to_string(&file_path).map_err(|e| e.to_string())?;
    let on_event_clone = on_event.clone();
    let result = pool
        .import_sql(&sql, disable_fk_checks, cancel, move |done, total| {
            on_event_clone.send(ImportProgress::Progress { done, total }).ok();
        })
        .await;

    // Always clean up the cancel flag
    state.import_cancels.lock().await.remove(&connection_id);

    match result {
        Ok(count) => {
            on_event.send(ImportProgress::Done { count }).ok();
            Ok(format!("{} statement(s) executed", count))
        }
        Err(crate::db::DbError::Cancelled) => {
            on_event.send(ImportProgress::Cancelled).ok();
            Err("Import cancelled".to_string())
        }
        Err(e) => {
            on_event.send(ImportProgress::Error { message: e.to_string() }).ok();
            Err(e.to_string())
        }
    }
}

#[tauri::command]
pub async fn cancel_import(
    connection_id: String,
    state: State<'_, AppState>,
) -> Result<(), String> {
    if let Some(flag) = state.import_cancels.lock().await.get(&connection_id) {
        flag.store(true, Ordering::Relaxed);
    }
    Ok(())
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
