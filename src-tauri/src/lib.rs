pub mod android_bridge;
pub mod cli;
pub mod commands;
pub mod pipeline;
pub mod flac;
pub mod image;
pub mod artwork;
pub mod fs;
pub mod logging;
pub mod state;
pub mod util;

use state::app_state::AppState;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    run_with_startup_paths(Vec::new());
}

/// Launch the GUI, optionally pre-loading paths from the command line.
pub fn run_with_startup_paths(startup_paths: Vec<String>) {
    tauri::Builder::default()
        .plugin(android_bridge::plugin())
        .plugin(tauri_plugin_shell::init())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_fs::init())
        .plugin(tauri_plugin_store::Builder::default().build())
        .plugin(tauri_plugin_notification::init())
        .manage(AppState::with_startup_paths(startup_paths))
        .invoke_handler(tauri::generate_handler![
            commands::processing::start_processing,
            commands::processing::cancel_processing,
            commands::processing::get_processing_status,
            commands::processing::get_worker_statuses,
            commands::processing::get_recent_events,
            commands::processing::get_top_compression,
            commands::folders::select_folders,
            commands::folders::select_files,
            commands::folders::select_output_folder,
            commands::folders::is_mobile,
            commands::folders::scan_folders,
            commands::folders::validate_folder,
            commands::folders::get_startup_paths,
            commands::settings::get_settings,
            commands::settings::save_settings,
            commands::settings::get_cpu_count,
            commands::settings::get_default_log_folder,
            commands::logs::get_efc_log,
            commands::logs::write_text_file,
        ])
        .run(tauri::generate_context!())
        .expect("error while running FlacCrunch");
}
