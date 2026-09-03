//! Line editing for command-line applications, on native, WASI Preview 2,
//! and the browser.
//!
//! # Surface
//!
//! Entry points: [`Session`] and [`Session::read_line`], built on a
//! [`Terminal`] — [`term::platform`] gives you the one for this target, or
//! `term::browser::XtermTerminal::attach` takes the page's xterm.js
//! instance. [`ReadOutcome`] is what a read produces. [`Error`] and
//! [`Result`] are this crate's.
//!
//! Configurable values: none here; see [`session`], [`history`] and
//! [`editor`] for theirs.
//!
//! Fan-out points: the module list below. [`term`] is where platforms
//! diverge; [`keymap`] is where a key press becomes an intent; [`extend`]
//! is where a host adds its own behaviour.
//!
//! ```no_run
//! # // The browser has no zero-argument constructor -- there the page hands
//! # // over its xterm.js terminal -- so this example is checked on the two
//! # // targets whose terminal opens itself.
//! # #[cfg(not(all(target_arch = "wasm32", target_os = "unknown")))]
//! # async fn run() -> ego_cli::Result<()> {
//! use ego_cli::{ReadOutcome, Session, term};
//!
//! let mut session = Session::new(term::platform()?);
//! session.set_prompt("diluvium> ");
//!
//! loop {
//!     match session.read_line().await? {
//!         ReadOutcome::Line(line) => session.print(&format!("{line}\n")).await?,
//!         ReadOutcome::Interrupted => continue,
//!         ReadOutcome::Eof => break,
//!     }
//! }
//! # Ok(())
//! # }
//! ```
//!
//! # What the platform decides
//!
//! Every convenience here — word motions, undo, history recall — needs the
//! terminal to hand over keys as they are pressed, and only two of the
//! three targets can:
//!
//! | Target | Keys | Notes |
//! |---|---|---|
//! | Native (`x86_64-*`, …) | yes, on a tty | crossterm sets raw mode and decodes, Windows console included; a pipe falls back to whole lines |
//! | Browser (`wasm32-unknown-unknown`) | yes | xterm.js has no line discipline, so `onData` is already the escape stream a tty would send |
//! | WASI Preview 2 (`wasm32-wasip2`) | no | a component cannot reach the host's termios, so stdin arrives one finished line at a time |
//!
//! A [`Session`] reads [`term::Capabilities`] and runs whichever loop the
//! terminal can support, so the same host code works on all three and gets
//! whatever the platform can give. A host that wants to say so can check
//! [`Session::capabilities`] and tell the user what they are missing.
//!
//! # Extending it
//!
//! [`extend::Completer`] and [`extend::Highlighter`] are the two hooks, and
//! [`keymap::Keymap`] rebinds any key to any [`keymap::Action`]. All three
//! are plain data or plain traits: nothing here reaches back into the host.
//!
//! # Testing against it
//!
//! [`term::mem::MemTerminal`] is a [`Terminal`] made of two buffers. Script
//! the input, read back the escapes, on any target — which is how a
//! completer gets tested without a tty.

pub mod decode;
pub mod editor;
pub mod extend;
pub mod history;
pub mod key;
pub mod keymap;
pub mod render;
pub mod session;
pub mod style;
pub mod term;

pub use editor::{EditOutcome, LineEditor};
pub use extend::{Completer, Completion, Highlighter, NoCompleter, NoHighlighter, WordCompleter};
pub use history::History;
pub use key::{KeyCode, KeyPress, Mods};
pub use keymap::{Action, Keymap};
pub use session::{ReadOutcome, Session};
pub use term::{Capabilities, Event, Size, Terminal};

use std::fmt;
use std::io;

/// Anything that can go wrong reading a line.
///
/// Deliberately small. End of input and Ctrl+C are not errors — they are
/// [`ReadOutcome`] variants, because a caller has to handle them either way
/// and burying them in an error type only makes that harder.
#[derive(Debug)]
pub enum Error {
    /// The terminal's underlying reads or writes failed.
    Io(io::Error),
    /// The terminal itself refused: no raw mode, a lost handle, a JavaScript
    /// exception.
    Terminal(String),
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Error::Io(e) => write!(f, "{e}"),
            Error::Terminal(message) => write!(f, "{message}"),
        }
    }
}

impl std::error::Error for Error {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Error::Io(e) => Some(e),
            Error::Terminal(_) => None,
        }
    }
}

impl From<io::Error> for Error {
    fn from(error: io::Error) -> Self {
        Error::Io(error)
    }
}

pub type Result<T> = std::result::Result<T, Error>;
