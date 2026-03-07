//! Shared keyboard event handlers for both Standard and Vim modes
//!
//! These handlers extract common functionality to reduce code duplication
//! across different keybinding modes in the TUI.

use crate::app::App;
use crate::popups;
use std::sync::mpsc;

/// Result of handling a quit command
pub enum QuitResult {
    /// Exit the application
    Exit,
    /// Show quit confirmation dialog
    ShowConfirmation,
}

/// Handle save command (Ctrl+S)
/// Returns Ok(()) on success, Err with message on failure
pub fn handle_save(app: &mut App) -> Result<(), String> {
    app.save().map_err(|e| format!("Error: {e}"))
}

/// Handle quit command
/// Returns QuitResult indicating whether to exit, show confirmation, or continue
pub fn handle_quit(app: &App) -> QuitResult {
    if app.dirty {
        QuitResult::ShowConfirmation
    } else {
        QuitResult::Exit
    }
}

/// Handle quit confirmation dialog response
pub enum QuitConfirmResult {
    /// Save and exit
    SaveAndExit,
    /// Exit without saving
    ExitWithoutSave,
    /// Cancel and continue
    Cancel,
    /// Unhandled key
    Unhandled,
}

/// Process quit confirmation dialog key
pub fn handle_quit_confirmation(
    key_code: crossterm::event::KeyCode,
    app: &mut App,
) -> QuitConfirmResult {
    use crossterm::event::KeyCode;

    match key_code {
        KeyCode::Char('y') | KeyCode::Char('Y') => {
            // Save and quit
            if let Err(e) = app.save() {
                app.set_status(&format!("Error saving: {e}"));
                app.show_quit_confirmation = false;
                QuitConfirmResult::Cancel
            } else {
                QuitConfirmResult::SaveAndExit
            }
        }
        KeyCode::Char('n') | KeyCode::Char('N') => QuitConfirmResult::ExitWithoutSave,
        KeyCode::Esc | KeyCode::Char('q') => {
            app.show_quit_confirmation = false;
            QuitConfirmResult::Cancel
        }
        _ => QuitConfirmResult::Unhandled,
    }
}

/// Handle help navigation in Standard mode
/// Returns true if the key was handled
pub fn handle_help_standard(
    key_code: crossterm::event::KeyCode,
    app: &mut App,
    terminal_height: u16,
) -> bool {
    use crossterm::event::KeyCode;

    if !app.show_help {
        return false;
    }

    let max_scroll = popups::help_max_scroll(terminal_height, app.keybinding_mode);

    match key_code {
        KeyCode::Char('?') | KeyCode::Esc | KeyCode::F(1) => {
            app.toggle_help();
            true
        }
        KeyCode::Down => {
            app.help_scroll_down(max_scroll);
            true
        }
        KeyCode::Up => {
            app.help_scroll_up();
            true
        }
        _ => true, // Consume all keys when help is open
    }
}

/// Handle help navigation in Vim mode
/// Returns true if the key was handled
pub fn handle_help_vim(
    key_code: crossterm::event::KeyCode,
    app: &mut App,
    terminal_height: u16,
) -> bool {
    use crossterm::event::KeyCode;

    if !app.show_help {
        return false;
    }

    let max_scroll = popups::help_max_scroll(terminal_height, app.keybinding_mode);

    match key_code {
        KeyCode::Char('q') | KeyCode::Esc => {
            app.toggle_help();
            true
        }
        KeyCode::Char('j') | KeyCode::Down => {
            app.help_scroll_down(max_scroll);
            true
        }
        KeyCode::Char('k') | KeyCode::Up => {
            app.help_scroll_up();
            true
        }
        _ => true, // Consume all keys when help is open
    }
}

/// Spawn a background thread to fetch exchange rates
pub fn spawn_rate_fetch(
    app: &mut App,
) -> mpsc::Receiver<Result<std::collections::HashMap<String, f64>, String>> {
    use crate::app::FetchStatus;
    use std::thread;

    app.fetch_status = FetchStatus::Fetching;
    app.fetch_start = Some(std::time::Instant::now());
    let fetch_config = app.fetch_config();
    let (tx, rx) = mpsc::channel();
    thread::spawn(move || {
        let Ok(rt) = tokio::runtime::Runtime::new() else {
            let _ = tx.send(Err("Failed to create async runtime".to_string()));
            return;
        };
        rt.block_on(async {
            let result = numr_core::fetch_rates_with_config(&fetch_config).await;
            let _ = tx.send(result);
        });
    });
    rx
}

/// Handle keybinding mode toggle (Shift+Tab)
pub fn handle_keybinding_toggle(app: &mut App) {
    use crate::app::KeybindingMode;

    app.toggle_keybinding_mode();
    let mode_name = match app.keybinding_mode {
        KeybindingMode::Vim => "Vim",
        KeybindingMode::Standard => "Standard",
    };
    app.set_status(mode_name);
}
