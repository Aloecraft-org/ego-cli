//! A terminal made of two buffers, for tests and for driving the editor
//! from something that is not a terminal at all.
//!
//! # Surface
//!
//! Entry points: [`MemTerminal::raw`], [`MemTerminal::cooked`], the input
//! feeders [`MemTerminal::push_input`], [`MemTerminal::push_key`],
//! [`MemTerminal::push_line`], [`MemTerminal::push_resize`],
//! [`MemTerminal::push_eof`], and the output readers
//! [`MemTerminal::output`], [`MemTerminal::take_output`],
//! [`MemTerminal::is_raw`], [`MemTerminal::raw_calls`].
//!
//! Configurable values: none; size and capabilities are constructor
//! arguments.
//!
//! Fan-out points: none.
//!
//! This is part of the library rather than its test suite on purpose: a
//! host writing a completer wants to assert what Tab does without owning a
//! tty, and every test in this crate that exercises a whole session runs
//! against this backend on all three targets.

use std::collections::VecDeque;

use crate::Result;
use crate::decode::AnsiDecoder;
use crate::key::KeyPress;
use crate::term::{Capabilities, Event, Size, Terminal};

/// Scripted input in, written bytes out.
#[derive(Debug)]
pub struct MemTerminal {
    events: VecDeque<Event>,
    output: String,
    size: Size,
    capabilities: Capabilities,
    decoder: AnsiDecoder,
    raw: bool,
    raw_calls: Vec<bool>,
}

impl MemTerminal {
    /// A terminal that delivers key presses, like a tty or xterm.js.
    pub fn raw(size: Size) -> Self {
        Self::with_capabilities(
            size,
            Capabilities {
                raw_mode: true,
                ansi: true,
                resize_events: true,
                line_discipline: false,
            },
        )
    }

    /// A terminal that delivers finished lines, like WASI Preview 2.
    pub fn cooked(size: Size) -> Self {
        Self::with_capabilities(
            size,
            Capabilities {
                raw_mode: false,
                ansi: false,
                resize_events: false,
                line_discipline: true,
            },
        )
    }

    pub fn with_capabilities(size: Size, capabilities: Capabilities) -> Self {
        Self {
            events: VecDeque::new(),
            output: String::new(),
            size,
            capabilities,
            decoder: AnsiDecoder::new(),
            raw: false,
            raw_calls: Vec::new(),
        }
    }

    /// Queue what a terminal would send for `input`, decoded the same way
    /// the browser backend decodes xterm.js: `"ls\x1b[D\r"` is two
    /// characters, a Left, and an Enter.
    pub fn push_input(&mut self, input: &str) -> &mut Self {
        for key in self.decoder.push(input) {
            self.events.push_back(Event::Key(key));
        }
        self
    }

    pub fn push_key(&mut self, key: KeyPress) -> &mut Self {
        self.events.push_back(Event::Key(key));
        self
    }

    pub fn push_line(&mut self, line: &str) -> &mut Self {
        self.events.push_back(Event::Line(line.to_string()));
        self
    }

    pub fn push_resize(&mut self, size: Size) -> &mut Self {
        self.size = size;
        self.events.push_back(Event::Resize(size));
        self
    }

    pub fn push_eof(&mut self) -> &mut Self {
        self.events.push_back(Event::Eof);
        self
    }

    /// Everything written so far, escapes included.
    pub fn output(&self) -> &str {
        &self.output
    }

    pub fn take_output(&mut self) -> String {
        std::mem::take(&mut self.output)
    }

    /// Whether [`Terminal::set_raw`] was last called with `true`.
    pub fn is_raw(&self) -> bool {
        self.raw
    }

    /// Every [`Terminal::set_raw`] argument, in order.
    ///
    /// A session that fails to put the terminal back is a bug that only
    /// shows up in the shell the user runs next, so it is worth being able
    /// to assert on.
    pub fn raw_calls(&self) -> &[bool] {
        &self.raw_calls
    }
}

impl Terminal for MemTerminal {
    fn capabilities(&self) -> Capabilities {
        self.capabilities
    }

    fn size(&self) -> Size {
        self.size
    }

    fn set_raw(&mut self, enabled: bool) -> Result<()> {
        self.raw = enabled;
        self.raw_calls.push(enabled);
        Ok(())
    }

    async fn next_event(&mut self) -> Result<Event> {
        // Running out of script is end of input, not a hang: a test that
        // forgot an Enter should fail, not block forever.
        Ok(self.events.pop_front().unwrap_or(Event::Eof))
    }

    async fn write(&mut self, text: &str) -> Result<()> {
        self.output.push_str(text);
        Ok(())
    }

    async fn flush(&mut self) -> Result<()> {
        Ok(())
    }
}
