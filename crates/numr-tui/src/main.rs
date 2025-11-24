//! numr TUI - Terminal User Interface for the numr calculator

mod app;
mod exchange;
mod ui;

use anyhow::Result;
use crossterm::{
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

fn main() -> Result<()> {
    // Setup terminal
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen, EnableMouseCapture)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    // Parse args
    let args = Args::parse();

    // Determine path
    let path = args.file.or_else(|| {
        ProjectDirs::from("com", "numr", "numr")
            .map(|proj_dirs| proj_dirs.config_dir().join("default.numr"))
    });

    // Create app and run
    let mut app = App::new(path);
    app.fetch_status = app::FetchStatus::Fetching;

    // Spawn currency fetcher
    let (tx, rx) = mpsc::channel();
    thread::spawn(move || {
        let rt = tokio::runtime::Runtime::new().unwrap();
        rt.block_on(async {
            match exchange::fetch_rates().await {
                Ok(rates) => {
                    let _ = tx.send(Ok(rates));
                }
                Err(e) => {
                    let _ = tx.send(Err(e.to_string()));
                }
            }
        });
    });

    let res = run_app(&mut terminal, &mut app, rx);

    // Restore terminal
    disable_raw_mode()?;
    execute!(
        terminal.backend_mut(),
        LeaveAlternateScreen,
        DisableMouseCapture
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
    rx: mpsc::Receiver<Result<std::collections::HashMap<String, f64>, String>>,
) -> Result<()> {
    loop {
        terminal.draw(|f| ui::draw(f, app))?;

        // Check for rate updates
        if let Ok(result) = rx.try_recv() {
            app.update_rates(result);
        }
        // Poll for events
        if event::poll(Duration::from_millis(100))? {
            match event::read()? {
                Event::Key(key) => match app.mode {
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
                            KeyCode::Char('q') => return Ok(()),
                            KeyCode::Char('s') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                                if let Err(e) = app.save() {
                                    eprintln!("Error saving: {e}");
                                }
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
                                // Insert line above (simplified: just new line for now or move up and new line)
                                // For now let's just do standard 'o' behavior
                                app.move_to_line_start();
                                // Logic for 'O' is a bit more complex with current primitives, skip for MVP
                            }
                            KeyCode::Char('h') | KeyCode::Left => app.move_left(),
                            KeyCode::Char('j') | KeyCode::Down => app.move_down(),
                            KeyCode::Char('k') | KeyCode::Up => app.move_up(),
                            KeyCode::Char('l') | KeyCode::Right => app.move_right(),
                            KeyCode::Char('x') => app.delete_char_forward(),
                            KeyCode::Char('d') => {
                                // Start pending delete (waiting for second 'd')
                                app.pending = PendingCommand::Delete;
                            }
                            KeyCode::Char('$') => app.move_to_line_end(),
                            KeyCode::Char('0') => app.move_to_line_start(),
                            KeyCode::Char('w') => app.toggle_wrap(),
                            KeyCode::F(12) => app.toggle_debug(),
                            _ => {}
                        }
                    }
                    InputMode::Insert => match key.code {
                        KeyCode::Esc => app.mode = InputMode::Normal,
                        KeyCode::Char('s') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                            if let Err(e) = app.save() {
                                eprintln!("Error saving: {e}");
                            }
                        }
                        KeyCode::Char(c) => app.insert_char(c),
                        KeyCode::Backspace => app.delete_char(),
                        KeyCode::Enter => app.new_line(),
                        KeyCode::Up => app.move_up(),
                        KeyCode::Down => app.move_down(),
                        KeyCode::Left => app.move_left(),
                        KeyCode::Right => app.move_right(),
                        KeyCode::Delete => app.delete_char_forward(),
                        KeyCode::F(12) => app.toggle_debug(),
                        _ => {}
                    },
                },
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
