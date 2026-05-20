pub mod badhub;
pub mod btp;
pub mod commands;
pub mod config;
pub mod sync;
pub mod tablet;

use tauri::menu::{Menu, MenuItem};
use tauri::tray::{TrayIconBuilder, TrayIconEvent};
use tauri::{AppHandle, Manager, WindowEvent};

#[tauri::command]
fn app_version() -> String {
    env!("CARGO_PKG_VERSION").to_string()
}

/// Richtet das Datei-Logging ein: eine tägliche Logdatei `bts-light.log`
/// im App-Log-Verzeichnis. Fehlschläge sind unkritisch – die App läuft
/// auch ohne Log weiter.
fn init_logging(app: &AppHandle) {
    let Ok(dir) = app.path().app_log_dir() else {
        return;
    };
    if std::fs::create_dir_all(&dir).is_err() {
        return;
    }
    let file = tracing_appender::rolling::daily(&dir, "bts-light.log");
    let _ = tracing_subscriber::fmt()
        .with_writer(file)
        .with_ansi(false)
        .try_init();
}

/// Öffnet das Log-Verzeichnis im Datei-Manager.
#[tauri::command]
fn open_log_dir(app: AppHandle) -> Result<(), String> {
    use tauri_plugin_opener::OpenerExt;
    let dir = app.path().app_log_dir().map_err(|e| e.to_string())?;
    app.opener()
        .open_path(dir.to_string_lossy(), None::<String>)
        .map_err(|e| e.to_string())
}

/// Holt das Hauptfenster nach vorn (aus dem Tray heraus).
fn show_main_window(app: &AppHandle) {
    if let Some(window) = app.get_webview_window("main") {
        let _ = window.show();
        let _ = window.set_focus();
    }
}

/// Richtet das System-Tray-Icon mit Kontextmenü ein.
fn setup_tray(app: &AppHandle) -> tauri::Result<()> {
    let open = MenuItem::with_id(app, "open", "BTS Light öffnen", true, None::<&str>)?;
    let quit = MenuItem::with_id(app, "quit", "Beenden", true, None::<&str>)?;
    let menu = Menu::with_items(app, &[&open, &quit])?;

    TrayIconBuilder::new()
        .icon(
            app.default_window_icon()
                .cloned()
                .expect("Fenster-Icon ist konfiguriert"),
        )
        .tooltip("BTS Light")
        .menu(&menu)
        .on_menu_event(|app, event| match event.id.as_ref() {
            "open" => show_main_window(app),
            "quit" => app.exit(0),
            _ => {}
        })
        .on_tray_icon_event(|tray, event| {
            if let TrayIconEvent::DoubleClick { .. } = event {
                show_main_window(tray.app_handle());
            }
        })
        .build(app)?;
    Ok(())
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_updater::Builder::new().build())
        .plugin(tauri_plugin_process::init())
        .manage(commands::AppState::default())
        .invoke_handler(tauri::generate_handler![
            app_version,
            commands::load_config,
            commands::save_config,
            commands::test_btp,
            commands::start_sync,
            commands::stop_sync,
            commands::get_status,
            commands::open_live_view,
            commands::tablet_overview,
            open_log_dir,
        ])
        .setup(|app| {
            init_logging(app.handle());
            tracing::info!("bts-light v{} gestartet", env!("CARGO_PKG_VERSION"));
            setup_tray(app.handle())?;
            Ok(())
        })
        .on_window_event(|window, event| {
            // Das Schließen-Kreuz beendet die App nicht, sondern minimiert
            // sie ins Tray – der Liveticker läuft im Hintergrund weiter.
            if let WindowEvent::CloseRequested { api, .. } = event {
                let _ = window.hide();
                api.prevent_close();
            }
        })
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
