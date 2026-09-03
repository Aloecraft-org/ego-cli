//! The prompt: a terminal, an editor, and the loop between them.
//!
//! # Surface
//!
//! Entry points: [`Session::new`], [`Session::read_line`],
//! [`Session::print`]; the configuration [`Session::set_prompt`],
//! [`Session::set_completer`], [`Session::set_highlighter`],
//! [`Session::set_completion_list_limit`]; the accessors
//! [`Session::editor`], [`Session::editor_mut`], [`Session::history`],
//! [`Session::history_mut`], [`Session::keymap`], [`Session::keymap_mut`],
//! [`Session::terminal`], [`Session::terminal_mut`],
//! [`Session::capabilities`]. [`ReadOutcome`] is what a read produces.
//!
//! Configurable values: [`DEFAULT_PROMPT`],
//! [`DEFAULT_COMPLETION_LIST_LIMIT`].
//!
//! Fan-out points: [`Session::read_line`] picks one of two loops from
//! [`crate::term::Capabilities::raw_mode`] — `edit_loop` where there are
//! keys to edit with, `read_line_cooked` where input arrives a line at a
//! time. `edit_loop`'s match on [`crate::editor::EditOutcome`] is the only
//! place an editing outcome turns into terminal output.

use std::borrow::Cow;

use crate::Result;
use crate::editor::{EditOutcome, LineEditor};
use crate::extend::{Completer, Highlighter, NoCompleter, NoHighlighter};
use crate::history::History;
use crate::keymap::Keymap;
use crate::render::Renderer;
use crate::style::{self, display_width};
use crate::term::{Capabilities, Event, Terminal};

pub const DEFAULT_PROMPT: &str = "> ";

/// How many completion candidates to print before saying how many are left.
pub const DEFAULT_COMPLETION_LIST_LIMIT: usize = 200;

/// The result of asking for one line.
#[derive(Clone, PartialEq, Eq, Debug)]
pub enum ReadOutcome {
    /// The line, without its newline. Already recorded in history.
    Line(String),
    /// Ctrl+C: the line was abandoned, the session is still good.
    Interrupted,
    /// End of input: Ctrl+D on an empty line, or the terminal closed.
    Eof,
}

/// A prompt bound to a terminal.
///
/// Generic over the backend so each one's futures stay its own; see
/// [`crate::term::Terminal`].
pub struct Session<T: Terminal> {
    term: T,
    editor: LineEditor,
    renderer: Renderer,
    keymap: Keymap,
    completer: Box<dyn Completer>,
    highlighter: Box<dyn Highlighter>,
    prompt: String,
    completion_list_limit: usize,
}

impl<T: Terminal> Session<T> {
    pub fn new(term: T) -> Self {
        let cols = term.size().cols;
        Self {
            term,
            editor: LineEditor::new(),
            renderer: Renderer::new(cols),
            keymap: Keymap::new(),
            completer: Box::new(NoCompleter),
            highlighter: Box::new(NoHighlighter),
            prompt: DEFAULT_PROMPT.to_string(),
            completion_list_limit: DEFAULT_COMPLETION_LIST_LIMIT,
        }
    }

    pub fn prompt(&self) -> &str {
        &self.prompt
    }

    /// Set the prompt. It may carry SGR escapes and must be a single line.
    pub fn set_prompt(&mut self, prompt: impl Into<String>) {
        self.prompt = prompt.into();
    }

    pub fn set_completer(&mut self, completer: impl Completer + 'static) {
        self.completer = Box::new(completer);
    }

    pub fn set_highlighter(&mut self, highlighter: impl Highlighter + 'static) {
        self.highlighter = Box::new(highlighter);
    }

    pub fn set_completion_list_limit(&mut self, limit: usize) {
        self.completion_list_limit = limit;
    }

    pub fn editor(&self) -> &LineEditor {
        &self.editor
    }

    pub fn editor_mut(&mut self) -> &mut LineEditor {
        &mut self.editor
    }

    pub fn history(&self) -> &History {
        self.editor.history()
    }

    pub fn history_mut(&mut self) -> &mut History {
        self.editor.history_mut()
    }

    pub fn keymap(&self) -> &Keymap {
        &self.keymap
    }

    pub fn keymap_mut(&mut self) -> &mut Keymap {
        &mut self.keymap
    }

    pub fn terminal(&self) -> &T {
        &self.term
    }

    pub fn terminal_mut(&mut self) -> &mut T {
        &mut self.term
    }

    pub fn capabilities(&self) -> Capabilities {
        self.term.capabilities()
    }

    /// Write `text` to the terminal and flush.
    ///
    /// For the host's own output between prompts. A lone `\n` is turned
    /// into `\r\n` on a terminal without a line discipline of its own —
    /// xterm.js would otherwise start the next line halfway across the
    /// screen. Text that already says `\r\n` is left as it is.
    pub async fn print(&mut self, text: &str) -> Result<()> {
        let text = if !self.term.capabilities().line_discipline && text.contains('\n') {
            Cow::Owned(text.replace("\r\n", "\n").replace('\n', "\r\n"))
        } else {
            Cow::Borrowed(text)
        };
        self.term.write(&text).await?;
        self.term.flush().await
    }

    /// Read one line.
    ///
    /// Editing where the terminal delivers keys, a plain read where it
    /// delivers lines. Either way the accepted line is recorded in history
    /// and the cursor is left on a fresh line below the prompt.
    pub async fn read_line(&mut self) -> Result<ReadOutcome> {
        if self.term.capabilities().raw_mode {
            self.term.set_raw(true)?;
            let outcome = self.edit_loop().await;
            // Restore before returning, including on the error path: a
            // terminal left in raw mode outlives this process.
            let _ = self.term.set_raw(false);
            let _ = self.term.flush().await;
            outcome
        } else {
            self.read_line_cooked().await
        }
    }

    // depth: the two loops.

    async fn read_line_cooked(&mut self) -> Result<ReadOutcome> {
        let prompt = self.prompt_text();
        self.term.write(&prompt).await?;
        self.term.flush().await?;

        loop {
            match self.term.next_event().await? {
                Event::Line(line) => {
                    self.editor.history_mut().push(&line);
                    return Ok(ReadOutcome::Line(line));
                }
                Event::Eof => return Ok(ReadOutcome::Eof),
                Event::Resize(size) => self.renderer.set_cols(size.cols),
                // A line-at-a-time terminal has no keys to report, but a
                // backend that grows them should not be silently ignored.
                Event::Key(_) => {}
            }
        }
    }

    async fn edit_loop(&mut self) -> Result<ReadOutcome> {
        self.renderer.set_cols(self.term.size().cols);
        self.renderer.forget();
        self.draw("").await?;

        loop {
            match self.term.next_event().await? {
                Event::Resize(size) => {
                    self.renderer.set_cols(size.cols);
                    self.draw("").await?;
                }

                // The terminal went away mid-line. Whatever was typed is
                // lost with it; say end of input rather than pretend. The
                // write below is a courtesy to a terminal that is still
                // readable by a human, and is allowed to fail.
                Event::Eof => {
                    let _ = self.finish().await;
                    return Ok(ReadOutcome::Eof);
                }

                // Only from a backend that reports raw mode and then sends
                // lines anyway. Take it at face value.
                Event::Line(line) => {
                    self.editor.history_mut().push(&line);
                    return Ok(ReadOutcome::Line(line));
                }

                Event::Key(key) => {
                    let action = self.keymap.lookup(key);
                    match self.editor.apply(action) {
                        EditOutcome::Continue => self.draw("").await?,
                        EditOutcome::Ignored => {}

                        EditOutcome::Accept(line) => {
                            self.finish().await?;
                            return Ok(ReadOutcome::Line(line));
                        }

                        EditOutcome::Complete => self.complete().await?,

                        EditOutcome::ClearScreen => {
                            self.term.write("\x1b[H\x1b[2J").await?;
                            self.renderer.forget();
                            self.draw("").await?;
                        }

                        EditOutcome::Interrupt => {
                            // Leave the abandoned line on screen, marked,
                            // the way every shell does.
                            self.draw("^C").await?;
                            self.finish().await?;
                            self.editor.take_buffer();
                            self.editor.history_mut().end_navigation();
                            return Ok(ReadOutcome::Interrupted);
                        }

                        EditOutcome::Eof => {
                            self.finish().await?;
                            return Ok(ReadOutcome::Eof);
                        }
                    }
                }
            }
        }
    }

    // depth: drawing.

    /// Redraw the prompt and line, with `suffix` appended after the line and
    /// the cursor placed after it when it is non-empty.
    async fn draw(&mut self, suffix: &str) -> Result<()> {
        let prompt = self.prompt_text();
        let buffer = self.editor.buffer();
        let cursor = self.editor.cursor();

        let mut line = self.highlighter.highlight(buffer).into_owned();
        let cursor_cols = if suffix.is_empty() {
            display_width(&buffer[..cursor])
        } else {
            let width = display_width(&line) + display_width(suffix);
            line.push_str(suffix);
            width
        };

        let frame = self.renderer.frame(&prompt, &line, cursor_cols);
        self.term.write(&frame).await?;
        self.term.flush().await
    }

    /// Leave the drawn line on screen and move below it.
    async fn finish(&mut self) -> Result<()> {
        let tail = self.renderer.finish();
        self.term.write(&tail).await?;
        self.term.flush().await
    }

    fn prompt_text(&self) -> String {
        if self.term.capabilities().ansi {
            self.highlighter.highlight_prompt(&self.prompt).into_owned()
        } else {
            style::strip(&self.prompt)
        }
    }

    // depth: completion.

    async fn complete(&mut self) -> Result<()> {
        let completion = self
            .completer
            .complete(self.editor.buffer(), self.editor.cursor());
        if completion.is_empty() {
            return Ok(());
        }

        if completion.candidates.len() == 1 {
            self.editor
                .replace_range(completion.range(), &completion.candidates[0]);
            return self.draw("").await;
        }

        // More than one: insert as much as they agree on, and if that adds
        // nothing, show what the choices are.
        let prefix = completion.common_prefix();
        if prefix.len() > completion.end.saturating_sub(completion.start) {
            self.editor.replace_range(completion.range(), &prefix);
            return self.draw("").await;
        }

        let listing = columnize(
            &completion.candidates,
            self.renderer.cols(),
            self.completion_list_limit,
        );
        let erase = self.renderer.erase();
        self.term.write(&erase).await?;
        self.term.write(&listing).await?;
        self.draw("").await
    }
}

impl<T: Terminal> Drop for Session<T> {
    fn drop(&mut self) {
        // A session dropped mid-line — by a panic, or by a host that gave
        // up — must not leave the terminal raw.
        let _ = self.term.set_raw(false);
    }
}

// depth: candidate listing.

/// Lay `items` out in columns that fit `cols`, ending with a newline.
///
/// Row-major, like a shell's completion list. Anything past `limit` is
/// summarized rather than printed, so a completer that offers everything on
/// the filesystem cannot scroll the session away.
fn columnize(items: &[String], cols: u16, limit: usize) -> String {
    let shown = items.len().min(limit.max(1));
    let widest = items[..shown]
        .iter()
        .map(|item| display_width(item))
        .max()
        .unwrap_or(0);
    let column = widest + 2;
    let per_row = ((cols as usize) / column.max(1)).max(1);

    let mut out = String::new();
    for (index, item) in items[..shown].iter().enumerate() {
        out.push_str(item);
        if (index + 1) % per_row == 0 || index + 1 == shown {
            out.push_str("\r\n");
        } else {
            for _ in display_width(item)..column {
                out.push(' ');
            }
        }
    }
    if items.len() > shown {
        out.push_str(&format!("... and {} more\r\n", items.len() - shown));
    }
    out
}
