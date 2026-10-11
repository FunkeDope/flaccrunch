//! Tauri application wiring (GUI feature only).

use crate::commands;
use crate::state::app_state::AppState;

/// Launch the GUI, optionally pre-loading paths from the command line.
pub fn run_with_startup_paths(startup_paths: Vec<String>) {
    tauri::Builder::default()
        .plugin(crate::android_bridge::plugin())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_fs::init())
        .manage(AppState::with_startup_paths(startup_paths))
        .invoke_handler(tauri::generate_handler![
            commands::processing::start_processing,
            commands::processing::cancel_processing,
            commands::processing::enqueue_files,
            commands::folders::select_folders,
            commands::folders::select_files,
            commands::folders::is_mobile,
            commands::folders::scan_folders,
            commands::folders::get_startup_paths,
            commands::settings::get_settings,
            commands::settings::save_settings,
            commands::settings::get_cpu_count,
            commands::settings::get_default_log_folder,
            commands::logs::get_efc_log,
            commands::logs::write_text_file,
            commands::logs::copy_file_to_path,
        ])
        .run(tauri::generate_context!())
        .expect("error while running FlacCrunch");
}
