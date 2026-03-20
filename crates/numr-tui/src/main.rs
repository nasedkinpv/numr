//! numr TUI - Terminal User Interface for the numr calculator

mod app;
mod config;
mod handlers;
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
use std::time::Duration;

use app::{App, InputMode, KeybindingMode, PendingCommand};
use clap::Parser;
use directories::ProjectDirs;
use handlers::{
    handle_help_standard, handle_help_vim, handle_keybinding_toggle, handle_quit,
    handle_quit_confirmation, handle_save, spawn_rate_fetch, QuitConfirmResult, QuitResult,
};
use ratatui::layout::Rect;
use std::path::PathBuf;

#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
struct Args {
    /// Path to the numr file to open
    #[arg(value_name = "FILE")]
    file: Option<PathBuf>,
}

/// Expand ~ to home directory (cross-platform)
fn expand_tilde(path: &str) -> PathBuf {
    if let Some(stripped) = path.strip_prefix("~/") {
        if let Some(base) = directories::BaseDirs::new() {
            return base.home_dir().join(stripped);
        }
    }
    PathBuf::from(path)
}

fn main() -> Result<()> {
    // Parse args first - handles --help/--version before terminal setup
    let args = Args::parse();

    // Load config
    let (config, config_warning) = config::Config::load();

    // Determine path: CLI arg > config.files.default_path > default location
    let path = args.file.or_else(|| {
        config
            .files
            .default_path
            .as_ref()
            .map(|s| expand_tilde(s))
            .or_else(|| {
                ProjectDirs::from("", "", "numr")
                    .map(|proj_dirs| proj_dirs.config_dir().join("default.numr"))
            })
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
    let mut app = App::new(path, config);

    // Show config warning if any
    if let Some(warning) = config_warning {
        app.set_status(&warning);
    }

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
    initial_rx: mpsc::Receiver<Result<numr_core::FetchResult, String>>,
) -> Result<()> {
    let mut stdout = io::stdout();
    let mut rate_rx = Some(initial_rx);

    loop {
        // Clear expired status messages before drawing
        app.clear_status_if_expired();

        let terminal_size = terminal.size()?;
        let terminal_area = Rect::new(0, 0, terminal_size.width, terminal_size.height);
        let (viewport_width, viewport_height) = ui::viewport_dimensions(app, terminal_area);
        app.set_viewport_size(viewport_width, viewport_height);

        terminal.draw(|f| ui::draw(f, app))?;

        // Update cursor style based on mode
        update_cursor_style(&mut stdout, app)?;

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
                    // Handle quit confirmation popup first
                    if app.show_quit_confirmation {
                        match handle_quit_confirmation(key.code, app) {
                            QuitConfirmResult::SaveAndExit | QuitConfirmResult::ExitWithoutSave => {
                                return Ok(())
                            }
                            QuitConfirmResult::Cancel | QuitConfirmResult::Unhandled => continue,
                        }
                    }

                    // Handle keybinding mode toggle (Shift+Tab works in both modes)
                    if key.code == KeyCode::BackTab {
                        handle_keybinding_toggle(app);
                        continue;
                    }

                    // Route to mode-specific handler
                    let result = match app.keybinding_mode {
                        KeybindingMode::Standard => {
                            handle_standard_mode(key, app, terminal, &mut rate_rx)?
                        }
                        KeybindingMode::Vim => handle_vim_mode(key, app, terminal, &mut rate_rx)?,
                    };

                    if result == ControlFlow::Exit {
                        return Ok(());
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

/// Control flow result from key handlers
#[derive(PartialEq, Eq)]
enum ControlFlow {
    Continue,
    Exit,
}

/// Update cursor style based on current mode
fn update_cursor_style(stdout: &mut io::Stdout, app: &App) -> Result<()> {
    match (app.keybinding_mode, app.mode) {
        (KeybindingMode::Standard, _) => execute!(stdout, SetCursorStyle::BlinkingBar)?,
        (KeybindingMode::Vim, InputMode::Normal) => {
            execute!(stdout, SetCursorStyle::DefaultUserShape)?
        }
        (KeybindingMode::Vim, InputMode::Insert) => execute!(stdout, SetCursorStyle::BlinkingBar)?,
    }
    Ok(())
}

/// Handle Standard mode keys (direct input like traditional editors)
fn handle_standard_mode<B: ratatui::backend::Backend>(
    key: crossterm::event::KeyEvent,
    app: &mut App,
    terminal: &Terminal<B>,
    rate_rx: &mut Option<mpsc::Receiver<Result<numr_core::FetchResult, String>>>,
) -> Result<ControlFlow> {
    let ctrl = key.modifiers.contains(KeyModifiers::CONTROL);

    // Help popup handling
    if handle_help_standard(key.code, app, terminal.size()?.height) {
        return Ok(ControlFlow::Continue);
    }

    match key.code {
        KeyCode::Char('q') if ctrl => match handle_quit(app) {
            QuitResult::Exit => return Ok(ControlFlow::Exit),
            QuitResult::ShowConfirmation => app.show_quit_confirmation = true,
        },
        KeyCode::Char('s') if ctrl => {
            if let Err(e) = handle_save(app) {
                app.set_status(&e);
            } else {
                app.set_status(app::STATUS_SAVED);
            }
        }
        KeyCode::Char('r') if ctrl => {
            *rate_rx = Some(spawn_rate_fetch(app));
        }
        KeyCode::Char('k') if ctrl => app.delete_line(),
        KeyCode::Char('a') if ctrl => app.move_to_line_start(),
        KeyCode::Char('e') if ctrl => app.move_to_line_end(),
        KeyCode::Char('g') if ctrl => app.move_to_first_line(),
        KeyCode::Char('l') if ctrl => app.toggle_line_numbers(),
        KeyCode::Char('w') if ctrl => app.toggle_wrap(),
        KeyCode::Char('h') if ctrl => app.toggle_header(),
        KeyCode::Char('?') | KeyCode::F(1) => app.toggle_help(),
        KeyCode::F(12) => app.toggle_debug(),
        KeyCode::Char(c) => app.insert_char(c),
        KeyCode::Backspace => app.delete_char(),
        KeyCode::Delete => app.delete_char_forward(),
        KeyCode::Enter => app.new_line(),
        KeyCode::Up => app.move_up(),
        KeyCode::Down => app.move_down(),
        KeyCode::Left => app.move_left(),
        KeyCode::Right => app.move_right(),
        KeyCode::Home => app.move_to_line_start(),
        KeyCode::End => app.move_to_line_end(),
        KeyCode::PageUp => app.page_up(),
        KeyCode::PageDown => app.page_down(),
        _ => {}
    }

    Ok(ControlFlow::Continue)
}

/// Handle Vim mode keys (modal editing)
fn handle_vim_mode<B: ratatui::backend::Backend>(
    key: crossterm::event::KeyEvent,
    app: &mut App,
    terminal: &Terminal<B>,
    rate_rx: &mut Option<mpsc::Receiver<Result<numr_core::FetchResult, String>>>,
) -> Result<ControlFlow> {
    match app.mode {
        InputMode::Normal => handle_vim_normal_mode(key, app, terminal, rate_rx),
        InputMode::Insert => handle_vim_insert_mode(key, app, rate_rx),
    }
}

/// Handle Vim Normal mode keys
fn handle_vim_normal_mode<B: ratatui::backend::Backend>(
    key: crossterm::event::KeyEvent,
    app: &mut App,
    terminal: &Terminal<B>,
    rate_rx: &mut Option<mpsc::Receiver<Result<numr_core::FetchResult, String>>>,
) -> Result<ControlFlow> {
    // Handle pending commands first
    match app.pending {
        PendingCommand::Delete => {
            if key.code == KeyCode::Char('d') {
                app.delete_line();
            }
            app.pending = PendingCommand::None;
            return Ok(ControlFlow::Continue);
        }
        PendingCommand::Go => {
            if key.code == KeyCode::Char('g') {
                app.move_to_first_line();
            }
            app.pending = PendingCommand::None;
            return Ok(ControlFlow::Continue);
        }
        PendingCommand::None => {}
    }

    // Toggle help or close it with Esc
    match key.code {
        KeyCode::Char('?') | KeyCode::F(1) => {
            app.toggle_help();
            return Ok(ControlFlow::Continue);
        }
        KeyCode::Esc if app.show_help => {
            app.toggle_help();
            return Ok(ControlFlow::Continue);
        }
        _ => {}
    }

    // Handle help navigation if open
    if handle_help_vim(key.code, app, terminal.size()?.height) {
        return Ok(ControlFlow::Continue);
    }

    let ctrl = key.modifiers.contains(KeyModifiers::CONTROL);

    match key.code {
        KeyCode::Char('q') => match handle_quit(app) {
            QuitResult::Exit => return Ok(ControlFlow::Exit),
            QuitResult::ShowConfirmation => app.show_quit_confirmation = true,
        },
        KeyCode::Char('s') if ctrl => {
            if let Err(e) = handle_save(app) {
                app.set_status(&e);
            } else {
                app.set_status(app::STATUS_SAVED);
            }
        }
        KeyCode::Char('r') if ctrl => {
            *rate_rx = Some(spawn_rate_fetch(app));
        }
        // Enter insert mode
        KeyCode::Char('i') => app.mode = InputMode::Insert,
        KeyCode::Char('a') => {
            app.move_right();
            app.mode = InputMode::Insert;
        }
        KeyCode::Char('A') => {
            app.move_to_line_end();
            app.mode = InputMode::Insert;
        }
        KeyCode::Char('I') => {
            app.move_to_line_start();
            app.mode = InputMode::Insert;
        }
        KeyCode::Char('o') => {
            app.move_to_line_end();
            app.new_line();
            app.mode = InputMode::Insert;
        }
        KeyCode::Char('O') => {
            app.move_to_line_start();
            app.new_line();
            app.move_up();
            app.mode = InputMode::Insert;
        }
        KeyCode::Char('C') => {
            app.delete_to_line_end();
            app.mode = InputMode::Insert;
        }
        KeyCode::Char('s') => {
            app.delete_char_forward();
            app.mode = InputMode::Insert;
        }
        // Movement
        KeyCode::Char(' ') => app.move_right(),
        KeyCode::Char('h') | KeyCode::Left => app.move_left(),
        KeyCode::Char('j') | KeyCode::Down => app.move_down(),
        KeyCode::Char('k') | KeyCode::Up => app.move_up(),
        KeyCode::Char('l') | KeyCode::Right => app.move_right(),
        KeyCode::Char('w') => app.move_word_forward(),
        KeyCode::Char('b') => app.move_word_backward(),
        KeyCode::Char('e') => app.move_word_end(),
        KeyCode::Char('G') => app.move_to_last_line(),
        KeyCode::Char('g') => app.pending = PendingCommand::Go,
        KeyCode::PageUp => app.page_up(),
        KeyCode::PageDown => app.page_down(),
        KeyCode::Home | KeyCode::Char('0') => app.move_to_line_start(),
        KeyCode::End | KeyCode::Char('$') => app.move_to_line_end(),
        KeyCode::Char('^') => app.move_to_line_start(),
        // Editing
        KeyCode::Char('x') => app.delete_char_forward(),
        KeyCode::Char('X') => app.delete_char(),
        KeyCode::Char('d') => app.pending = PendingCommand::Delete,
        KeyCode::Char('D') => app.delete_to_line_end(),
        KeyCode::Char('J') => app.join_with_next_line(),
        // Toggles
        KeyCode::Char('W') => app.toggle_wrap(),
        KeyCode::Char('N') => app.toggle_line_numbers(),
        KeyCode::Char('H') => app.toggle_header(),
        KeyCode::F(12) => app.toggle_debug(),
        _ => {}
    }

    Ok(ControlFlow::Continue)
}

/// Handle Vim Insert mode keys
fn handle_vim_insert_mode(
    key: crossterm::event::KeyEvent,
    app: &mut App,
    rate_rx: &mut Option<mpsc::Receiver<Result<numr_core::FetchResult, String>>>,
) -> Result<ControlFlow> {
    let ctrl = key.modifiers.contains(KeyModifiers::CONTROL);

    match key.code {
        KeyCode::Esc => app.mode = InputMode::Normal,
        KeyCode::Char('s') if ctrl => {
            if let Err(e) = handle_save(app) {
                app.set_status(&e);
            } else {
                app.set_status(app::STATUS_SAVED);
            }
        }
        KeyCode::Char('r') if ctrl => {
            *rate_rx = Some(spawn_rate_fetch(app));
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
    }

    Ok(ControlFlow::Continue)
}
