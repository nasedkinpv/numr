//! numr TUI - Terminal User Interface for the numr calculator

mod app;
mod config;
mod handlers;
mod line_layout;
mod persistence;
mod popups;
mod theme;
mod ui;

use crossterm::{
    cursor::SetCursorStyle,
    event::{
        self, DisableMouseCapture, EnableMouseCapture, Event, KeyCode, KeyModifiers,
        KeyboardEnhancementFlags, PopKeyboardEnhancementFlags, PushKeyboardEnhancementFlags,
    },
    execute,
    terminal::supports_keyboard_enhancement,
};
use ratatui::DefaultTerminal;
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

/// Ratatui owns raw mode and the alternate screen. This session owns only the
/// crossterm features that Ratatui deliberately leaves to applications.
struct TerminalSession {
    terminal: DefaultTerminal,
    active: bool,
    keyboard_enhancement: bool,
}

impl TerminalSession {
    fn enter() -> io::Result<Self> {
        let terminal = match ratatui::try_init() {
            Ok(terminal) => terminal,
            Err(error) => {
                ratatui::restore();
                return Err(error);
            }
        };
        let mut session = Self {
            terminal,
            active: true,
            keyboard_enhancement: false,
        };

        let setup = (|| {
            execute!(
                session.terminal.backend_mut(),
                EnableMouseCapture,
                SetCursorStyle::DefaultUserShape
            )?;
            if matches!(supports_keyboard_enhancement(), Ok(true)) {
                session.keyboard_enhancement = true;
                execute!(
                    session.terminal.backend_mut(),
                    PushKeyboardEnhancementFlags(
                        KeyboardEnhancementFlags::DISAMBIGUATE_ESCAPE_CODES
                    )
                )?;
            }
            Ok(())
        })();
        if let Err(error) = setup {
            let _ = session.restore();
            return Err(error);
        }

        Ok(session)
    }

    fn terminal_mut(&mut self) -> &mut DefaultTerminal {
        &mut self.terminal
    }

    fn restore(&mut self) -> io::Result<()> {
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
        remember_first_error(
            execute!(
                self.terminal.backend_mut(),
                DisableMouseCapture,
                SetCursorStyle::DefaultUserShape
            ),
            &mut first_error,
        );
        if let Err(error) = ratatui::try_restore() {
            remember_first_error(Err(error), &mut first_error);
            // try_restore short-circuits if disabling raw mode fails. Still
            // attempt the lower-impact screen cleanup before returning.
            remember_first_error(
                execute!(
                    self.terminal.backend_mut(),
                    crossterm::terminal::LeaveAlternateScreen
                ),
                &mut first_error,
            );
        }
        remember_first_error(self.terminal.show_cursor(), &mut first_error);

        first_error.map_or(Ok(()), Err)
    }
}

impl Drop for TerminalSession {
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

fn main() -> io::Result<()> {
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
    let mut terminal = TerminalSession::enter()?;

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

fn run_app(
    terminal: &mut DefaultTerminal,
    app: &mut App,
    rate_fetcher: &RateFetcher,
) -> io::Result<()> {
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

        let poll_timeout = app.next_wakeup().unwrap_or(Duration::from_secs(60 * 60));
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
                    let terminal_height = terminal.size()?.height;
                    let result = match app.keybinding_mode {
                        KeybindingMode::Standard => {
                            handle_standard_mode(key, app, terminal_height, rate_fetcher)
                        }
                        KeybindingMode::Vim => {
                            handle_vim_mode(key, app, terminal_height, rate_fetcher)
                        }
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
fn update_cursor_style(stdout: &mut io::Stdout, app: &App) -> io::Result<()> {
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
fn handle_standard_mode(
    key: crossterm::event::KeyEvent,
    app: &mut App,
    terminal_height: u16,
    rate_fetcher: &RateFetcher,
) -> ControlFlow {
    let ctrl = key.modifiers.contains(KeyModifiers::CONTROL);
    let alt = key.modifiers.contains(KeyModifiers::ALT);
    let super_key = key.modifiers.contains(KeyModifiers::SUPER);

    // Help popup handling
    if handle_help(key.code, app, terminal_height) {
        return ControlFlow::Continue;
    }

    match key.code {
        KeyCode::Char('q') if ctrl => match handle_quit(app) {
            QuitResult::Exit => return ControlFlow::Exit,
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

    ControlFlow::Continue
}

/// Handle Vim mode keys (modal editing)
fn handle_vim_mode(
    key: crossterm::event::KeyEvent,
    app: &mut App,
    terminal_height: u16,
    rate_fetcher: &RateFetcher,
) -> ControlFlow {
    match app.mode {
        InputMode::Normal => handle_vim_normal_mode(key, app, terminal_height, rate_fetcher),
        InputMode::Insert => handle_vim_insert_mode(key, app, rate_fetcher),
    }
}

/// Handle Vim Normal mode keys
fn handle_vim_normal_mode(
    key: crossterm::event::KeyEvent,
    app: &mut App,
    terminal_height: u16,
    rate_fetcher: &RateFetcher,
) -> ControlFlow {
    // Handle pending commands first
    match app.pending {
        PendingCommand::Delete => {
            if key.code == KeyCode::Char('d') {
                app.delete_line();
            }
            app.pending = PendingCommand::None;
            return ControlFlow::Continue;
        }
        PendingCommand::Go => {
            if key.code == KeyCode::Char('g') {
                app.move_to_first_line();
            }
            app.pending = PendingCommand::None;
            return ControlFlow::Continue;
        }
        PendingCommand::None => {}
    }

    if handle_help(key.code, app, terminal_height) {
        return ControlFlow::Continue;
    }

    let ctrl = key.modifiers.contains(KeyModifiers::CONTROL);

    match key.code {
        KeyCode::Char('q') => match handle_quit(app) {
            QuitResult::Exit => return ControlFlow::Exit,
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

    ControlFlow::Continue
}

/// Handle Vim Insert mode keys
fn handle_vim_insert_mode(
    key: crossterm::event::KeyEvent,
    app: &mut App,
    rate_fetcher: &RateFetcher,
) -> ControlFlow {
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

    ControlFlow::Continue
}

#[cfg(test)]
mod tests {
    use super::*;
    use crossterm::event::{KeyEvent, KeyModifiers};

    const TERMINAL_HEIGHT: u16 = 8;

    fn key(code: KeyCode) -> KeyEvent {
        KeyEvent::new(code, KeyModifiers::NONE)
    }

    fn modified_key(code: KeyCode, modifiers: KeyModifiers) -> KeyEvent {
        KeyEvent::new(code, modifiers)
    }

    #[test]
    fn standard_mode_edits_unicode_and_moves_between_lines() {
        let rate_fetcher = RateFetcher::new();
        let mut app = App::default();
        app.keybinding_mode = KeybindingMode::Standard;
        app.mode = InputMode::Insert;

        handle_standard_mode(
            key(KeyCode::Char('é')),
            &mut app,
            TERMINAL_HEIGHT,
            &rate_fetcher,
        );
        handle_standard_mode(
            key(KeyCode::Enter),
            &mut app,
            TERMINAL_HEIGHT,
            &rate_fetcher,
        );
        handle_standard_mode(
            key(KeyCode::Char('🧮')),
            &mut app,
            TERMINAL_HEIGHT,
            &rate_fetcher,
        );

        assert_eq!(app.lines(), &["é".to_string(), "🧮".to_string()]);
        assert_eq!((app.cursor_x(), app.cursor_y()), (1, 1));
        assert!(app.is_dirty());
    }

    #[test]
    fn standard_mode_supports_terminal_native_deletion_shortcuts() {
        let rate_fetcher = RateFetcher::new();
        let mut app = App::default();
        app.keybinding_mode = KeybindingMode::Standard;

        for c in "hello world  ".chars() {
            handle_standard_mode(
                key(KeyCode::Char(c)),
                &mut app,
                TERMINAL_HEIGHT,
                &rate_fetcher,
            );
        }
        handle_standard_mode(
            modified_key(KeyCode::Backspace, KeyModifiers::ALT),
            &mut app,
            TERMINAL_HEIGHT,
            &rate_fetcher,
        );
        assert_eq!(app.lines(), &["hello ".to_string()]);

        handle_standard_mode(
            modified_key(KeyCode::Char('u'), KeyModifiers::CONTROL),
            &mut app,
            TERMINAL_HEIGHT,
            &rate_fetcher,
        );
        assert_eq!(app.lines(), &[String::new()]);

        for c in "one two".chars() {
            handle_standard_mode(
                key(KeyCode::Char(c)),
                &mut app,
                TERMINAL_HEIGHT,
                &rate_fetcher,
            );
        }
        handle_standard_mode(
            modified_key(KeyCode::Char('w'), KeyModifiers::CONTROL),
            &mut app,
            TERMINAL_HEIGHT,
            &rate_fetcher,
        );
        assert_eq!(app.lines(), &["one ".to_string()]);

        handle_standard_mode(
            modified_key(KeyCode::Backspace, KeyModifiers::SUPER),
            &mut app,
            TERMINAL_HEIGHT,
            &rate_fetcher,
        );
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
                TERMINAL_HEIGHT,
                &rate_fetcher,
            );
        }
        assert_eq!(app.lines(), &[String::new()]);

        handle_standard_mode(
            modified_key(KeyCode::Char('X'), KeyModifiers::SHIFT),
            &mut app,
            TERMINAL_HEIGHT,
            &rate_fetcher,
        );
        assert_eq!(app.lines(), &["X".to_string()]);
    }

    #[test]
    fn vim_mode_transitions_and_pending_delete_are_consistent() {
        let rate_fetcher = RateFetcher::new();
        let mut app = App::default();

        handle_vim_mode(
            key(KeyCode::Char('i')),
            &mut app,
            TERMINAL_HEIGHT,
            &rate_fetcher,
        );
        assert_eq!(app.mode, InputMode::Insert);
        for code in [
            KeyCode::Char('é'),
            KeyCode::Enter,
            KeyCode::Char('2'),
            KeyCode::Esc,
        ] {
            handle_vim_mode(key(code), &mut app, TERMINAL_HEIGHT, &rate_fetcher);
        }
        assert_eq!(app.mode, InputMode::Normal);

        handle_vim_mode(
            key(KeyCode::Char('d')),
            &mut app,
            TERMINAL_HEIGHT,
            &rate_fetcher,
        );
        assert_eq!(app.pending, PendingCommand::Delete);
        handle_vim_mode(
            key(KeyCode::Char('d')),
            &mut app,
            TERMINAL_HEIGHT,
            &rate_fetcher,
        );

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
            handle_vim_insert_mode(key(KeyCode::Char(c)), &mut app, &rate_fetcher);
        }
        handle_vim_insert_mode(
            modified_key(KeyCode::Char('w'), KeyModifiers::CONTROL),
            &mut app,
            &rate_fetcher,
        );
        assert_eq!(app.lines(), &["hello ".to_string()]);

        handle_vim_insert_mode(
            modified_key(KeyCode::Char('u'), KeyModifiers::CONTROL),
            &mut app,
            &rate_fetcher,
        );
        assert_eq!(app.lines(), &[String::new()]);
        assert_eq!(app.mode, InputMode::Insert);

        for c in "hello world".chars() {
            handle_vim_insert_mode(key(KeyCode::Char(c)), &mut app, &rate_fetcher);
        }
        handle_vim_insert_mode(
            modified_key(KeyCode::Backspace, KeyModifiers::ALT),
            &mut app,
            &rate_fetcher,
        );
        assert_eq!(app.lines(), &["hello ".to_string()]);
        handle_vim_insert_mode(
            modified_key(KeyCode::Backspace, KeyModifiers::SUPER),
            &mut app,
            &rate_fetcher,
        );
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
            );
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
