#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod adapters;
mod atomic_write;
mod commands;
mod document_snapshot;
mod documents;
mod editor_session;
mod editor_writeback;
mod models;
mod observability;
mod persist;
mod result_flow;
mod rewrite;
mod rewrite_batch_commit;
mod rewrite_job_state;
mod rewrite_jobs;
mod rewrite_permissions;
mod rewrite_projection;
mod rewrite_targets;
mod rewrite_unit;
mod rewrite_writeback;
mod session_access;
mod session_builder;
mod session_capability_models;
mod session_edit;
mod session_flow;
mod session_loader;
mod session_messages;
mod session_refresh;
mod settings_validation;
mod state;
mod storage;
#[cfg(test)]
mod test_support;
mod text_boundaries;
mod textual_template;

use commands::{
    apply_suggestion, cancel_rewrite, close_main_window, delete_suggestion, dismiss_suggestion,
    export_document, finalize_document, is_main_window_maximized, load_session, load_settings,
    minimize_main_window, open_document, pause_rewrite, reset_session, resume_rewrite,
    retry_rewrite_unit, rewrite_selection, run_document_writeback, save_settings,
    start_drag_main_window, start_resize_main_window, start_rewrite, test_provider,
    toggle_maximize_main_window,
};
use state::AppState;
use tauri_plugin_log::{Target, TargetKind, TimezoneStrategy};

fn build_log_plugin<R: tauri::Runtime>() -> tauri::plugin::TauriPlugin<R> {
    tauri_plugin_log::Builder::new()
        .level(log::LevelFilter::Info)
        .timezone_strategy(TimezoneStrategy::UseLocal)
        .targets([
            Target::new(TargetKind::LogDir { file_name: None }),
            Target::new(TargetKind::Stdout),
            Target::new(TargetKind::Webview),
        ])
        .build()
}

fn main() {
    tauri::Builder::default()
        .manage(AppState::default())
        .plugin(build_log_plugin())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_process::init())
        .setup(|app| {
            #[cfg(desktop)]
            {
                if let Err(error) = app
                    .handle()
                    .plugin(tauri_plugin_updater::Builder::new().build())
                {
                    log::warn!("updater plugin init failed: error={error}");
                }
            }
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            load_settings,
            save_settings,
            test_provider,
            open_document,
            load_session,
            reset_session,
            run_document_writeback,
            start_rewrite,
            pause_rewrite,
            resume_rewrite,
            cancel_rewrite,
            rewrite_selection,
            apply_suggestion,
            dismiss_suggestion,
            delete_suggestion,
            retry_rewrite_unit,
            export_document,
            finalize_document,
            is_main_window_maximized,
            minimize_main_window,
            toggle_maximize_main_window,
            close_main_window,
            start_drag_main_window,
            start_resize_main_window
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
