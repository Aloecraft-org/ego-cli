//! Terminals: what the session reads from and writes to.
//!
//! # Surface
//!
//! Entry points: the [`Terminal`] trait; [`Event`], [`Capabilities`],
//! [`Size`]; [`PlatformTerminal`] and [`platform`], which give you the right
//! backend for wherever this was compiled to run.
//!
//! Configurable values: [`Size::DEFAULT`], the size assumed when the
//! platform will not say.
//!
//! Fan-out points: the backends. [`mem::MemTerminal`] everywhere;
//! `blocking::BlockingNative` and `native::NativeTerminal` off wasm;
//! `stdio::CookedStdio` wherever there is a stdin; `browser::XtermTerminal`
//! in the browser. Which one [`PlatformTerminal`] names is decided at the
//! bottom of this file, and nowhere else.
//!
//! # Two native backends
//!
//! `native::NativeTerminal` reads through crossterm's `EventStream` and
//! writes through `ego_platform`, both of which want an async runtime.
//! `blocking::BlockingNative` blocks on crossterm and writes with
//! `std::io`, so no future it returns is ever `Pending` and a `Session`
//! over it runs to completion under any executor -- no runtime, and no
//! tokio in the dependency tree. The `runtime` feature (on by default)
//! selects which one [`platform`] gives you; both types are there either
//! way, so a host that wants the other one names it.
//!
//! # Raw mode is the axis
//!
//! Everything a line editor does — moving by word, recalling history,
//! undoing — needs the terminal to hand over keys as they are pressed. Two
//! of the three targets can do that: natively by asking the tty, in the
//! browser because xterm.js has no other mode. WASI Preview 2 cannot: a
//! component has no way to reach the host's termios, so its stdin arrives a
//! finished line at a time, exactly as the host's own line discipline
//! produced it.
//!
//! So a backend reports what it can do through [`Capabilities`] and
//! delivers either [`Event::Key`] or [`Event::Line`], and
//! `session::Session::read_line` runs the editing loop or the plain one
//! accordingly. A host that wants to know writes to
//! [`Capabilities::raw_mode`]; a host that does not gets a working prompt
//! either way, with fewer conveniences where the platform has fewer to
//! give.

use std::future::Future;

use crate::Result;
use crate::key::KeyPress;

pub mod mem;

#[cfg(all(
    feature = "runtime",
    not(all(target_arch = "wasm32", target_os = "unknown"))
))]
pub mod stdio;

#[cfg(not(target_arch = "wasm32"))]
pub mod blocking;
#[cfg(not(target_arch = "wasm32"))]
pub(crate) mod crossterm_key;
#[cfg(all(feature = "runtime", not(target_arch = "wasm32")))]
pub mod native;

#[cfg(all(target_arch = "wasm32", target_os = "unknown"))]
pub mod browser;

/// A terminal's dimensions, in cells.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct Size {
    pub cols: u16,
    pub rows: u16,
}

impl Size {
    /// What to assume when the platform will not say — the VT100 default,
    /// still the convention for a terminal of unknown size.
    pub const DEFAULT: Size = Size { cols: 80, rows: 24 };

    pub const fn new(cols: u16, rows: u16) -> Self {
        Self { cols, rows }
    }
}

impl Default for Size {
    fn default() -> Self {
        Size::DEFAULT
    }
}

/// What a backend can do.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct Capabilities {
    /// Keys arrive as they are pressed, so the session can edit the line.
    /// False means input arrives a finished line at a time.
    pub raw_mode: bool,
    /// Escape sequences written to this terminal are interpreted rather
    /// than printed. False means draw nothing but the text.
    pub ansi: bool,
    /// [`Event::Resize`] will be delivered when the terminal changes size.
    pub resize_events: bool,
    /// Outside raw mode this terminal returns the carriage for a lone
    /// `\n` on its own, the way a tty's line discipline does. False means
    /// every line break has to say `\r\n` — which is the browser, where
    /// there is no discipline to turn on.
    pub line_discipline: bool,
}

/// Something that happened at the terminal.
#[derive(Clone, PartialEq, Eq, Debug)]
pub enum Event {
    /// A key press. Only from a terminal with `raw_mode`.
    Key(KeyPress),
    /// A finished line, newline already stripped. Only from a terminal
    /// without `raw_mode`.
    Line(String),
    /// The terminal changed size.
    Resize(Size),
    /// No more input will arrive.
    Eof,
}

/// A source of input and a sink for output.
///
/// The methods that touch the outside world are `async`; the ones that read
/// state the backend already has are not. Implementations return their own
/// futures rather than boxed ones, which keeps the browser's non-`Send`
/// futures legal without infecting the native ones — the reason
/// `Terminal` is used generically and never as a `dyn`.
pub trait Terminal {
    fn capabilities(&self) -> Capabilities;

    fn size(&self) -> Size;

    /// Put the terminal in or out of raw mode.
    ///
    /// A backend that has no raw mode, or is always in one, succeeds
    /// without doing anything: the session asks unconditionally and reads
    /// [`Capabilities::raw_mode`] to know what it got.
    fn set_raw(&mut self, enabled: bool) -> Result<()>;

    /// The next event, waiting for one if there is none yet.
    fn next_event(&mut self) -> impl Future<Output = Result<Event>>;

    /// Queue `text` for the terminal.
    fn write(&mut self, text: &str) -> impl Future<Output = Result<()>>;

    /// Make everything written so far visible.
    fn flush(&mut self) -> impl Future<Output = Result<()>>;
}

// The backend for wherever this was compiled to run. The browser needs the
// page's xterm.js instance handed to it, so it has no `platform()` — call
// `browser::XtermTerminal::attach` with the terminal object instead.

/// The backend for this target.
///
/// With the `runtime` feature (the default) this is the async backend; with
/// it off, [`blocking::BlockingNative`], which needs no executor. Both types
/// exist whenever they compile, so a host that wants the other one names it
/// directly rather than flipping a feature.
#[cfg(all(feature = "runtime", not(target_arch = "wasm32")))]
pub type PlatformTerminal = native::NativeTerminal;

/// The backend for this target: no `runtime` feature, so no async runtime.
#[cfg(all(not(feature = "runtime"), not(target_arch = "wasm32")))]
pub type PlatformTerminal = blocking::BlockingNative;

/// The backend for this target.
///
/// WASI Preview 2 has no way to ask the host for raw mode, so this is the
/// line-at-a-time backend; see the module docs.
#[cfg(all(feature = "runtime", target_arch = "wasm32", target_env = "p2"))]
pub type PlatformTerminal = stdio::CookedStdio;

/// Open the terminal this target has.
#[cfg(all(feature = "runtime", not(target_arch = "wasm32")))]
pub fn platform() -> Result<PlatformTerminal> {
    native::NativeTerminal::new()
}

/// Open the terminal this target has.
#[cfg(all(not(feature = "runtime"), not(target_arch = "wasm32")))]
pub fn platform() -> Result<PlatformTerminal> {
    blocking::BlockingNative::new()
}

/// Open the terminal this target has.
#[cfg(all(feature = "runtime", target_arch = "wasm32", target_env = "p2"))]
pub fn platform() -> Result<PlatformTerminal> {
    Ok(stdio::CookedStdio::new())
}
