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
        .setup(|_app| {
            // On macOS, transparent: true clears the WKWebView background but the
            // NSWindow background defaults to white. Set it to clear explicitly so
            // the CSS rounded corners show the desktop instead of white in the corners.
            #[cfg(target_os = "macos")]
            {
                use objc::{class, msg_send, sel, sel_impl};
                use tauri::{Manager, WebviewWindowExtMacOS};
                let window = _app.get_webview_window("main").unwrap();
                let ns_win = window.ns_window() as *mut objc::runtime::Object;
                unsafe {
                    let clear: *mut objc::runtime::Object =
                        msg_send![class!(NSColor), clearColor];
                    let _: () = msg_send![ns_win, setBackgroundColor: clear];
                    let _: () = msg_send![ns_win, setOpaque: false as objc::runtime::BOOL];
                }
            }
            Ok(())
        })
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
