//! Keelson desktop shell: the engine behind a Svelte window.

#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod commands;
mod config;
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
            commands::save_key,
            commands::save_model,
            commands::save_locale,
            commands::complete_onboarding,
            commands::open_session,
            commands::send_message,
            commands::cancel_turn,
            commands::load_turns,
            commands::respond_interaction,
        ])
        .run(tauri::generate_context!())
        .expect("keelson desktop failed to start");
}
