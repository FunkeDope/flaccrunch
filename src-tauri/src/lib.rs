pub mod commands;
pub mod pipeline;
pub mod flac;
pub mod image;
pub mod artwork;
pub mod fs;
pub mod logging;
pub mod state;
pub mod sidecar;
pub mod util;

use state::app_state::AppState;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_shell::init())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_store::Builder::default().build())
        .plugin(tauri_plugin_notification::init())
        .manage(AppState::default())
        .invoke_handler(tauri::generate_handler![
            commands::processing::start_processing,
            commands::processing::cancel_processing,
            commands::processing::get_processing_status,
            commands::processing::get_worker_statuses,
            commands::processing::get_recent_events,
            commands::processing::get_top_compression,
            commands::folders::select_folders,
            commands::folders::scan_folders,
            commands::folders::validate_folder,
            commands::settings::get_settings,
            commands::settings::save_settings,
            commands::settings::get_cpu_count,
            commands::settings::get_default_log_folder,
            commands::logs::get_run_log,
            commands::logs::get_summary_log,
            commands::logs::open_log_folder,
        ])
        .run(tauri::generate_context!())
        .expect("error while running FlacCrunch");
}
