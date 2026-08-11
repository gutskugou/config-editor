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
    ShowError,
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
        KeyCode::Char('d') => Action::ShowError,
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

#[cfg(test)]
mod tests {
    use super::{confirm_action, normal_action, text_action, Action};
    use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

    fn key(code: KeyCode) -> KeyEvent {
        KeyEvent::new(code, KeyModifiers::NONE)
    }

    #[test]
    fn normal_keymap_maps_all_binding_keys() {
        let m = |c| key(KeyCode::Char(c));
        assert_eq!(normal_action(m('q')), Action::Quit);
        assert_eq!(normal_action(m('k')), Action::Move(-1));
        assert_eq!(normal_action(key(KeyCode::Up)), Action::Move(-1));
        assert_eq!(normal_action(m('j')), Action::Move(1));
        assert_eq!(normal_action(key(KeyCode::Down)), Action::Move(1));
        assert_eq!(normal_action(m('h')), Action::FocusUp);
        assert_eq!(normal_action(key(KeyCode::Left)), Action::FocusUp);
        assert_eq!(normal_action(key(KeyCode::Esc)), Action::FocusUp);
        assert_eq!(normal_action(m('l')), Action::FocusDown);
        assert_eq!(normal_action(key(KeyCode::Right)), Action::FocusDown);
        assert_eq!(normal_action(key(KeyCode::Enter)), Action::FocusDown);
        assert_eq!(normal_action(m('/')), Action::Search);
        assert_eq!(normal_action(m('s')), Action::Set);
        assert_eq!(normal_action(m('e')), Action::Edit);
        assert_eq!(normal_action(m('r')), Action::Restore);
        assert_eq!(normal_action(m('d')), Action::ShowError);
        assert_eq!(normal_action(m('x')), Action::None);
        let ctrl_c = KeyEvent::new(KeyCode::Char('c'), KeyModifiers::CONTROL);
        assert_eq!(normal_action(ctrl_c), Action::Quit);
    }

    #[test]
    fn confirm_and_text_keymap_map_all_binding_keys() {
        let m = |c| key(KeyCode::Char(c));
        assert_eq!(confirm_action(m('y')), Action::Apply);
        assert_eq!(confirm_action(m('Y')), Action::Apply);
        assert_eq!(confirm_action(m('n')), Action::Reject);
        assert_eq!(confirm_action(m('N')), Action::Reject);
        assert_eq!(confirm_action(key(KeyCode::Esc)), Action::Reject);
        assert_eq!(confirm_action(m('k')), Action::Move(-1));
        assert_eq!(confirm_action(m('j')), Action::Move(1));
        assert_eq!(confirm_action(key(KeyCode::PageUp)), Action::PgUp);
        assert_eq!(confirm_action(key(KeyCode::PageDown)), Action::PgDn);
        assert_eq!(confirm_action(m('q')), Action::Quit);
        assert_eq!(confirm_action(m('x')), Action::None);
        assert_eq!(text_action(key(KeyCode::Esc)), Action::Cancel);
        assert_eq!(text_action(key(KeyCode::Backspace)), Action::Backspace);
        assert_eq!(text_action(key(KeyCode::Enter)), Action::Submit);
        assert_eq!(text_action(m('a')), Action::Char('a'));
        assert_eq!(text_action(m('x')), Action::Char('x'));
        let ctrl_c = KeyEvent::new(KeyCode::Char('c'), KeyModifiers::CONTROL);
        assert_eq!(text_action(ctrl_c), Action::Quit);
    }
}
