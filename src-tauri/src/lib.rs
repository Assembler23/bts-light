pub mod aushang;
pub mod azure_tts;
pub mod badhub;
pub mod btp;
pub mod commands;
pub mod config;
pub mod hall_colors;
pub mod log_upload;
pub mod print;
pub mod sync;
pub mod tablet;

use tauri::menu::{Menu, MenuItem};
use tauri::tray::{TrayIconBuilder, TrayIconEvent};
use tauri::{AppHandle, Manager, WindowEvent};
use tauri_plugin_dialog::{DialogExt, MessageDialogButtons};

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
            // Über `beenden`, nicht `app.exit` direkt: Seit der Entprellung
            // (Spec monitor-livestand-push, S2) schreibt nicht mehr jeder
            // Punkt selbst, und dies ist der dokumentierte Weg, eine in den
            // Infobereich minimierte App zu schließen — ohne den letzten
            // Schreibvorgang ginge hier Spielstand verloren.
            "quit" => beenden(app),
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
        // Single-Instance MUSS das erste Plugin sein. Verhindert eine
        // zweite bts-light-Instanz – sonst kollidiert deren Tablet-Server
        // mit dem Port 8088 der schon laufenden Instanz.
        .plugin(tauri_plugin_single_instance::init(|app, _args, _cwd| {
            show_main_window(app);
        }))
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_updater::Builder::new().build())
        .plugin(tauri_plugin_process::init())
        .plugin(tauri_plugin_dialog::init())
        .manage(commands::AppState::default())
        .invoke_handler(tauri::generate_handler![
            app_version,
            commands::load_config,
            commands::save_config,
            commands::test_btp,
            commands::azure_tts_speak,
            commands::start_sync,
            commands::stop_sync,
            commands::flush_live_scores,
            commands::get_status,
            commands::wifi_status,
            commands::internet_status,
            commands::open_live_view,
            commands::open_external,
            commands::tablet_overview,
            commands::walkover_proposals,
            commands::dismiss_walkover,
            commands::confirm_walkover,
            commands::enter_result,
            commands::disqualify_match,
            commands::export_identity,
            commands::import_identity,
            commands::scorekeeper_queue,
            commands::remove_scorekeeper,
            commands::advance_scorekeeper,
            commands::add_scorekeeper,
            commands::assign_court,
            commands::free_court,
            commands::set_court_locked,
            commands::preparation_candidates,
            commands::tournament_draws,
            commands::tournament_stats,
            commands::fetch_pronunciations,
            commands::share_pronunciations,
            commands::checkin_state,
            commands::checkin_set_player,
            commands::checkin_set_times,
            commands::checkin_announcement,
            commands::finished_matches,
            commands::match_timeline,
            commands::match_scoresheet_html,
            commands::aushang_html,
            commands::printer_list,
            commands::print_scoresheet,
            commands::print_warning,
            commands::clear_print_warning,
            commands::officials_roster,
            commands::official_assign,
            commands::official_clear,
            commands::official_pause,
            commands::auto_assign_exclude,
            commands::queue_reorder,
            commands::queue_order_reset,
            commands::official_reorder,
            commands::official_set_club,
            commands::official_blocklists,
            commands::official_set_blocklists,
            commands::official_appearances,
            commands::officials_court_switches,
            commands::officials_set_court_switches,
            commands::publish_freetext,
            commands::pending_freetext,
            commands::pending_announce_jobs,
            commands::note_court_call,
            commands::tl_web_info,
            commands::tl_device_add,
            commands::tl_device_remove,
            commands::tl_web_set_enabled,
            commands::set_hall_layout,
            commands::remove_hall_layout,
            commands::set_hall_color,
            commands::remove_hall_color,
            commands::hall_colors_view,
            commands::cloud_announce_state,
            commands::cloud_slaves,
            commands::pairing_code,
            commands::resolve_pairing_code,
            commands::slave_devices,
            commands::call_preparation,
            commands::retract_preparation,
            commands::winners_overview,
            commands::set_winners_selection,
            commands::add_court_ad,
            commands::remove_court_ad,
            commands::list_court_ads,
            commands::set_court_ad_label,
            commands::set_court_ad_bar,
            commands::set_court_ad_style,
            commands::read_tournament_logo,
            commands::monitor_devices,
            commands::assign_monitor,
            commands::set_monitor_hall,
            commands::monitor_command,
            commands::forget_monitor_device,
            open_log_dir,
        ])
        .setup(|app| {
            init_logging(app.handle());
            tracing::info!("bts-light v{} gestartet", env!("CARGO_PKG_VERSION"));
            setup_tray(app.handle())?;
            Ok(())
        })
        .on_window_event(|window, event| {
            // Das Schließen-Kreuz beendet bts-light wirklich. Läuft gerade
            // ein Liveticker, wird vorher rückgefragt, damit nicht
            // versehentlich der Live-Feed eines Turniers abreißt. Für
            // Hintergrundbetrieb das Fenster minimieren statt schließen.
            if let WindowEvent::CloseRequested { api, .. } = event {
                // Schließen selbst übernehmen und dann gezielt beenden –
                // unabhängig vom Tauri-Standardverhalten.
                api.prevent_close();
                let app = window.app_handle().clone();
                let sync_running = app
                    .state::<commands::AppState>()
                    .status
                    .lock()
                    .map(|s| s.running)
                    .unwrap_or(false);
                if sync_running {
                    let app_for_dialog = app.clone();
                    app.dialog()
                        .message(
                            "Der Liveticker läuft noch – beim Beenden hört \
                             bts-light auf, Ergebnisse zu senden. Trotzdem \
                             beenden?",
                        )
                        .title("bts-light beenden?")
                        .buttons(MessageDialogButtons::OkCancelCustom(
                            "Beenden".to_string(),
                            "Abbrechen".to_string(),
                        ))
                        .show(move |confirmed| {
                            if confirmed {
                                beenden(&app_for_dialog);
                            }
                        });
                } else {
                    beenden(&app);
                }
            }
        })
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}

/// Beendet die App — aber erst, nachdem der aufgelaufene Live-Stand auf der
/// Platte steht (Spec `monitor-livestand-push`, S2).
///
/// Seit der Entprellung schreibt nicht mehr jeder gezählte Punkt selbst,
/// sondern ein Sekundentakt. Ohne diesen letzten Schreibvorgang gingen beim
/// Beenden bis zu einer Sekunde Spielstand verloren — und anders als bei
/// einem Absturz gibt es hier keinen Grund, das hinzunehmen.
fn beenden(app: &tauri::AppHandle) {
    app.state::<commands::AppState>().tablet.flush_scores();
    app.exit(0);
}
