//! The native terminal: a tty in raw mode, or a pipe read as lines.
//!
//! # Surface
//!
//! Entry points: [`NativeTerminal::new`], [`NativeTerminal::is_raw_capable`].
//!
//! Configurable values: none; everything is detected.
//!
//! Fan-out points: `Inner` — a tty gets the raw path, anything else falls
//! back to [`CookedStdio`]. `from_crossterm`
//! is where a crossterm key
//! event becomes a [`KeyPress`](crate::key::KeyPress).
//!
//! Decoding is crossterm's here rather than [`crate::decode`]'s, because on
//! Windows the input is not an escape stream at all: it is console API
//! records, and crossterm already turns both of those into the same event.

use futures_util::StreamExt;

use crate::term::crossterm_key::event_from_crossterm;
use crate::term::stdio::CookedStdio;
use crate::term::{Capabilities, Event, Size, Terminal};
use crate::{Error, Result};

use tokio::io::AsyncWriteExt;

/// Which of the two shapes this process actually got.
enum Inner {
    /// stdin is a tty: crossterm reads keys, and raw mode is ours to set.
    Tty {
        events: crossterm::event::EventStream,
        out: ego_platform::io::Stdout,
        raw: bool,
    },
    /// stdin is not a tty: no keys to read, no raw mode to enter.
    Piped(CookedStdio),
}

/// The terminal on Linux, macOS and Windows.
pub struct NativeTerminal {
    inner: Inner,
}

impl NativeTerminal {
    /// Open stdin and stdout, taking the raw path if stdin is a terminal.
    pub fn new() -> Result<Self> {
        use std::io::IsTerminal;

        let inner = if std::io::stdin().is_terminal() {
            Inner::Tty {
                events: crossterm::event::EventStream::new(),
                out: ego_platform::io::stdout(),
                raw: false,
            }
        } else {
            Inner::Piped(CookedStdio::new())
        };
        Ok(Self { inner })
    }

    /// Whether this process got a tty.
    pub fn is_raw_capable(&self) -> bool {
        matches!(self.inner, Inner::Tty { .. })
    }
}

impl Drop for NativeTerminal {
    fn drop(&mut self) {
        // Leaving a terminal in raw mode makes the shell that follows
        // unusable, so this is a backstop for a panic on the way out.
        if let Inner::Tty { raw: true, .. } = self.inner {
            let _ = crossterm::terminal::disable_raw_mode();
        }
    }
}

impl Terminal for NativeTerminal {
    fn capabilities(&self) -> Capabilities {
        match &self.inner {
            Inner::Tty { .. } => Capabilities {
                raw_mode: true,
                ansi: true,
                resize_events: true,
                line_discipline: true,
            },
            Inner::Piped(cooked) => cooked.capabilities(),
        }
    }

    fn size(&self) -> Size {
        match &self.inner {
            // A terminal that reports zero columns — a pty opened without a
            // window size, a console that has not been sized yet — is not
            // one cell wide, it is declining to answer.
            Inner::Tty { .. } => match crossterm::terminal::size() {
                Ok((cols, rows)) if cols > 0 && rows > 0 => Size::new(cols, rows),
                _ => Size::DEFAULT,
            },
            Inner::Piped(cooked) => cooked.size(),
        }
    }

    fn set_raw(&mut self, enabled: bool) -> Result<()> {
        match &mut self.inner {
            Inner::Tty { raw, .. } => {
                if *raw == enabled {
                    return Ok(());
                }
                if enabled {
                    crossterm::terminal::enable_raw_mode()?;
                } else {
                    crossterm::terminal::disable_raw_mode()?;
                }
                *raw = enabled;
                Ok(())
            }
            Inner::Piped(cooked) => cooked.set_raw(enabled),
        }
    }

    async fn next_event(&mut self) -> Result<Event> {
        match &mut self.inner {
            Inner::Tty { events, .. } => loop {
                let Some(event) = events.next().await else {
                    return Ok(Event::Eof);
                };
                if let Some(event) = event_from_crossterm(event.map_err(Error::from)?) {
                    return Ok(event);
                }
            },
            Inner::Piped(cooked) => cooked.next_event().await,
        }
    }

    async fn write(&mut self, text: &str) -> Result<()> {
        match &mut self.inner {
            Inner::Tty { out, .. } => {
                out.write_all(text.as_bytes()).await?;
                Ok(())
            }
            Inner::Piped(cooked) => cooked.write(text).await,
        }
    }

    async fn flush(&mut self) -> Result<()> {
        match &mut self.inner {
            Inner::Tty { out, .. } => {
                out.flush().await?;
                Ok(())
            }
            Inner::Piped(cooked) => cooked.flush().await,
        }
    }
}
