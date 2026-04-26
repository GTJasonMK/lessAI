#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod adapters;
mod commands;
mod core;
mod domain;
mod documents;
mod editor;
mod rewrite;
mod rewrite_core;
mod rewrite_unit;
mod session;
#[cfg(test)]
mod test_support;
mod textual_template;

pub(crate) use core::{
    atomic_write, document_snapshot, network_proxy, observability, persist, result_flow,
    settings_validation, state, storage, text_boundaries,
};
pub(crate) use domain::models;
pub(crate) use editor::{editor_session, editor_writeback};
pub(crate) use rewrite_core::{
    rewrite_batch_commit, rewrite_job_state, rewrite_jobs, rewrite_permissions, rewrite_projection,
    rewrite_targets, rewrite_writeback,
};
pub(crate) use session::{
    session_access, session_builder, session_capability_models, session_edit, session_flow,
    session_loader, session_messages, session_refresh,
};

use commands::{
    apply_suggestion, cancel_rewrite, close_main_window, delete_suggestion, dismiss_suggestion,
    export_document, finalize_document, install_system_package_release,
    is_main_window_maximized, list_release_versions, load_session, load_settings,
    minimize_main_window, open_document, pause_rewrite, reset_session, resume_rewrite,
    retry_rewrite_unit, rewrite_selection, run_document_writeback, save_settings,
    start_drag_main_window, start_resize_main_window, start_rewrite, switch_release_version,
    test_provider, toggle_maximize_main_window,
};
use state::AppState;
use tauri_plugin_log::{Target, TargetKind, TimezoneStrategy};

#[cfg(target_os = "linux")]
fn apply_linux_graphics_compat_env() {
    #[derive(Clone, Copy, PartialEq, Eq)]
    enum GraphicsMode {
        Native,
        Auto,
        Safe,
    }

    fn set_if_unset(name: &str, value: &str) {
        match std::env::var_os(name) {
            Some(existing) if !existing.is_empty() => {}
            _ => unsafe { std::env::set_var(name, value) },
        }
    }

    fn parse_graphics_mode(appimage_runtime: bool) -> (GraphicsMode, bool) {
        match std::env::var("LESSAI_LINUX_GRAPHICS_MODE") {
            Ok(raw_mode) => {
                let mode = raw_mode.to_ascii_lowercase();
                let parsed = match mode.as_str() {
                    "native" => GraphicsMode::Native,
                    "safe" => GraphicsMode::Safe,
                    "auto" => GraphicsMode::Auto,
                    _ => {
                        eprintln!(
                            "unknown LESSAI_LINUX_GRAPHICS_MODE='{raw_mode}', fallback to auto"
                        );
                        GraphicsMode::Auto
                    }
                };
                (parsed, true)
            }
            Err(_) => {
                let force_gpu = std::env::var("LESSAI_FORCE_GPU")
                    .map(|value| value == "1")
                    .unwrap_or(false);
                if force_gpu {
                    (GraphicsMode::Native, false)
                } else if appimage_runtime {
                    (GraphicsMode::Safe, false)
                } else {
                    (GraphicsMode::Auto, false)
                }
            }
        }
    }

    fn apply_safe_mode(session_is_wayland: bool, has_wayland: bool, has_x11: bool) {
        set_if_unset("WEBKIT_DISABLE_DMABUF_RENDERER", "1");
        set_if_unset("WEBKIT_DISABLE_COMPOSITING_MODE", "1");
        set_if_unset("WEBKIT_DMABUF_RENDERER_DISABLE_GBM", "1");
        set_if_unset("GSK_RENDERER", "cairo");
        set_if_unset("LIBGL_ALWAYS_SOFTWARE", "1");
        set_if_unset("NO_AT_BRIDGE", "1");

        if has_x11 {
            if session_is_wayland {
                unsafe {
                    std::env::remove_var("WAYLAND_DISPLAY");
                }
            }
            set_if_unset("GDK_BACKEND", "x11");
            set_if_unset("EGL_PLATFORM", "x11");
        } else if has_wayland {
            set_if_unset("GDK_BACKEND", "wayland");
            set_if_unset("EGL_PLATFORM", "wayland");
        }
    }

    let session_type = std::env::var("XDG_SESSION_TYPE")
        .unwrap_or_default()
        .to_ascii_lowercase();
    let has_wayland = std::env::var_os("WAYLAND_DISPLAY").is_some();
    let has_x11 = std::env::var_os("DISPLAY").is_some();
    let session_is_wayland = session_type == "wayland" || has_wayland;
    let appimage_runtime = std::env::var_os("APPIMAGE").is_some();

    let (graphics_mode, has_explicit_graphics_mode) = parse_graphics_mode(appimage_runtime);
    if appimage_runtime && !has_explicit_graphics_mode && graphics_mode == GraphicsMode::Safe {
        eprintln!(
            "AppImage detected: defaulting LESSAI_LINUX_GRAPHICS_MODE to safe. \
Set LESSAI_LINUX_GRAPHICS_MODE=auto or native to override."
        );
    }

    match graphics_mode {
        GraphicsMode::Native => {
            // Keep full native behavior. User/environment controls all graphics vars.
        }
        GraphicsMode::Auto => {
            // Prefer the user's current desktop session first; do not hard-force missing backends.
            if std::env::var_os("GDK_BACKEND").is_none() {
                match (has_wayland, has_x11) {
                    (true, true) => {
                        if session_type == "x11" {
                            set_if_unset("GDK_BACKEND", "x11,wayland");
                        } else {
                            set_if_unset("GDK_BACKEND", "wayland,x11");
                        }
                    }
                    (true, false) => set_if_unset("GDK_BACKEND", "wayland"),
                    (false, true) => set_if_unset("GDK_BACKEND", "x11"),
                    (false, false) => {}
                }
            }

            if std::env::var_os("EGL_PLATFORM").is_none() {
                if session_type == "x11" && has_x11 {
                    set_if_unset("EGL_PLATFORM", "x11");
                } else if session_is_wayland && has_wayland {
                    set_if_unset("EGL_PLATFORM", "wayland");
                } else if has_x11 {
                    set_if_unset("EGL_PLATFORM", "x11");
                }
            }

            // AppImage on Linux can still hit dmabuf/gbm regressions on some stacks.
            // Keep mitigation minimal in auto mode and avoid forcing software render.
            if appimage_runtime {
                set_if_unset("WEBKIT_DISABLE_DMABUF_RENDERER", "1");
                set_if_unset("WEBKIT_DMABUF_RENDERER_DISABLE_GBM", "1");
            }
        }
        GraphicsMode::Safe => {
            apply_safe_mode(session_is_wayland, has_wayland, has_x11);
            if appimage_runtime {
                eprintln!(
                    "linux graphics safe-mode enabled: WEBKIT_DISABLE_COMPOSITING_MODE=1, WEBKIT_DISABLE_DMABUF_RENDERER=1, WEBKIT_DMABUF_RENDERER_DISABLE_GBM=1, GSK_RENDERER=cairo, LIBGL_ALWAYS_SOFTWARE=1"
                );
            }
        }
    }
}

#[cfg(not(target_os = "linux"))]
fn apply_linux_graphics_compat_env() {}

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
    apply_linux_graphics_compat_env();

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
            list_release_versions,
            switch_release_version,
            install_system_package_release,
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
