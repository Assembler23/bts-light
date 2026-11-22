// Prevents additional console window on Windows in release, DO NOT REMOVE!!
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    // Sonderaufrufe des Kernwechsels (Probelauf, Wächter; Spec
    // hotfix-ohne-neustart): Sie müssen enden, BEVOR Tauri startet — ohne
    // Fenster und ohne die Einzelinstanz-Sperre, die sonst jede zweite
    // bts-light sofort beendet.
    // `args_os` statt `args`: Letzteres bricht bei einem Argument ab, das
    // kein gültiger Text ist — und das träfe JEDEN Start.
    let args: Vec<String> = std::env::args_os()
        .skip(1)
        .map(|a| a.to_string_lossy().into_owned())
        .collect();
    // Betrieb ohne Oberfläche (`--kopflos <ordner>`, ein Punktspiel auf dem
    // Server): ebenfalls vor Tauri — dort gibt es kein Display.
    if let Some(code) = bts_light_lib::kopflos_dienst::einstieg(&args) {
        std::process::exit(code);
    }
    if let Some(code) = bts_light_lib::kopflos::einstieg(&args) {
        std::process::exit(code);
    }
    if let Some(code) = bts_light_lib::tausch::einstieg(&args) {
        std::process::exit(code);
    }
    bts_light_lib::run()
}
