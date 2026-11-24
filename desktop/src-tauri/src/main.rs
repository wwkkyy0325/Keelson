//! Keelson desktop shell: the engine behind a Svelte window.

#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod build;
mod catalog;
mod checkpoints;
mod commands;
mod config;
mod perf;
mod records;
mod sessions;
mod state;
mod taskdoc;

use std::sync::Arc;

use tauri::menu::{MenuBuilder, MenuItemBuilder, PredefinedMenuItem, SubmenuBuilder};
use tauri::Manager;

/// The last resort when the JS thread itself is stuck: this runs on the native
/// side, so it still works while the webview cannot paint or dispatch. It drops
/// every open conversation (cancelling the turns streaming into them) and then
/// reloads the page, which boots a fresh engine and reopens the session.
fn force_recover_now(app: &tauri::AppHandle) {
    let shared = app.state::<Arc<state::DesktopState>>();
    let forgotten = shared.inner().forget_open();
    eprintln!("force recovery: dropped {forgotten} open conversation(s)");
    if let Some(window) = app.get_webview_window("main") {
        // `reload` goes through the webview dispatcher, not through JS: a
        // wedged page can still be replaced from here.
        if let Err(error) = window.reload() {
            eprintln!("force recovery: reload failed: {error}");
        }
    }
}

/// A menu is built by hand (rather than the platform default) only to own the
/// single item that must keep working while the page does not: ⌘⇧R.
fn build_menu(app: &mut tauri::App) -> tauri::Result<()> {
    let recover = MenuItemBuilder::with_id("force-recover", "强制恢复")
        .accelerator("CmdOrCtrl+Shift+R")
        .build(app)?;

    let app_menu = SubmenuBuilder::new(app, "Keelson")
        .item(&PredefinedMenuItem::about(app, None, None)?)
        .separator()
        .item(&recover)
        .separator()
        .item(&PredefinedMenuItem::hide(app, None)?)
        .item(&PredefinedMenuItem::hide_others(app, None)?)
        .item(&PredefinedMenuItem::show_all(app, None)?)
        .separator()
        .item(&PredefinedMenuItem::quit(app, None)?)
        .build()?;

    // Replacing the menu drops the default Edit/Window items, and with them
    // ⌘C/⌘V, so they are rebuilt explicitly.
    let edit_menu = SubmenuBuilder::new(app, "编辑")
        .item(&PredefinedMenuItem::undo(app, None)?)
        .item(&PredefinedMenuItem::redo(app, None)?)
        .separator()
        .item(&PredefinedMenuItem::cut(app, None)?)
        .item(&PredefinedMenuItem::copy(app, None)?)
        .item(&PredefinedMenuItem::paste(app, None)?)
        .item(&PredefinedMenuItem::select_all(app, None)?)
        .build()?;

    let window_menu = SubmenuBuilder::new(app, "窗口")
        .item(&PredefinedMenuItem::minimize(app, None)?)
        .item(&PredefinedMenuItem::close_window(app, None)?)
        .build()?;

    let menu = MenuBuilder::new(app)
        .item(&app_menu)
        .item(&edit_menu)
        .item(&window_menu)
        .build()?;
    app.set_menu(menu)?;
    Ok(())
}

fn main() {
    // Same credential source as the CLI: the launch directory and parents.
    dotenvy::dotenv().ok();
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .setup(|app| {
            app.manage(Arc::new(state::DesktopState::new(app.handle().clone())));
            build_menu(app)?;
            app.on_menu_event(|app, event| {
                if event.id() == "force-recover" {
                    force_recover_now(app);
                }
            });
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::setup_state,
            commands::model_catalog,
            commands::save_key,
            commands::save_model,
            commands::save_rig,
            commands::save_preset,
            commands::apply_preset,
            commands::delete_preset,
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
            commands::stop_background,
            commands::save_permission,
            commands::save_journal,
            commands::record_paths,
            commands::perf_report,
            commands::load_turns,
            commands::respond_interaction,
            commands::dismiss_interaction,
            commands::pending_interaction,
            commands::force_recover,
        ])
        .run(tauri::generate_context!())
        .expect("keelson desktop failed to start");
}
