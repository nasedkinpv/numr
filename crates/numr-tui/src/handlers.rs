//! Shared keyboard event handlers for both Standard and Vim modes
//!
//! These handlers extract common functionality to reduce code duplication
//! across different keybinding modes in the TUI.

use crate::app::App;
use crate::popups;
use numr_core::{FetchConfig, FetchResult};
use std::sync::mpsc::{self, Receiver, SyncSender, TryRecvError, TrySendError};

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
    app.save().map_err(|error| format!("Save failed: {error}"))
}

/// Handle quit command
/// Returns QuitResult indicating whether to exit, show confirmation, or continue
pub fn handle_quit(app: &App) -> QuitResult {
    if app.is_dirty() {
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
            if let Err(error) = handle_save(app) {
                app.set_status(&error);
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

/// Open, close, or navigate the mode-aware help popup.
/// Returns true when the key was consumed by help.
pub fn handle_help(
    key_code: crossterm::event::KeyCode,
    app: &mut App,
    terminal_height: u16,
) -> bool {
    use crate::app::KeybindingMode;
    use crossterm::event::KeyCode;

    if !app.show_help {
        return match key_code {
            KeyCode::Char('?') | KeyCode::F(1) => {
                app.toggle_help();
                true
            }
            _ => false,
        };
    }

    let max_scroll = popups::help_max_scroll(terminal_height, app.keybinding_mode);
    let vim = app.keybinding_mode == KeybindingMode::Vim;

    match key_code {
        KeyCode::Char('?') | KeyCode::F(1) | KeyCode::Esc => {
            app.toggle_help();
            true
        }
        KeyCode::Char('q') if vim => {
            app.toggle_help();
            true
        }
        KeyCode::Char('j') if vim => {
            app.help_scroll_down(max_scroll);
            true
        }
        KeyCode::Char('k') if vim => {
            app.help_scroll_up();
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

/// A single long-lived exchange-rate worker.
///
/// Repeated refresh keys while a request is active are coalesced by `request`,
/// so the TUI never accumulates runtimes or overlapping HTTP requests.
pub struct RateFetcher {
    request_tx: SyncSender<FetchConfig>,
    result_rx: Receiver<Result<FetchResult, String>>,
}

impl RateFetcher {
    pub fn new() -> Self {
        let (request_tx, request_rx) = mpsc::sync_channel::<FetchConfig>(1);
        let (result_tx, result_rx) = mpsc::channel();

        let _ = std::thread::Builder::new()
            .name("numr-rates".to_string())
            .spawn(move || {
                let runtime = match tokio::runtime::Builder::new_current_thread()
                    .enable_all()
                    .build()
                {
                    Ok(runtime) => runtime,
                    Err(error) => {
                        let _ =
                            result_tx.send(Err(format!("Failed to create async runtime: {error}")));
                        return;
                    }
                };

                while let Ok(fetch_config) = request_rx.recv() {
                    let result =
                        runtime.block_on(numr_core::fetch_rates_with_config(&fetch_config));
                    let result = result.map_err(|error| error.to_string());
                    if result_tx.send(result).is_err() {
                        break;
                    }
                }
            });

        Self {
            request_tx,
            result_rx,
        }
    }

    pub fn request(&self, app: &mut App) {
        use crate::app::FetchStatus;

        if app.fetch_status == FetchStatus::Fetching {
            return;
        }

        match self.request_tx.try_send(app.fetch_config()) {
            Ok(()) | Err(TrySendError::Full(_)) => {
                app.fetch_status = FetchStatus::Fetching;
                app.fetch_start = Some(std::time::Instant::now());
            }
            Err(TrySendError::Disconnected(_)) => {
                app.update_rates(Err("Rate service is unavailable".to_string()));
            }
        }
    }

    pub fn try_complete(&self, app: &mut App) -> bool {
        match self.result_rx.try_recv() {
            Ok(result) => {
                app.update_rates(result);
                true
            }
            Err(TryRecvError::Empty) => false,
            Err(TryRecvError::Disconnected) => {
                if app.fetch_status == crate::app::FetchStatus::Fetching {
                    app.update_rates(Err("Rate service stopped unexpectedly".to_string()));
                    true
                } else {
                    false
                }
            }
        }
    }
}

impl Default for RateFetcher {
    fn default() -> Self {
        Self::new()
    }
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn repeated_rate_requests_are_coalesced_while_fetching() {
        let (request_tx, request_rx) = mpsc::sync_channel(1);
        let (_result_tx, result_rx) = mpsc::channel();
        let fetcher = RateFetcher {
            request_tx,
            result_rx,
        };
        let mut app = App::default();

        fetcher.request(&mut app);
        assert_eq!(app.fetch_status, crate::app::FetchStatus::Fetching);
        assert!(request_rx.try_recv().is_ok());

        fetcher.request(&mut app);
        assert!(matches!(request_rx.try_recv(), Err(TryRecvError::Empty)));
    }

    #[test]
    fn quit_flow_preserves_unsaved_documents_until_the_user_decides() {
        let mut app = App::default();
        assert!(matches!(handle_quit(&app), QuitResult::Exit));

        app.insert_char('1');
        assert!(matches!(handle_quit(&app), QuitResult::ShowConfirmation));

        app.show_quit_confirmation = true;
        assert!(matches!(
            handle_quit_confirmation(crossterm::event::KeyCode::Esc, &mut app),
            QuitConfirmResult::Cancel
        ));
        assert!(!app.show_quit_confirmation);
        assert!(app.is_dirty());

        app.show_quit_confirmation = true;
        assert!(matches!(
            handle_quit_confirmation(crossterm::event::KeyCode::Char('y'), &mut app),
            QuitConfirmResult::Cancel
        ));
        assert!(app.is_dirty());
        assert!(app
            .status_message
            .as_deref()
            .is_some_and(|message| message.starts_with("Save failed:")));

        assert!(matches!(
            handle_quit_confirmation(crossterm::event::KeyCode::Char('n'), &mut app),
            QuitConfirmResult::ExitWithoutSave
        ));
    }

    #[test]
    fn help_keys_are_mode_aware() {
        use crate::app::KeybindingMode;
        use crossterm::event::KeyCode;

        for mode in [KeybindingMode::Standard, KeybindingMode::Vim] {
            let mut app = App::default();
            app.keybinding_mode = mode;
            assert!(handle_help(KeyCode::F(1), &mut app, 24));
            assert!(app.show_help);
            assert!(handle_help(KeyCode::Down, &mut app, 24));
            assert_eq!(app.help_scroll, 1);

            let close_key = if mode == KeybindingMode::Vim {
                KeyCode::Char('q')
            } else {
                KeyCode::Esc
            };
            assert!(handle_help(close_key, &mut app, 24));
            assert!(!app.show_help);
        }
    }
}
