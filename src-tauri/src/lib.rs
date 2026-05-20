pub mod badhub;
pub mod btp;
pub mod commands;
pub mod config;
pub mod sync;

#[tauri::command]
fn app_version() -> String {
    env!("CARGO_PKG_VERSION").to_string()
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .manage(commands::AppState::default())
        .invoke_handler(tauri::generate_handler![
            app_version,
            commands::load_config,
            commands::save_config,
            commands::test_btp,
            commands::start_sync,
            commands::stop_sync,
            commands::get_status,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
