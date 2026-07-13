//! numr TUI - Terminal User Interface for the numr calculator

mod app;
mod config;
mod handlers;
mod line_layout;
mod persistence;
mod popups;
mod theme;
mod ui;

use anyhow::Result;
use crossterm::{
    cursor::SetCursorStyle,
    event::{
        self, DisableMouseCapture, EnableMouseCapture, Event, KeyCode, KeyModifiers,
        KeyboardEnhancementFlags, PopKeyboardEnhancementFlags, PushKeyboardEnhancementFlags,
    },
    execute,
    terminal::{
        disable_raw_mode, enable_raw_mode, supports_keyboard_enhancement, EnterAlternateScreen,
        LeaveAlternateScreen,
    },
};
use ratatui::{backend::CrosstermBackend, Terminal};
use std::io;
use std::time::Duration;

use app::{App, InputMode, KeybindingMode, PendingCommand};
use clap::Parser;
use directories::ProjectDirs;
use handlers::{
    handle_help, handle_keybinding_toggle, handle_quit, handle_quit_confirmation, handle_save,
    QuitConfirmResult, QuitResult, RateFetcher,
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

type CrosstermTerminal = Terminal<CrosstermBackend<io::Stdout>>;

struct TerminalGuard {
    terminal: CrosstermTerminal,
    active: bool,
    keyboard_enhancement: bool,
}

impl TerminalGuard {
    fn enter() -> Result<Self> {
        enable_raw_mode()?;
        let mut stdout = io::stdout();
        if let Err(error) = execute!(
            stdout,
            EnterAlternateScreen,
            EnableMouseCapture,
            SetCursorStyle::DefaultUserShape
        ) {
            let _ = disable_raw_mode();
            let _ = execute!(
                io::stdout(),
                LeaveAlternateScreen,
                DisableMouseCapture,
                SetCursorStyle::DefaultUserShape
            );
            return Err(error.into());
        }

        let keyboard_enhancement = matches!(supports_keyboard_enhancement(), Ok(true));
        if keyboard_enhancement {
            if let Err(error) = execute!(
                stdout,
                PushKeyboardEnhancementFlags(KeyboardEnhancementFlags::DISAMBIGUATE_ESCAPE_CODES)
            ) {
                let _ = disable_raw_mode();
                let _ = execute!(
                    io::stdout(),
                    LeaveAlternateScreen,
                    DisableMouseCapture,
                    SetCursorStyle::DefaultUserShape
                );
                return Err(error.into());
            }
        }

        match Terminal::new(CrosstermBackend::new(stdout)) {
            Ok(terminal) => Ok(Self {
                terminal,
                active: true,
                keyboard_enhancement,
            }),
            Err(error) => {
                if keyboard_enhancement {
                    let _ = execute!(io::stdout(), PopKeyboardEnhancementFlags);
                }
                let _ = disable_raw_mode();
                let _ = execute!(
                    io::stdout(),
                    LeaveAlternateScreen,
                    DisableMouseCapture,
                    SetCursorStyle::DefaultUserShape
                );
                Err(error.into())
            }
        }
    }

    fn terminal_mut(&mut self) -> &mut CrosstermTerminal {
        &mut self.terminal
    }

    fn restore(&mut self) -> Result<()> {
        if !self.active {
            return Ok(());
        }
        self.active = false;

        let mut first_error = None;
        if self.keyboard_enhancement {
            remember_first_error(
                execute!(self.terminal.backend_mut(), PopKeyboardEnhancementFlags),
                &mut first_error,
            );
        }
        remember_first_error(disable_raw_mode(), &mut first_error);
        remember_first_error(
            execute!(
                self.terminal.backend_mut(),
                LeaveAlternateScreen,
                DisableMouseCapture,
                SetCursorStyle::DefaultUserShape
            ),
            &mut first_error,
        );
        remember_first_error(self.terminal.show_cursor(), &mut first_error);

        first_error.map_or(Ok(()), |error| Err(error.into()))
    }
}

impl Drop for TerminalGuard {
    fn drop(&mut self) {
        let _ = self.restore();
    }
}

fn remember_first_error(result: io::Result<()>, first_error: &mut Option<io::Error>) {
    if let Err(error) = result {
        if first_error.is_none() {
            *first_error = Some(error);
        }
    }
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

    // Setup terminal only after arg parsing to keep --help/--version ordinary.
    let mut terminal = TerminalGuard::enter()?;

    // Create app and run
    let mut app = App::new(path, config);

    // Show config warning if any
    if let Some(warning) = config_warning {
        app.set_status(&warning);
    }

    let rate_fetcher = RateFetcher::new();
    rate_fetcher.request(&mut app);

    let run_result = run_app(terminal.terminal_mut(), &mut app, &rate_fetcher);
    let restore_result = terminal.restore();
    run_result?;
    restore_result
}

fn run_app<B: ratatui::backend::Backend>(
    terminal: &mut Terminal<B>,
    app: &mut App,
    rate_fetcher: &RateFetcher,
) -> Result<()>
where
    B::Error: Send + Sync + 'static,
{
    let mut stdout = io::stdout();
    let mut needs_redraw = true;

    loop {
        needs_redraw |= app.clear_status_if_expired();
        needs_redraw |= rate_fetcher.try_complete(app);

        if needs_redraw {
            let terminal_size = terminal.size()?;
            let terminal_area = Rect::new(0, 0, terminal_size.width, terminal_size.height);
            let (viewport_width, viewport_height) = ui::viewport_dimensions(app, terminal_area);
            app.set_viewport_size(viewport_width, viewport_height);

            terminal.draw(|frame| ui::draw(frame, app))?;
            update_cursor_style(&mut stdout, app)?;
            needs_redraw = false;
        }

        let poll_timeout = app
            .animation_interval()
            .unwrap_or(Duration::from_secs(60 * 60));
        if event::poll(poll_timeout)? {
            needs_redraw = true;
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
                            handle_standard_mode(key, app, terminal, rate_fetcher)?
                        }
                        KeybindingMode::Vim => handle_vim_mode(key, app, terminal, rate_fetcher)?,
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
        } else if app.has_active_animation() {
            needs_redraw = true;
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

fn save_and_report(app: &mut App) {
    match handle_save(app) {
        Ok(()) => app.set_status(app::STATUS_SAVED),
        Err(error) => app.set_status(&error),
    }
}

fn accepts_text_input(modifiers: KeyModifiers) -> bool {
    modifiers.difference(KeyModifiers::SHIFT).is_empty()
}

/// Handle Standard mode keys (direct input like traditional editors)
fn handle_standard_mode<B: ratatui::backend::Backend>(
    key: crossterm::event::KeyEvent,
    app: &mut App,
    terminal: &Terminal<B>,
    rate_fetcher: &RateFetcher,
) -> Result<ControlFlow>
where
    B::Error: Send + Sync + 'static,
{
    let ctrl = key.modifiers.contains(KeyModifiers::CONTROL);
    let alt = key.modifiers.contains(KeyModifiers::ALT);
    let super_key = key.modifiers.contains(KeyModifiers::SUPER);

    // Help popup handling
    if handle_help(key.code, app, terminal.size()?.height) {
        return Ok(ControlFlow::Continue);
    }

    match key.code {
        KeyCode::Char('q') if ctrl => match handle_quit(app) {
            QuitResult::Exit => return Ok(ControlFlow::Exit),
            QuitResult::ShowConfirmation => app.show_quit_confirmation = true,
        },
        KeyCode::Char('s') if ctrl => {
            save_and_report(app);
        }
        KeyCode::Char('r') if ctrl => {
            rate_fetcher.request(app);
        }
        KeyCode::Char('k') if ctrl => app.delete_to_line_end(),
        KeyCode::Char('u') if ctrl => app.delete_to_line_start(),
        KeyCode::Char('w') if ctrl => app.delete_word_backward(),
        KeyCode::Backspace if alt => app.delete_word_backward(),
        KeyCode::Backspace if super_key => app.delete_to_line_start(),
        KeyCode::Char('a') if ctrl => app.move_to_line_start(),
        KeyCode::Char('e') if ctrl => app.move_to_line_end(),
        KeyCode::Char('g') if ctrl => app.move_to_first_line(),
        KeyCode::Char('l') if ctrl => app.toggle_line_numbers(),
        KeyCode::Char('z') if alt => app.toggle_wrap(),
        KeyCode::Char('h') if ctrl => app.toggle_header(),
        KeyCode::F(12) => app.toggle_debug(),
        KeyCode::Char(c) if accepts_text_input(key.modifiers) => app.insert_char(c),
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
    rate_fetcher: &RateFetcher,
) -> Result<ControlFlow>
where
    B::Error: Send + Sync + 'static,
{
    match app.mode {
        InputMode::Normal => handle_vim_normal_mode(key, app, terminal, rate_fetcher),
        InputMode::Insert => handle_vim_insert_mode(key, app, rate_fetcher),
    }
}

/// Handle Vim Normal mode keys
fn handle_vim_normal_mode<B: ratatui::backend::Backend>(
    key: crossterm::event::KeyEvent,
    app: &mut App,
    terminal: &Terminal<B>,
    rate_fetcher: &RateFetcher,
) -> Result<ControlFlow>
where
    B::Error: Send + Sync + 'static,
{
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

    if handle_help(key.code, app, terminal.size()?.height) {
        return Ok(ControlFlow::Continue);
    }

    let ctrl = key.modifiers.contains(KeyModifiers::CONTROL);

    match key.code {
        KeyCode::Char('q') => match handle_quit(app) {
            QuitResult::Exit => return Ok(ControlFlow::Exit),
            QuitResult::ShowConfirmation => app.show_quit_confirmation = true,
        },
        KeyCode::Char('s') if ctrl => {
            save_and_report(app);
        }
        KeyCode::Char('r') if ctrl => {
            rate_fetcher.request(app);
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
    rate_fetcher: &RateFetcher,
) -> Result<ControlFlow> {
    let ctrl = key.modifiers.contains(KeyModifiers::CONTROL);
    let alt = key.modifiers.contains(KeyModifiers::ALT);
    let super_key = key.modifiers.contains(KeyModifiers::SUPER);

    match key.code {
        KeyCode::Esc => app.mode = InputMode::Normal,
        KeyCode::Char('s') if ctrl => {
            save_and_report(app);
        }
        KeyCode::Char('r') if ctrl => {
            rate_fetcher.request(app);
        }
        KeyCode::Char('w') if ctrl => app.delete_word_backward(),
        KeyCode::Char('u') if ctrl => app.delete_to_line_start(),
        KeyCode::Backspace if alt => app.delete_word_backward(),
        KeyCode::Backspace if super_key => app.delete_to_line_start(),
        KeyCode::Char(c) if accepts_text_input(key.modifiers) => app.insert_char(c),
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

#[cfg(test)]
mod tests {
    use super::*;
    use crossterm::event::{KeyEvent, KeyModifiers};
    use ratatui::backend::TestBackend;

    fn key(code: KeyCode) -> KeyEvent {
        KeyEvent::new(code, KeyModifiers::NONE)
    }

    fn modified_key(code: KeyCode, modifiers: KeyModifiers) -> KeyEvent {
        KeyEvent::new(code, modifiers)
    }

    #[test]
    fn standard_mode_edits_unicode_and_moves_between_lines() {
        let terminal = Terminal::new(TestBackend::new(40, 8)).unwrap();
        let rate_fetcher = RateFetcher::new();
        let mut app = App::default();
        app.keybinding_mode = KeybindingMode::Standard;
        app.mode = InputMode::Insert;

        handle_standard_mode(key(KeyCode::Char('é')), &mut app, &terminal, &rate_fetcher).unwrap();
        handle_standard_mode(key(KeyCode::Enter), &mut app, &terminal, &rate_fetcher).unwrap();
        handle_standard_mode(key(KeyCode::Char('🧮')), &mut app, &terminal, &rate_fetcher).unwrap();

        assert_eq!(app.lines(), &["é".to_string(), "🧮".to_string()]);
        assert_eq!((app.cursor_x(), app.cursor_y()), (1, 1));
        assert!(app.is_dirty());
    }

    #[test]
    fn standard_mode_supports_terminal_native_deletion_shortcuts() {
        let terminal = Terminal::new(TestBackend::new(40, 8)).unwrap();
        let rate_fetcher = RateFetcher::new();
        let mut app = App::default();
        app.keybinding_mode = KeybindingMode::Standard;

        for c in "hello world  ".chars() {
            handle_standard_mode(key(KeyCode::Char(c)), &mut app, &terminal, &rate_fetcher)
                .unwrap();
        }
        handle_standard_mode(
            modified_key(KeyCode::Backspace, KeyModifiers::ALT),
            &mut app,
            &terminal,
            &rate_fetcher,
        )
        .unwrap();
        assert_eq!(app.lines(), &["hello ".to_string()]);

        handle_standard_mode(
            modified_key(KeyCode::Char('u'), KeyModifiers::CONTROL),
            &mut app,
            &terminal,
            &rate_fetcher,
        )
        .unwrap();
        assert_eq!(app.lines(), &[String::new()]);

        for c in "one two".chars() {
            handle_standard_mode(key(KeyCode::Char(c)), &mut app, &terminal, &rate_fetcher)
                .unwrap();
        }
        handle_standard_mode(
            modified_key(KeyCode::Char('w'), KeyModifiers::CONTROL),
            &mut app,
            &terminal,
            &rate_fetcher,
        )
        .unwrap();
        assert_eq!(app.lines(), &["one ".to_string()]);

        handle_standard_mode(
            modified_key(KeyCode::Backspace, KeyModifiers::SUPER),
            &mut app,
            &terminal,
            &rate_fetcher,
        )
        .unwrap();
        assert_eq!(app.lines(), &[String::new()]);

        for modifiers in [
            KeyModifiers::CONTROL,
            KeyModifiers::ALT,
            KeyModifiers::SUPER,
            KeyModifiers::META,
            KeyModifiers::HYPER,
        ] {
            handle_standard_mode(
                modified_key(KeyCode::Char('x'), modifiers),
                &mut app,
                &terminal,
                &rate_fetcher,
            )
            .unwrap();
        }
        assert_eq!(app.lines(), &[String::new()]);

        handle_standard_mode(
            modified_key(KeyCode::Char('X'), KeyModifiers::SHIFT),
            &mut app,
            &terminal,
            &rate_fetcher,
        )
        .unwrap();
        assert_eq!(app.lines(), &["X".to_string()]);
    }

    #[test]
    fn vim_mode_transitions_and_pending_delete_are_consistent() {
        let terminal = Terminal::new(TestBackend::new(40, 8)).unwrap();
        let rate_fetcher = RateFetcher::new();
        let mut app = App::default();

        handle_vim_mode(key(KeyCode::Char('i')), &mut app, &terminal, &rate_fetcher).unwrap();
        assert_eq!(app.mode, InputMode::Insert);
        handle_vim_mode(key(KeyCode::Char('é')), &mut app, &terminal, &rate_fetcher).unwrap();
        handle_vim_mode(key(KeyCode::Enter), &mut app, &terminal, &rate_fetcher).unwrap();
        handle_vim_mode(key(KeyCode::Char('2')), &mut app, &terminal, &rate_fetcher).unwrap();
        handle_vim_mode(key(KeyCode::Esc), &mut app, &terminal, &rate_fetcher).unwrap();
        assert_eq!(app.mode, InputMode::Normal);

        handle_vim_mode(key(KeyCode::Char('d')), &mut app, &terminal, &rate_fetcher).unwrap();
        assert_eq!(app.pending, PendingCommand::Delete);
        handle_vim_mode(key(KeyCode::Char('d')), &mut app, &terminal, &rate_fetcher).unwrap();

        assert_eq!(app.lines(), &["é".to_string()]);
        assert_eq!(app.pending, PendingCommand::None);
        assert_eq!((app.cursor_x(), app.cursor_y()), (0, 0));
    }

    #[test]
    fn vim_insert_mode_supports_safe_native_deletion_shortcuts() {
        let rate_fetcher = RateFetcher::new();
        let mut app = App::default();
        app.mode = InputMode::Insert;

        for c in "hello world".chars() {
            handle_vim_insert_mode(key(KeyCode::Char(c)), &mut app, &rate_fetcher).unwrap();
        }
        handle_vim_insert_mode(
            modified_key(KeyCode::Char('w'), KeyModifiers::CONTROL),
            &mut app,
            &rate_fetcher,
        )
        .unwrap();
        assert_eq!(app.lines(), &["hello ".to_string()]);

        handle_vim_insert_mode(
            modified_key(KeyCode::Char('u'), KeyModifiers::CONTROL),
            &mut app,
            &rate_fetcher,
        )
        .unwrap();
        assert_eq!(app.lines(), &[String::new()]);
        assert_eq!(app.mode, InputMode::Insert);

        for c in "hello world".chars() {
            handle_vim_insert_mode(key(KeyCode::Char(c)), &mut app, &rate_fetcher).unwrap();
        }
        handle_vim_insert_mode(
            modified_key(KeyCode::Backspace, KeyModifiers::ALT),
            &mut app,
            &rate_fetcher,
        )
        .unwrap();
        assert_eq!(app.lines(), &["hello ".to_string()]);
        handle_vim_insert_mode(
            modified_key(KeyCode::Backspace, KeyModifiers::SUPER),
            &mut app,
            &rate_fetcher,
        )
        .unwrap();
        assert_eq!(app.lines(), &[String::new()]);

        for modifiers in [
            KeyModifiers::CONTROL,
            KeyModifiers::ALT,
            KeyModifiers::SUPER,
            KeyModifiers::META,
            KeyModifiers::HYPER,
        ] {
            handle_vim_insert_mode(
                modified_key(KeyCode::Char('x'), modifiers),
                &mut app,
                &rate_fetcher,
            )
            .unwrap();
        }
        assert_eq!(app.lines(), &[String::new()]);
    }

    #[test]
    fn first_terminal_error_is_not_masked_by_cleanup_errors() {
        let mut first = None;
        remember_first_error(
            Err(io::Error::new(io::ErrorKind::BrokenPipe, "first")),
            &mut first,
        );
        remember_first_error(Err(io::Error::other("second")), &mut first);

        assert_eq!(first.unwrap().to_string(), "first");
    }
}
