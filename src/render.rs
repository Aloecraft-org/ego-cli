//! Drawing the prompt and the line, and putting the cursor back.
//!
//! # Surface
//!
//! Entry points: [`Renderer::new`], [`Renderer::frame`],
//! [`Renderer::erase`], [`Renderer::finish`], [`Renderer::forget`],
//! [`Renderer::set_cols`].
//!
//! Configurable values: none; the width comes from the terminal.
//!
//! Fan-out points: none.
//!
//! The renderer emits ANSI, which is the one thing every target here
//! agrees on: a native terminal, wasmtime's passthrough, and xterm.js all
//! read the same escapes. Nothing in this module is platform-specific.
//!
//! # Why the trailing space
//!
//! Terminals disagree about where the cursor is after writing the last cell
//! of a row. xterm defers the wrap and leaves the cursor on the old row;
//! others wrap immediately. When the drawn text ends exactly on the margin,
//! [`frame`](Renderer::frame) writes one space and a carriage return: on a
//! deferred terminal the space resolves the pending wrap, on an eager one it
//! lands harmlessly at the start of the row that was already entered, and
//! both end at column zero of the same row. [`finish`](Renderer::finish)
//! erases that space before moving on.

use std::fmt::Write as _;

use crate::style::display_width;

/// Tracks where it last drew, so the next frame can return to the top of
/// the block and redraw it.
#[derive(Clone, Debug)]
pub struct Renderer {
    cols: u16,
    /// Rows below the block's first row where the cursor was left.
    cursor_row: u16,
    /// Rows below the block's first row where the drawn text ended.
    end_row: u16,
    /// The last frame ended on the margin and wrote a padding space.
    padded: bool,
    drawn: bool,
}

impl Renderer {
    pub fn new(cols: u16) -> Self {
        Self {
            cols: cols.max(1),
            cursor_row: 0,
            end_row: 0,
            padded: false,
            drawn: false,
        }
    }

    /// Tell the renderer the terminal is a different width now.
    ///
    /// The terminal reflows what is already on screen when it resizes, and
    /// no escape sequence reports where that left the cursor, so the frame
    /// drawn immediately after a resize can smudge. The one after it is
    /// correct.
    pub fn set_cols(&mut self, cols: u16) {
        self.cols = cols.max(1);
    }

    pub fn cols(&self) -> u16 {
        self.cols
    }

    /// Forget the drawn block without erasing it: the next frame draws
    /// where the cursor is now.
    ///
    /// For when something else has written to the terminal — a clear, a log
    /// line, a printed completion list.
    pub fn forget(&mut self) {
        self.cursor_row = 0;
        self.end_row = 0;
        self.padded = false;
        self.drawn = false;
    }

    /// The escapes that replace the drawn block with `prompt` + `line` and
    /// leave the cursor `cursor_cols` cells into the line.
    ///
    /// Both strings may carry SGR escapes; widths ignore them. Neither may
    /// contain a newline — this is a line editor, and a newline would move
    /// the cursor somewhere the block bookkeeping cannot follow.
    pub fn frame(&mut self, prompt: &str, line: &str, cursor_cols: usize) -> String {
        let cols = self.cols as usize;
        let mut out = String::with_capacity(prompt.len() + line.len() + 32);

        self.rewind(&mut out);
        out.push_str(prompt);
        out.push_str(line);

        let total = display_width(prompt) + display_width(line);
        self.padded = total > 0 && total.is_multiple_of(cols);
        if self.padded {
            out.push_str(" \r");
        }
        let end_row = total / cols;
        let end_col = if self.padded { 0 } else { total % cols };

        let target = display_width(prompt) + cursor_cols;
        let cursor_row = target / cols;
        let cursor_col = target % cols;

        move_rows(&mut out, end_row, cursor_row);
        if cursor_col != end_col || cursor_row != end_row {
            out.push('\r');
            if cursor_col > 0 {
                let _ = write!(out, "\x1b[{cursor_col}C");
            }
        }

        self.end_row = end_row as u16;
        self.cursor_row = cursor_row as u16;
        self.drawn = true;
        out
    }

    /// The escapes that erase the drawn block, leaving the cursor where it
    /// began.
    pub fn erase(&mut self) -> String {
        let mut out = String::new();
        self.rewind(&mut out);
        self.forget();
        out
    }

    /// The escapes that leave the drawn block on screen and move to a fresh
    /// line below it, for after a line is accepted.
    pub fn finish(&mut self) -> String {
        let mut out = String::new();
        if self.drawn {
            move_rows(&mut out, self.cursor_row as usize, self.end_row as usize);
            if self.padded {
                // The last row holds nothing but the padding space.
                out.push_str("\r\x1b[K");
            }
        }
        out.push_str("\r\n");
        self.forget();
        out
    }

    // depth: cursor bookkeeping.

    /// Move to the top-left of the drawn block and erase everything below.
    fn rewind(&self, out: &mut String) {
        if !self.drawn {
            return;
        }
        out.push('\r');
        if self.cursor_row > 0 {
            let _ = write!(out, "\x1b[{}A", self.cursor_row);
        }
        out.push_str("\x1b[J");
    }
}

fn move_rows(out: &mut String, from: usize, to: usize) {
    match from.cmp(&to) {
        std::cmp::Ordering::Greater => {
            let _ = write!(out, "\x1b[{}A", from - to);
        }
        std::cmp::Ordering::Less => {
            let _ = write!(out, "\x1b[{}B", to - from);
        }
        std::cmp::Ordering::Equal => {}
    }
}
