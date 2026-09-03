//! Turning a crossterm event into one of ours.
//!
//! # Surface
//!
//! Entry points: [`from_crossterm`], [`event_from_crossterm`].
//!
//! Configurable values: none.
//!
//! Fan-out points: the `match` in `from_crossterm` is the whole key table.
//!
//! Shared by both native backends. Decoding is crossterm's rather than
//! [`crate::decode`]'s because on Windows the input is not an escape stream
//! at all: it is console API records, and crossterm turns both of those into
//! the same event.

use crate::key::{KeyCode, KeyPress, Mods};
use crate::term::{Event, Size};

/// The [`Event`] a crossterm event means, or `None` for one that is not
/// input: focus changes, mouse motion, a key release, a bare modifier.
pub(crate) fn event_from_crossterm(event: crossterm::event::Event) -> Option<Event> {
    match event {
        crossterm::event::Event::Key(key) => {
            // A terminal with the kitty protocol on reports releases and
            // repeats too; only a press is input.
            if key.kind == crossterm::event::KeyEventKind::Release {
                return None;
            }
            from_crossterm(key).map(Event::Key)
        }
        crossterm::event::Event::Resize(cols, rows) => Some(Event::Resize(Size::new(cols, rows))),
        // Focus, mouse and paste events: not this crate's.
        _ => None,
    }
}

pub(crate) fn from_crossterm(event: crossterm::event::KeyEvent) -> Option<KeyPress> {
    use crossterm::event::{KeyCode as Ct, KeyModifiers as CtMods};

    let mut mods = Mods::NONE;
    if event.modifiers.contains(CtMods::CONTROL) {
        mods = mods | Mods::CTRL;
    }
    if event.modifiers.contains(CtMods::ALT) {
        mods = mods | Mods::ALT;
    }
    if event.modifiers.contains(CtMods::SHIFT) {
        mods = mods | Mods::SHIFT;
    }

    let code = match event.code {
        Ct::Char(c) => KeyCode::Char(c),
        Ct::Enter => KeyCode::Enter,
        Ct::Tab => KeyCode::Tab,
        Ct::BackTab => return Some(KeyPress::new(KeyCode::Tab, mods | Mods::SHIFT)),
        Ct::Backspace => KeyCode::Backspace,
        Ct::Delete => KeyCode::Delete,
        Ct::Insert => KeyCode::Insert,
        Ct::Esc => KeyCode::Escape,
        Ct::Left => KeyCode::Left,
        Ct::Right => KeyCode::Right,
        Ct::Up => KeyCode::Up,
        Ct::Down => KeyCode::Down,
        Ct::Home => KeyCode::Home,
        Ct::End => KeyCode::End,
        Ct::PageUp => KeyCode::PageUp,
        Ct::PageDown => KeyCode::PageDown,
        Ct::F(n) => KeyCode::F(n),
        // Ctrl+Space and Ctrl+@ both arrive as NUL.
        Ct::Null => return Some(KeyPress::new(KeyCode::Char(' '), mods | Mods::CTRL)),
        // Lock keys, media keys, bare modifier presses: nothing to edit with.
        _ => return None,
    };

    Some(KeyPress::new(code, mods).normalized())
}
