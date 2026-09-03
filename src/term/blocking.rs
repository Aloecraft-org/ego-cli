//! A native terminal that needs no async runtime.
//!
//! # Surface
//!
//! Entry points: [`BlockingNative::new`], [`BlockingNative::poll`],
//! [`BlockingNative::is_raw_capable`].
//!
//! Configurable values: none.
//!
//! Fan-out points: none; key decoding is
//! `term::crossterm_key`'s, shared with `term::native::NativeTerminal`.
//!
//! # Why a second native backend
//!
//! `term::native::NativeTerminal` reads through
//! crossterm's `EventStream`, which wants a reactor, and writes through
//! `ego_platform::io::stdout`, which is tokio's and dispatches to a blocking
//! pool. Both are right for a host that is already async and wrong for one
//! that is not: a state machine whose loop is its own, that wants a line at
//! exactly one point and is happy to block there, would be taking on an
//! async runtime for nothing.
//!
//! So this one blocks. `crossterm::event::read` waits on the terminal
//! directly and `std::io::Stdout` writes directly, which means no future
//! here ever returns `Pending` and the whole of [`Session::read_line`] runs
//! to completion on the first poll — under `futures::executor::block_on`, or
//! any other executor, with no runtime at all:
//!
//! ```no_run
//! # fn run() -> ego_cli::Result<()> {
//! use ego_cli::{Session, term::blocking::BlockingNative};
//!
//! let mut session = Session::new(BlockingNative::new()?);
//! let outcome = futures_executor::block_on(session.read_line())?;
//! # let _ = outcome;
//! # Ok(())
//! # }
//! ```
//!
//! (`futures::executor::block_on` is the same function, re-exported.)
//!
//! [`Session::read_line`]: crate::Session::read_line
//!
//! Nothing is given up but concurrency: a blocked read is a blocked thread,
//! so a host that wants to do something else while waiting should either use
//! the async backend or call [`poll`](BlockingNative::poll) first.

use std::io::{IsTerminal, Write};
use std::time::Duration;

use crate::Result;
use crate::term::crossterm_key::event_from_crossterm;
use crate::term::{Capabilities, Event, Size, Terminal};

/// A native terminal read with blocking crossterm calls and written with
/// `std::io`.
pub struct BlockingNative {
    out: std::io::Stdout,
    raw: bool,
    tty: bool,
}

impl BlockingNative {
    /// Open stdin and stdout.
    ///
    /// Succeeds whether or not stdin is a terminal. When it is not,
    /// [`capabilities`](Terminal::capabilities) reports no raw mode and
    /// whole lines are read from stdin instead, the same shape the async
    /// backend falls back to for a pipe -- just with `std::io` doing the
    /// reading rather than `ego_platform`.
    pub fn new() -> Result<Self> {
        Ok(Self {
            out: std::io::stdout(),
            raw: false,
            tty: std::io::stdin().is_terminal(),
        })
    }

    /// Whether this process got a tty.
    pub fn is_raw_capable(&self) -> bool {
        self.tty
    }

    /// Whether an event is waiting, giving up after `timeout`.
    ///
    /// For a host that wants to interleave its own work with the wait: check
    /// here, and only call into [`Session::read_line`](crate::Session::read_line)
    /// once there is something to read.
    pub fn poll(&mut self, timeout: Duration) -> Result<bool> {
        Ok(crossterm::event::poll(timeout)?)
    }
}

impl Drop for BlockingNative {
    fn drop(&mut self) {
        // Leaving a terminal raw makes the shell that follows unusable, so
        // this is a backstop for a panic on the way out.
        if self.raw {
            let _ = crossterm::terminal::disable_raw_mode();
        }
    }
}

impl Terminal for BlockingNative {
    fn capabilities(&self) -> Capabilities {
        Capabilities {
            raw_mode: self.tty,
            ansi: self.tty,
            resize_events: self.tty,
            line_discipline: true,
        }
    }

    fn size(&self) -> Size {
        // A terminal reporting zero columns is declining to answer, not
        // saying it is one cell wide.
        match crossterm::terminal::size() {
            Ok((cols, rows)) if cols > 0 && rows > 0 => Size::new(cols, rows),
            _ => Size::DEFAULT,
        }
    }

    fn set_raw(&mut self, enabled: bool) -> Result<()> {
        if self.raw == enabled || !self.tty {
            return Ok(());
        }
        if enabled {
            crossterm::terminal::enable_raw_mode()?;
        } else {
            crossterm::terminal::disable_raw_mode()?;
        }
        self.raw = enabled;
        Ok(())
    }

    /// Blocks the calling thread until something happens.
    ///
    /// `crossterm::event::read` waits on the terminal itself, so there is no
    /// polling interval to pick and no idle spinning: the future is ready by
    /// the time it is first polled.
    async fn next_event(&mut self) -> Result<Event> {
        // Not a terminal: there are no keys to report, only whatever the
        // pipe hands over, a line at a time.
        if !self.tty {
            let mut line = String::new();
            return match std::io::stdin().read_line(&mut line)? {
                0 => Ok(Event::Eof),
                _ => Ok(Event::Line(line.trim_end_matches(['\n', '\r']).to_string())),
            };
        }
        loop {
            if let Some(event) = event_from_crossterm(crossterm::event::read()?) {
                return Ok(event);
            }
        }
    }

    async fn write(&mut self, text: &str) -> Result<()> {
        self.out.write_all(text.as_bytes())?;
        Ok(())
    }

    async fn flush(&mut self) -> Result<()> {
        self.out.flush()?;
        Ok(())
    }
}
