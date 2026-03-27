mod commands;
mod db;

use commands::{add_connection, list_tables, run_query, AppState};

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .manage(AppState::new())
        .invoke_handler(tauri::generate_handler![add_connection, run_query, list_tables])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
