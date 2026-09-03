//! Incremental decoder from a terminal's character stream to [`KeyPress`].
//!
//! # Surface
//!
//! Entry points: [`AnsiDecoder::new`], [`AnsiDecoder::push`],
//! [`AnsiDecoder::flush`], [`AnsiDecoder::reset`].
//!
//! Configurable values: none.
//!
//! Fan-out points: `State` is the state machine's closed set of positions;
//! `ground_key`, `csi_dispatch` and `ss3_dispatch` are the three places a
//! sequence turns into a key.
//!
//! Used by any backend handed raw terminal input: the browser (xterm.js
//! `onData`), and anything reading a pty. The native backend does not use
//! it — crossterm already decodes, including the Windows console API, which
//! is not an escape stream at all.
//!
//! # Why a state machine
//!
//! `ego_shell` decoded by chaining `str::replace` over each input chunk,
//! substituting private-use codepoints for the sequences it knew. That is
//! wrong in three ways this is not: a sequence split across two `onData`
//! callbacks decodes as garbage; a private-use character typed by the user
//! is indistinguishable from a substitution; and every new key needs a new
//! `replace` and a new codepoint. Here a control character's meaning falls
//! out of one rule — `\x01`..`\x1a` is `Ctrl` plus a letter — so `Ctrl+W`,
//! `Ctrl+U` and `Ctrl+K` cost nothing to support, and partial sequences stay
//! pending until the rest arrives.

use crate::key::{KeyCode, KeyPress, Mods};

/// Where the machine is between calls to [`AnsiDecoder::push`].
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum State {
    /// Not inside a sequence.
    Ground,
    /// Saw `ESC`.
    Esc,
    /// Saw `ESC [`; collecting parameters.
    Csi,
    /// Saw `ESC O` (application cursor keys).
    Ss3,
}

/// Turns terminal input into key presses, one chunk at a time.
///
/// Feed it whatever arrives, however it is split. State carries across
/// calls, so a sequence delivered in pieces still decodes to one key.
#[derive(Debug)]
pub struct AnsiDecoder {
    state: State,
    params: Vec<u32>,
    current: Option<u32>,
    /// A private CSI (`ESC [ ?` and friends) is a report, not a key; parse
    /// past it and emit nothing.
    private: bool,
}

impl Default for AnsiDecoder {
    fn default() -> Self {
        Self::new()
    }
}

impl AnsiDecoder {
    pub fn new() -> Self {
        Self {
            state: State::Ground,
            params: Vec::new(),
            current: None,
            private: false,
        }
    }

    /// Decode `chunk`, returning the keys it completed.
    ///
    /// A chunk that ends mid-sequence leaves the decoder pending: the keys
    /// come out of a later `push`. The one exception is a chunk ending in a
    /// bare `ESC` with nothing after it, which is reported as
    /// [`KeyCode::Escape`] — a terminal delivers a sequence as one write, so
    /// an `ESC` alone at the end of a write is the Escape key, and waiting
    /// for a byte that will never come would swallow it. A caller that can
    /// tell the difference some other way (a timer, say) should read
    /// [`Self::flush`] instead and split its writes accordingly.
    pub fn push(&mut self, chunk: &str) -> Vec<KeyPress> {
        let mut keys = Vec::new();
        for c in chunk.chars() {
            self.step(c, &mut keys);
        }
        if self.state == State::Esc {
            self.state = State::Ground;
            keys.push(KeyPress::plain(KeyCode::Escape));
        }
        keys
    }

    /// Resolve a pending bare `ESC` into [`KeyCode::Escape`], discarding any
    /// half-read sequence.
    ///
    /// For callers that time out on an idle input stream rather than trust
    /// chunk boundaries.
    pub fn flush(&mut self) -> Vec<KeyPress> {
        let pending = self.state == State::Esc;
        self.reset();
        if pending {
            vec![KeyPress::plain(KeyCode::Escape)]
        } else {
            Vec::new()
        }
    }

    /// Drop any partial sequence and return to the ground state.
    pub fn reset(&mut self) {
        self.state = State::Ground;
        self.params.clear();
        self.current = None;
        self.private = false;
    }

    // depth: the state machine proper.

    fn step(&mut self, c: char, keys: &mut Vec<KeyPress>) {
        match self.state {
            State::Ground => {
                if c == '\x1b' {
                    self.state = State::Esc;
                } else {
                    keys.push(ground_key(c, Mods::NONE));
                }
            }

            State::Esc => match c {
                '[' => {
                    self.state = State::Csi;
                    self.params.clear();
                    self.current = None;
                    self.private = false;
                }
                'O' => self.state = State::Ss3,
                // ESC ESC: the first one was the Escape key.
                '\x1b' => keys.push(KeyPress::plain(KeyCode::Escape)),
                // Anything else is Alt + that key. Terminals send Alt this
                // way by default; Alt+Backspace (ESC DEL) is how most of
                // them spell "delete the previous word".
                _ => {
                    self.state = State::Ground;
                    keys.push(ground_key(c, Mods::ALT));
                }
            },

            State::Csi => match c {
                '0'..='9' => {
                    let digit = c as u32 - '0' as u32;
                    self.current = Some(self.current.unwrap_or(0) * 10 + digit);
                }
                // ':' separates sub-parameters; nothing here reads one, so
                // treat it as a separator and let the value be ignored.
                ';' | ':' => {
                    self.params.push(self.current.take().unwrap_or(0));
                }
                '<' | '=' | '>' | '?' => self.private = true,
                // rxvt spells the modifier as the final byte: '^' is
                // control, '$' is shift, on the ESC [ n ~ family.
                '^' | '$' => {
                    self.finish_csi(c, keys);
                }
                c if ('\u{40}'..='\u{7e}').contains(&c) => {
                    self.finish_csi(c, keys);
                }
                // Intermediates and anything unrecognized: keep collecting.
                _ => {}
            },

            State::Ss3 => {
                self.state = State::Ground;
                if let Some(code) = ss3_dispatch(c) {
                    keys.push(KeyPress::plain(code));
                }
            }
        }
    }

    fn finish_csi(&mut self, final_char: char, keys: &mut Vec<KeyPress>) {
        if let Some(param) = self.current.take() {
            self.params.push(param);
        }
        let private = self.private;
        self.state = State::Ground;
        self.private = false;
        if !private && let Some(key) = csi_dispatch(final_char, &self.params) {
            keys.push(key.normalized());
        }
        self.params.clear();
    }
}

// depth: sequence tables.

/// A character outside any escape sequence.
///
/// The control range is decoded by rule rather than by table: `\x01`..`\x1a`
/// is `Ctrl` plus the matching letter, which is what the terminal means by
/// it and what every readline binding is written against.
fn ground_key(c: char, mods: Mods) -> KeyPress {
    let key = match c {
        // Ctrl+M and Ctrl+J, but nobody means them that way.
        '\r' | '\n' => KeyPress::new(KeyCode::Enter, mods),
        '\t' => KeyPress::new(KeyCode::Tab, mods),
        '\x7f' => KeyPress::new(KeyCode::Backspace, mods),
        // Ctrl+Backspace: xterm.js and the Windows console both send BS
        // here, while a plain Backspace sends DEL above.
        '\x08' => KeyPress::new(KeyCode::Backspace, mods | Mods::CTRL),
        // Ctrl+Space and Ctrl+@ share this byte; space is the one people press.
        '\0' => KeyPress::new(KeyCode::Char(' '), mods | Mods::CTRL),
        '\x01'..='\x1a' => {
            let letter = (b'a' + (c as u8 - 1)) as char;
            KeyPress::new(KeyCode::Char(letter), mods | Mods::CTRL)
        }
        '\x1c' => KeyPress::new(KeyCode::Char('\\'), mods | Mods::CTRL),
        '\x1d' => KeyPress::new(KeyCode::Char(']'), mods | Mods::CTRL),
        '\x1e' => KeyPress::new(KeyCode::Char('^'), mods | Mods::CTRL),
        // Ctrl+_ is also how a terminal reports Ctrl+/, readline's undo.
        '\x1f' => KeyPress::new(KeyCode::Char('_'), mods | Mods::CTRL),
        c => KeyPress::new(KeyCode::Char(c), mods),
    };
    key.normalized()
}

/// `ESC [ params final`.
fn csi_dispatch(final_char: char, params: &[u32]) -> Option<KeyPress> {
    let first = params.first().copied().unwrap_or(0);
    let second = params.get(1).copied();

    let code = match final_char {
        'A' => KeyCode::Up,
        'B' => KeyCode::Down,
        'C' => KeyCode::Right,
        'D' => KeyCode::Left,
        'H' => KeyCode::Home,
        'F' => KeyCode::End,
        'P' => KeyCode::F(1),
        'Q' => KeyCode::F(2),
        'R' => KeyCode::F(3),
        'S' => KeyCode::F(4),
        'Z' => return Some(KeyPress::new(KeyCode::Tab, Mods::SHIFT)),
        '~' | '^' | '$' => tilde_code(first)?,
        _ => return None,
    };

    let mods = match final_char {
        // rxvt encodes the modifier in the final byte.
        '^' => Mods::CTRL,
        '$' => Mods::SHIFT,
        _ => match second {
            Some(param) => Mods::from_xterm_param(param),
            // A lone parameter on an arrow key is the rxvt/PuTTY spelling of
            // a modified arrow (ESC [ 5 D for Ctrl+Left). Standard xterm
            // would read it as "move left 5 columns", but a cursor-movement
            // command is not something a terminal sends *to* an application.
            None if matches!(final_char, 'A' | 'B' | 'C' | 'D') && (2..=8).contains(&first) => {
                Mods::from_xterm_param(first)
            }
            None => Mods::NONE,
        },
    };

    Some(KeyPress::new(code, mods))
}

/// The `n` in `ESC [ n ~`.
fn tilde_code(param: u32) -> Option<KeyCode> {
    Some(match param {
        1 | 7 => KeyCode::Home,
        2 => KeyCode::Insert,
        3 => KeyCode::Delete,
        4 | 8 => KeyCode::End,
        5 => KeyCode::PageUp,
        6 => KeyCode::PageDown,
        11..=15 => KeyCode::F((param - 10) as u8),
        17..=21 => KeyCode::F((param - 11) as u8),
        23..=26 => KeyCode::F((param - 12) as u8),
        _ => return None,
    })
}

/// `ESC O x`, the application-cursor-key form. Sent by a terminal whose
/// keypad is in application mode, which xterm.js does by default for the
/// arrows once an application asks for it.
fn ss3_dispatch(final_char: char) -> Option<KeyCode> {
    Some(match final_char {
        'A' => KeyCode::Up,
        'B' => KeyCode::Down,
        'C' => KeyCode::Right,
        'D' => KeyCode::Left,
        'H' => KeyCode::Home,
        'F' => KeyCode::End,
        'P' => KeyCode::F(1),
        'Q' => KeyCode::F(2),
        'R' => KeyCode::F(3),
        'S' => KeyCode::F(4),
        _ => return None,
    })
}
