mod commands;
mod db;

use commands::{
    add_connection, get_system_theme, get_table_definition, list_tables, reconnect_connection,
    run_query, AppState,
};

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_store::Builder::new().build())
        .plugin(tauri_plugin_keyring::init())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_fs::init())
        .plugin(tauri_plugin_window_state::Builder::new().build())
        .manage(AppState::new())
        .invoke_handler(tauri::generate_handler![
            add_connection,
            reconnect_connection,
            run_query,
            list_tables,
            get_table_definition,
            get_system_theme
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
