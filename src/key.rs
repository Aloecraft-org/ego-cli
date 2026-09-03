//! The normalized key vocabulary every backend decodes into.
//!
//! # Surface
//!
//! Entry points: [`KeyPress`] and its constructors ([`KeyPress::plain`],
//! [`KeyPress::char`], [`KeyPress::ctrl`], [`KeyPress::alt`],
//! [`KeyPress::normalized`]); [`KeyCode`]; [`Mods`].
//!
//! Configurable values: none.
//!
//! Fan-out points: [`KeyCode`] is the closed set of physical keys a backend
//! may report, and [`Mods`] the closed set of modifiers. Adding a key means
//! adding a variant here and a case in every decoder
//! (`decode::AnsiDecoder`, `term::native`).
//!
//! A `KeyPress` says what the human pressed, never what it should do —
//! that mapping lives in [`crate::keymap`], so a host can rebind without
//! touching a decoder. This is the one place the transplanted `ego_shell`
//! design changed shape: its `NormalizedKey` mixed the two (`CtrlLeft` and
//! `Undo` were siblings), which left no room for rebinding and forced a new
//! enum variant for every modifier combination.

use std::fmt;
use std::ops::BitOr;

/// Modifier keys held during a key press.
///
/// A bit set rather than an enum: `CTRL | SHIFT` is one value, and a
/// decoder that learns about a new combination needs no new variant.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Default)]
pub struct Mods(u8);

impl Mods {
    pub const NONE: Mods = Mods(0);
    pub const SHIFT: Mods = Mods(1);
    pub const ALT: Mods = Mods(2);
    pub const CTRL: Mods = Mods(4);

    pub const fn is_empty(self) -> bool {
        self.0 == 0
    }

    pub const fn contains(self, other: Mods) -> bool {
        self.0 & other.0 == other.0
    }

    pub const fn union(self, other: Mods) -> Mods {
        Mods(self.0 | other.0)
    }

    pub const fn without(self, other: Mods) -> Mods {
        Mods(self.0 & !other.0)
    }

    pub const fn ctrl(self) -> bool {
        self.contains(Mods::CTRL)
    }

    pub const fn alt(self) -> bool {
        self.contains(Mods::ALT)
    }

    pub const fn shift(self) -> bool {
        self.contains(Mods::SHIFT)
    }

    /// Decode an xterm modifier parameter: the `5` in `ESC [ 1 ; 5 D`.
    ///
    /// The parameter is `1 + bits`, where bit 0 is shift, 1 is alt, 2 is
    /// control. Higher bits (meta, hyper, super) have no counterpart here
    /// and are dropped.
    pub const fn from_xterm_param(param: u32) -> Mods {
        if param == 0 {
            return Mods::NONE;
        }
        Mods(((param - 1) & 0b111) as u8)
    }
}

impl BitOr for Mods {
    type Output = Mods;

    fn bitor(self, rhs: Mods) -> Mods {
        self.union(rhs)
    }
}

impl fmt::Debug for Mods {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.is_empty() {
            return f.write_str("NONE");
        }
        let mut first = true;
        for (bit, name) in [
            (Mods::CTRL, "CTRL"),
            (Mods::ALT, "ALT"),
            (Mods::SHIFT, "SHIFT"),
        ] {
            if self.contains(bit) {
                if !first {
                    f.write_str("|")?;
                }
                f.write_str(name)?;
                first = false;
            }
        }
        Ok(())
    }
}

/// A physical key, modifiers excluded.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum KeyCode {
    Char(char),
    Enter,
    Tab,
    Backspace,
    Delete,
    Insert,
    Escape,
    Left,
    Right,
    Up,
    Down,
    Home,
    End,
    PageUp,
    PageDown,
    /// Function key, 1-based: `F(1)` is F1.
    F(u8),
}

/// A key press: a [`KeyCode`] and the [`Mods`] held with it.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub struct KeyPress {
    pub code: KeyCode,
    pub mods: Mods,
}

impl KeyPress {
    pub const fn new(code: KeyCode, mods: Mods) -> Self {
        Self { code, mods }
    }

    pub const fn plain(code: KeyCode) -> Self {
        Self::new(code, Mods::NONE)
    }

    pub const fn char(c: char) -> Self {
        Self::new(KeyCode::Char(c), Mods::NONE)
    }

    pub fn ctrl(c: char) -> Self {
        Self::new(KeyCode::Char(c.to_ascii_lowercase()), Mods::CTRL)
    }

    pub fn alt(c: char) -> Self {
        Self::new(KeyCode::Char(c), Mods::ALT)
    }

    /// The form used for keymap lookup.
    ///
    /// Terminals disagree about the case of a control character's letter —
    /// `ESC [` reports `Ctrl+A` as an uppercase `A`, a raw `\x01` carries no
    /// case at all — so a control-modified ASCII letter is folded to
    /// lowercase and its (meaningless, terminal-invented) shift bit dropped.
    /// Everything else is left exactly as the decoder reported it.
    pub fn normalized(self) -> Self {
        match self.code {
            KeyCode::Char(c) if self.mods.ctrl() && c.is_ascii_alphabetic() => Self::new(
                KeyCode::Char(c.to_ascii_lowercase()),
                self.mods.without(Mods::SHIFT),
            ),
            _ => self,
        }
    }
}
