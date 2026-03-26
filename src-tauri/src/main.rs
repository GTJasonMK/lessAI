#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod adapters;
mod commands;
mod documents;
mod models;
mod rewrite;
mod rewrite_jobs;
mod state;
mod storage;

use commands::{
    apply_suggestion, cancel_rewrite, delete_suggestion, dismiss_suggestion, export_document,
    finalize_document, load_session, load_settings, open_document, pause_rewrite, reset_session,
    resume_rewrite, retry_chunk, save_document_edits, save_settings, start_rewrite, test_provider,
};
use state::AppState;

fn main() {
    tauri::Builder::default()
        .manage(AppState::default())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_process::init())
        .setup(|app| {
            #[cfg(desktop)]
            {
                if let Err(error) = app
                    .handle()
                    .plugin(tauri_plugin_updater::Builder::new().build())
                {
                    eprintln!("[WARN] Updater plugin init failed: {error}");
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
            save_document_edits,
            start_rewrite,
            pause_rewrite,
            resume_rewrite,
            cancel_rewrite,
            apply_suggestion,
            dismiss_suggestion,
            delete_suggestion,
            retry_chunk,
            export_document,
            finalize_document
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
