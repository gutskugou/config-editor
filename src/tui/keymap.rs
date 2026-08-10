use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Action {
    Quit,
    Move(isize),
    FocusUp,
    FocusDown,
    Search,
    Set,
    Edit,
    Restore,
    Apply,
    Reject,
    PgUp,
    PgDn,
    Cancel,
    Submit,
    Backspace,
    Char(char),
    None,
}

pub fn normal_action(key: KeyEvent) -> Action {
    match key.code {
        KeyCode::Char('c') if key.modifiers.contains(KeyModifiers::CONTROL) => Action::Quit,
        KeyCode::Char('q') => Action::Quit,
        KeyCode::Up | KeyCode::Char('k') => Action::Move(-1),
        KeyCode::Down | KeyCode::Char('j') => Action::Move(1),
        KeyCode::Left | KeyCode::Char('h') | KeyCode::Esc => Action::FocusUp,
        KeyCode::Right | KeyCode::Char('l') | KeyCode::Enter => Action::FocusDown,
        KeyCode::Char('/') => Action::Search,
        KeyCode::Char('s') => Action::Set,
        KeyCode::Char('e') => Action::Edit,
        KeyCode::Char('r') => Action::Restore,
        _ => Action::None,
    }
}

pub fn confirm_action(key: KeyEvent) -> Action {
    match key.code {
        KeyCode::Char('c') if key.modifiers.contains(KeyModifiers::CONTROL) => Action::Quit,
        KeyCode::Char('q') => Action::Quit,
        KeyCode::Up | KeyCode::Char('k') => Action::Move(-1),
        KeyCode::Down | KeyCode::Char('j') => Action::Move(1),
        KeyCode::PageUp => Action::PgUp,
        KeyCode::PageDown => Action::PgDn,
        KeyCode::Char('y') | KeyCode::Char('Y') => Action::Apply,
        KeyCode::Char('n') | KeyCode::Char('N') | KeyCode::Esc => Action::Reject,
        _ => Action::None,
    }
}

pub fn text_action(key: KeyEvent) -> Action {
    match key.code {
        KeyCode::Char('c') if key.modifiers.contains(KeyModifiers::CONTROL) => Action::Quit,
        KeyCode::Esc => Action::Cancel,
        KeyCode::Backspace => Action::Backspace,
        KeyCode::Enter => Action::Submit,
        KeyCode::Char(c) => Action::Char(c),
        _ => Action::None,
    }
}
