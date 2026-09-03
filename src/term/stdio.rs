//! Line-at-a-time stdin and stdout, through the platform layer.
//!
//! # Surface
//!
//! Entry points: [`CookedStdio::new`], [`CookedStdio::set_ansi`],
//! [`CookedStdio::set_size`].
//!
//! Configurable values: none beyond those setters.
//!
//! Fan-out points: none.
//!
//! This is the whole terminal on WASI Preview 2, and the fallback natively
//! when stdin is a pipe rather than a tty. In both cases the line
//! discipline belongs to somebody else — the host's terminal driver, or
//! whoever is on the other end of the pipe — so there are no keys to
//! report, only finished lines, and nothing here tries to draw.
//!
//! Reading goes through `ego_platform::io`, which is what makes the WASI
//! case work at all: a plain `std::io::stdin().read()` inside a component
//! blocks the single-threaded runtime and starves every other task, and
//! that module polls `wasi:io` instead.

use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader, Lines};

use crate::Result;
use crate::term::{Capabilities, Event, Size, Terminal};

type StdinLines = Lines<BufReader<ego_platform::io::Stdin>>;

/// Finished lines in, plain text out.
pub struct CookedStdio {
    lines: StdinLines,
    out: ego_platform::io::Stdout,
    ansi: bool,
    size: Size,
}

impl Default for CookedStdio {
    fn default() -> Self {
        Self::new()
    }
}

impl CookedStdio {
    pub fn new() -> Self {
        Self {
            lines: BufReader::new(ego_platform::io::stdin()).lines(),
            out: ego_platform::io::stdout(),
            ansi: stdout_is_terminal(),
            size: Size::DEFAULT,
        }
    }

    /// Whether escapes written here will be interpreted.
    ///
    /// Detected natively; on WASI it is a guess, because a component cannot
    /// ask what its stdout is attached to.
    pub fn set_ansi(&mut self, ansi: bool) {
        self.ansi = ansi;
    }

    /// Tell this terminal how wide it is, if you know better than
    /// [`Size::DEFAULT`].
    pub fn set_size(&mut self, size: Size) {
        self.size = size;
    }
}

impl Terminal for CookedStdio {
    fn capabilities(&self) -> Capabilities {
        Capabilities {
            raw_mode: false,
            ansi: self.ansi,
            resize_events: false,
            line_discipline: true,
        }
    }

    fn size(&self) -> Size {
        self.size
    }

    fn set_raw(&mut self, _enabled: bool) -> Result<()> {
        Ok(()) // nothing to do; `capabilities().raw_mode` already said no
    }

    async fn next_event(&mut self) -> Result<Event> {
        match self.lines.next_line().await? {
            Some(line) => Ok(Event::Line(line)),
            None => Ok(Event::Eof),
        }
    }

    async fn write(&mut self, text: &str) -> Result<()> {
        self.out.write_all(text.as_bytes()).await?;
        Ok(())
    }

    async fn flush(&mut self) -> Result<()> {
        self.out.flush().await?;
        Ok(())
    }
}

// depth: platform probe.

#[cfg(not(target_arch = "wasm32"))]
fn stdout_is_terminal() -> bool {
    use std::io::IsTerminal;
    std::io::stdout().is_terminal()
}

/// WASI Preview 2 has no `isatty`. Assume yes: under `wasmtime run` stdout
/// is usually the host's terminal, and a stray escape in a redirected file
/// is a smaller harm than a colourless prompt in the common case.
#[cfg(target_arch = "wasm32")]
fn stdout_is_terminal() -> bool {
    true
}
