mod commands;
mod db;

use commands::{
    add_connection, create_database, export_database, export_table, get_system_theme,
    get_table_definition, import_sql, list_databases, list_tables, read_file_preview,
    reconnect_connection, run_query, AppState,
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
            get_system_theme,
            export_table,
            export_database,
            import_sql,
            list_databases,
            create_database,
            read_file_preview,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
