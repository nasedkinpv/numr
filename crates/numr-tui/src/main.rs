//! numr TUI - Terminal User Interface for the numr calculator

mod app;
mod popups;
mod ui;

use anyhow::Result;
use crossterm::{
    cursor::SetCursorStyle,
    event::{self, DisableMouseCapture, EnableMouseCapture, Event, KeyCode, KeyModifiers},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use ratatui::{backend::CrosstermBackend, Terminal};
use std::io;
use std::sync::mpsc;
use std::thread;
use std::time::Duration;

use app::{App, InputMode, PendingCommand};
use clap::Parser;
use directories::ProjectDirs;
use std::path::PathBuf;

#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
struct Args {
    /// Path to the numr file to open
    #[arg(value_name = "FILE")]
    file: Option<PathBuf>,
}

/// Spawn a background thread to fetch exchange rates
fn spawn_rate_fetch(
    app: &mut App,
) -> mpsc::Receiver<Result<std::collections::HashMap<String, f64>, String>> {
    app.fetch_status = app::FetchStatus::Fetching;
    app.fetch_start = Some(std::time::Instant::now());
    let (tx, rx) = mpsc::channel();
    thread::spawn(move || {
        let Ok(rt) = tokio::runtime::Runtime::new() else {
            let _ = tx.send(Err("Failed to create async runtime".to_string()));
            return;
        };
        rt.block_on(async {
            let result = numr_core::fetch_rates().await;
            let _ = tx.send(result);
        });
    });
    rx
}

fn main() -> Result<()> {
    // Parse args first - handles --help/--version before terminal setup
    let args = Args::parse();

    // Determine path
    let path = args.file.or_else(|| {
        ProjectDirs::from("com", "numr", "numr")
            .map(|proj_dirs| proj_dirs.config_dir().join("default.numr"))
    });

    // Setup terminal (only after arg parsing to avoid breaking terminal on --help)
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(
        stdout,
        EnterAlternateScreen,
        EnableMouseCapture,
        SetCursorStyle::DefaultUserShape
    )?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    // Create app and run
    let mut app = App::new(path);

    // Initial rate fetch
    let rx = spawn_rate_fetch(&mut app);

    let res = run_app(&mut terminal, &mut app, rx);

    // Restore terminal
    disable_raw_mode()?;
    execute!(
        terminal.backend_mut(),
        LeaveAlternateScreen,
        DisableMouseCapture,
        SetCursorStyle::DefaultUserShape
    )?;
    terminal.show_cursor()?;

    if let Err(err) = res {
        eprintln!("Error: {err:?}");
    }

    Ok(())
}

fn run_app<B: ratatui::backend::Backend>(
    terminal: &mut Terminal<B>,
    app: &mut App,
    initial_rx: mpsc::Receiver<Result<std::collections::HashMap<String, f64>, String>>,
) -> Result<()> {
    let mut stdout = io::stdout();
    let mut rate_rx = Some(initial_rx);

    loop {
        terminal.draw(|f| ui::draw(f, app))?;

        // Update cursor style based on mode
        match app.mode {
            InputMode::Normal => execute!(stdout, SetCursorStyle::DefaultUserShape)?,
            InputMode::Insert => execute!(stdout, SetCursorStyle::BlinkingBar)?,
        }

        // Check for rate updates
        if let Some(ref rx) = rate_rx {
            if let Ok(result) = rx.try_recv() {
                app.update_rates(result);
            }
        }
        // Poll for events
        if event::poll(Duration::from_millis(100))? {
            match event::read()? {
                Event::Key(key) => {
                    // Handle quit confirmation popup
                    if app.show_quit_confirmation {
                        match key.code {
                            KeyCode::Char('y') | KeyCode::Char('Y') => {
                                // Save and quit
                                if let Err(e) = app.save() {
                                    app.set_status(&format!("Error saving: {e}"));
                                    app.show_quit_confirmation = false;
                                } else {
                                    return Ok(());
                                }
                            }
                            KeyCode::Char('n') | KeyCode::Char('N') => {
                                // Quit without saving
                                return Ok(());
                            }
                            KeyCode::Esc | KeyCode::Char('q') => {
                                // Cancel
                                app.show_quit_confirmation = false;
                            }
                            _ => {}
                        }
                        continue;
                    }

                    match app.mode {
                        InputMode::Normal => {
                            // Handle pending commands first
                            if app.pending == PendingCommand::Delete {
                                if key.code == KeyCode::Char('d') {
                                    app.delete_line();
                                }
                                app.pending = PendingCommand::None;
                                continue;
                            }

                            match key.code {
                                KeyCode::Char('?') | KeyCode::F(1) => app.toggle_help(),
                                KeyCode::Esc => {
                                    if app.show_help {
                                        app.toggle_help();
                                    }
                                }
                                _ if app.show_help => {
                                    // Handle help popup navigation
                                    let max_scroll =
                                        popups::help_max_scroll(terminal.size()?.height);
                                    match key.code {
                                        KeyCode::Char('q') => app.toggle_help(),
                                        KeyCode::Char('j') | KeyCode::Down => {
                                            app.help_scroll_down(max_scroll)
                                        }
                                        KeyCode::Char('k') | KeyCode::Up => app.help_scroll_up(),
                                        _ => {}
                                    }
                                }
                                KeyCode::Char('q') => {
                                    if app.dirty {
                                        app.show_quit_confirmation = true;
                                    } else {
                                        return Ok(());
                                    }
                                }
                                KeyCode::Char('s')
                                    if key.modifiers.contains(KeyModifiers::CONTROL) =>
                                {
                                    if let Err(e) = app.save() {
                                        app.set_status(&format!("Error: {e}"));
                                    } else {
                                        app.set_status("Saved");
                                    }
                                }
                                KeyCode::Char('r')
                                    if key.modifiers.contains(KeyModifiers::CONTROL) =>
                                {
                                    // Refresh exchange rates
                                    rate_rx = Some(spawn_rate_fetch(app));
                                }
                                KeyCode::Char('i') => app.mode = InputMode::Insert,
                                KeyCode::Char('a') => {
                                    app.move_right();
                                    app.mode = InputMode::Insert;
                                }
                                KeyCode::Char('A') => {
                                    app.move_to_line_end();
                                    app.mode = InputMode::Insert;
                                }
                                KeyCode::Char('o') => {
                                    app.move_to_line_end();
                                    app.new_line();
                                    app.mode = InputMode::Insert;
                                }
                                KeyCode::Char('O') => {
                                    app.move_to_line_start();
                                    // Logic for 'O' is a bit more complex with current primitives, skip for MVP
                                }
                                KeyCode::Char('h') | KeyCode::Left => app.move_left(),
                                KeyCode::Char('j') | KeyCode::Down => app.move_down(),
                                KeyCode::Char('k') | KeyCode::Up => app.move_up(),
                                KeyCode::Char('l') | KeyCode::Right => app.move_right(),
                                KeyCode::PageUp => app.page_up(),
                                KeyCode::PageDown => app.page_down(),
                                KeyCode::Home => app.move_to_line_start(),
                                KeyCode::End => app.move_to_line_end(),
                                KeyCode::Char('x') => app.delete_char_forward(),
                                KeyCode::Char('d') => {
                                    // Start pending delete (waiting for second 'd')
                                    app.pending = PendingCommand::Delete;
                                }
                                KeyCode::Char('$') => app.move_to_line_end(),
                                KeyCode::Char('0') => app.move_to_line_start(),
                                KeyCode::Char('W') => app.toggle_wrap(),
                                KeyCode::Char('N') => app.toggle_line_numbers(),
                                KeyCode::Char('H') => app.toggle_header(),
                                KeyCode::F(12) => app.toggle_debug(),
                                _ => {}
                            }
                        }
                        InputMode::Insert => match key.code {
                            KeyCode::Esc => app.mode = InputMode::Normal,
                            KeyCode::Char('s') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                                if let Err(e) = app.save() {
                                    app.set_status(&format!("Error: {e}"));
                                } else {
                                    app.set_status("Saved");
                                }
                            }
                            KeyCode::Char('r') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                                rate_rx = Some(spawn_rate_fetch(app));
                            }
                            KeyCode::Char(c) => app.insert_char(c),
                            KeyCode::Backspace => app.delete_char(),
                            KeyCode::Enter => app.new_line(),
                            KeyCode::Up => app.move_up(),
                            KeyCode::Down => app.move_down(),
                            KeyCode::Left => app.move_left(),
                            KeyCode::Right => app.move_right(),
                            KeyCode::PageUp => app.page_up(),
                            KeyCode::PageDown => app.page_down(),
                            KeyCode::Home => app.move_to_line_start(),
                            KeyCode::End => app.move_to_line_end(),
                            KeyCode::Delete => app.delete_char_forward(),
                            KeyCode::F(12) => app.toggle_debug(),
                            _ => {}
                        },
                    }
                }
                Event::Mouse(mouse) => match mouse.kind {
                    event::MouseEventKind::ScrollDown => app.move_down(),
                    event::MouseEventKind::ScrollUp => app.move_up(),
                    _ => {}
                },
                _ => {}
            }
        }
    }
}
