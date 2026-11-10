//! Keelson desktop shell: the engine behind a Svelte window.

#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod catalog;
mod checkpoints;
mod commands;
mod config;
mod sessions;
mod state;

use std::sync::Arc;

use tauri::Manager;

fn main() {
    // Same credential source as the CLI: the launch directory and parents.
    dotenvy::dotenv().ok();
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .setup(|app| {
            app.manage(Arc::new(state::DesktopState::new(app.handle().clone())));
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::setup_state,
            commands::model_catalog,
            commands::save_key,
            commands::save_model,
            commands::save_rig,
            commands::save_locale,
            commands::complete_onboarding,
            commands::open_session,
            commands::reopen_session,
            commands::new_session,
            commands::attach_files,
            commands::branch_session,
            commands::list_sessions,
            commands::delete_session,
            commands::rename_session,
            commands::archive_session,
            commands::send_message,
            commands::cancel_turn,
            commands::undo_last_turn,
            commands::turn_changes,
            commands::session_status,
            commands::background_tasks,
            commands::save_permission,
            commands::load_turns,
            commands::respond_interaction,
        ])
        .run(tauri::generate_context!())
        .expect("keelson desktop failed to start");
}
