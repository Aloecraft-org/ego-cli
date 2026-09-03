//! What a key press means, and the table that decides.
//!
//! # Surface
//!
//! Entry points: [`Action`], [`Keymap::new`], [`Keymap::empty`],
//! [`Keymap::bind`], [`Keymap::unbind`], [`Keymap::lookup`],
//! [`Keymap::bindings`].
//!
//! Configurable values: none; every binding is data, set in
//! [`Keymap::new`].
//!
//! Fan-out points: [`Action`] is the closed set of things a key can do —
//! every variant is handled in `editor::LineEditor::apply`. `DEFAULT_BINDINGS`
//! is the whole default table, in one place, so rebinding is a call rather
//! than a patch.
//!
//! An unbound printable key inserts itself; an unbound anything-else is
//! ignored. That fallback is why the table lists only the exceptions.

use std::collections::HashMap;

use crate::key::{KeyCode, KeyPress, Mods};

/// What the editor should do about a key.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Action {
    /// Insert this character at the cursor.
    Insert(char),
    /// Finish the line and hand it to the caller.
    Accept,
    /// Ask the completer what could follow.
    Complete,

    MoveLeft,
    MoveRight,
    MoveWordLeft,
    MoveWordRight,
    MoveStart,
    MoveEnd,

    HistoryPrev,
    HistoryNext,

    DeleteBack,
    DeleteForward,
    DeleteWordBack,
    DeleteWordForward,
    KillToStart,
    KillToEnd,

    Undo,
    Redo,

    /// Abandon the line but keep the session (Escape).
    Cancel,
    /// Redraw from a clean screen.
    ClearScreen,
    /// The line is abandoned and the caller is told (Ctrl+C).
    Interrupt,
    /// End of input on an empty line (Ctrl+D).
    Eof,
    /// Do nothing.
    Ignore,
}

/// The default table: every key that does something other than insert
/// itself.
///
/// Bindings follow readline where readline has an opinion, and the
/// desktop convention where it does not — so `Ctrl+A`/`Ctrl+E` move to the
/// ends of the line, and `Ctrl+Z`/`Ctrl+Y` are undo and redo. `Ctrl+Z` is
/// safe to take: in raw mode the terminal delivers it as a byte instead of
/// raising SIGTSTP, and in cooked mode this table is never consulted.
const DEFAULT_BINDINGS: &[(KeyCode, Mods, Action)] = &[
    (KeyCode::Enter, Mods::NONE, Action::Accept),
    (KeyCode::Tab, Mods::NONE, Action::Complete),
    (KeyCode::Escape, Mods::NONE, Action::Cancel),
    // Motion.
    (KeyCode::Left, Mods::NONE, Action::MoveLeft),
    (KeyCode::Right, Mods::NONE, Action::MoveRight),
    (KeyCode::Home, Mods::NONE, Action::MoveStart),
    (KeyCode::End, Mods::NONE, Action::MoveEnd),
    (KeyCode::Left, Mods::CTRL, Action::MoveWordLeft),
    (KeyCode::Right, Mods::CTRL, Action::MoveWordRight),
    (KeyCode::Char('b'), Mods::ALT, Action::MoveWordLeft),
    (KeyCode::Char('f'), Mods::ALT, Action::MoveWordRight),
    (KeyCode::Char('a'), Mods::CTRL, Action::MoveStart),
    (KeyCode::Char('e'), Mods::CTRL, Action::MoveEnd),
    // History.
    (KeyCode::Up, Mods::NONE, Action::HistoryPrev),
    (KeyCode::Down, Mods::NONE, Action::HistoryNext),
    (KeyCode::Char('p'), Mods::CTRL, Action::HistoryPrev),
    (KeyCode::Char('n'), Mods::CTRL, Action::HistoryNext),
    // Deletion.
    (KeyCode::Backspace, Mods::NONE, Action::DeleteBack),
    (KeyCode::Delete, Mods::NONE, Action::DeleteForward),
    (KeyCode::Backspace, Mods::CTRL, Action::DeleteWordBack),
    (KeyCode::Backspace, Mods::ALT, Action::DeleteWordBack),
    (KeyCode::Char('w'), Mods::CTRL, Action::DeleteWordBack),
    (KeyCode::Delete, Mods::CTRL, Action::DeleteWordForward),
    (KeyCode::Char('d'), Mods::ALT, Action::DeleteWordForward),
    (KeyCode::Char('u'), Mods::CTRL, Action::KillToStart),
    (KeyCode::Char('k'), Mods::CTRL, Action::KillToEnd),
    // Undo. Ctrl+_ is how a terminal reports readline's Ctrl+/.
    (KeyCode::Char('z'), Mods::CTRL, Action::Undo),
    (KeyCode::Char('_'), Mods::CTRL, Action::Undo),
    (KeyCode::Char('y'), Mods::CTRL, Action::Redo),
    // Signals.
    (KeyCode::Char('c'), Mods::CTRL, Action::Interrupt),
    (KeyCode::Char('d'), Mods::CTRL, Action::Eof),
    (KeyCode::Char('l'), Mods::CTRL, Action::ClearScreen),
];

/// A key-press-to-action table.
#[derive(Clone, Debug)]
pub struct Keymap {
    bindings: HashMap<KeyPress, Action>,
}

impl Default for Keymap {
    fn default() -> Self {
        Self::new()
    }
}

impl Keymap {
    /// The default bindings.
    pub fn new() -> Self {
        let mut map = Self::empty();
        for &(code, mods, action) in DEFAULT_BINDINGS {
            map.bind(KeyPress::new(code, mods), action);
        }
        map
    }

    /// No bindings at all: printable keys still insert themselves, and
    /// nothing else does anything.
    pub fn empty() -> Self {
        Self {
            bindings: HashMap::new(),
        }
    }

    pub fn bind(&mut self, key: KeyPress, action: Action) {
        self.bindings.insert(key.normalized(), action);
    }

    pub fn unbind(&mut self, key: KeyPress) {
        self.bindings.remove(&key.normalized());
    }

    /// What `key` does.
    ///
    /// Falls back to inserting an unbound printable character, so a keymap
    /// only ever lists the exceptions.
    pub fn lookup(&self, key: KeyPress) -> Action {
        let key = key.normalized();
        if let Some(&action) = self.bindings.get(&key) {
            return action;
        }
        match key.code {
            KeyCode::Char(c) if !key.mods.ctrl() && !key.mods.alt() => Action::Insert(c),
            _ => Action::Ignore,
        }
    }

    /// Every explicit binding, in no particular order.
    pub fn bindings(&self) -> impl Iterator<Item = (KeyPress, Action)> {
        self.bindings.iter().map(|(&key, &action)| (key, action))
    }
}
