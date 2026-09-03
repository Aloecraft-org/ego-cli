//! The line being edited: buffer, cursor, undo stack, history walk.
//!
//! # Surface
//!
//! Entry points: [`LineEditor::new`], [`LineEditor::apply`] — the one
//! dispatch over [`Action`] — plus the accessors [`LineEditor::buffer`],
//! [`LineEditor::cursor`], [`LineEditor::set_line`],
//! [`LineEditor::replace_range`], [`LineEditor::clear`],
//! [`LineEditor::take_buffer`], [`LineEditor::history`],
//! [`LineEditor::history_mut`]. [`EditOutcome`] is what `apply` reports.
//!
//! Configurable values: [`DEFAULT_UNDO_LIMIT`],
//! [`LineEditor::set_undo_limit`].
//!
//! Fan-out points: [`EditOutcome`] is the closed set of things the session
//! has to react to; `EditRun` the closed set of undo-coalescing groups.
//!
//! Everything here is pure: no terminal, no async, no platform. A host can
//! drive a `LineEditor` from any source of [`Action`]s and read the buffer
//! back, which is how this module is tested on all three targets and how a
//! caller with its own event loop can reuse it.
//!
//! # Cursor invariant
//!
//! `cursor` is a byte index into `buffer` and is always on a `char`
//! boundary. Motion moves it by grapheme cluster, so an emoji with a
//! modifier or a combining accent is one Left press, not two.
//!
//! # Undo granularity
//!
//! A run of insertions is one undo step, broken at whitespace, so undo
//! takes back a word rather than a letter; a run of deletions likewise.
//! `ego_shell` snapshotted every keystroke, which made Ctrl+Z on a typed
//! line a chore. Anything else — a kill, a completion, recalling history —
//! is its own step.

use std::collections::VecDeque;
use std::ops::Range;

use unicode_segmentation::UnicodeSegmentation;

use crate::history::History;
use crate::keymap::Action;

pub const DEFAULT_UNDO_LIMIT: usize = 128;

/// What the session must do about an applied [`Action`].
#[derive(Clone, PartialEq, Eq, Debug)]
pub enum EditOutcome {
    /// State changed; redraw.
    Continue,
    /// Nothing happened; no redraw needed.
    Ignored,
    /// The line is finished. The editor is already reset and the line
    /// recorded in history.
    Accept(String),
    /// Run the completer against the current buffer and cursor.
    Complete,
    /// Redraw from a cleared screen.
    ClearScreen,
    /// Ctrl+C: abandon the line.
    Interrupt,
    /// Ctrl+D on an empty line: end of input.
    Eof,
}

/// The groups a run of edits coalesces into for undo.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum EditRun {
    Insert,
    DeleteBack,
    DeleteForward,
}

#[derive(Clone, PartialEq, Eq, Debug)]
struct Snapshot {
    buffer: String,
    cursor: usize,
}

/// One line of input, mid-edit.
#[derive(Debug)]
pub struct LineEditor {
    buffer: String,
    cursor: usize,

    undo: VecDeque<Snapshot>,
    redo: Vec<Snapshot>,
    undo_limit: usize,
    /// The run the last edit belonged to, or `None` if the next edit must
    /// start a new undo step.
    run: Option<EditRun>,

    history: History,
}

impl Default for LineEditor {
    fn default() -> Self {
        Self::new()
    }
}

impl LineEditor {
    pub fn new() -> Self {
        Self {
            buffer: String::new(),
            cursor: 0,
            undo: VecDeque::new(),
            redo: Vec::new(),
            undo_limit: DEFAULT_UNDO_LIMIT,
            run: None,
            history: History::new(),
        }
    }

    pub fn buffer(&self) -> &str {
        &self.buffer
    }

    /// The cursor as a byte index into [`buffer`](Self::buffer), always on a
    /// `char` boundary.
    pub fn cursor(&self) -> usize {
        self.cursor
    }

    pub fn is_empty(&self) -> bool {
        self.buffer.is_empty()
    }

    pub fn history(&self) -> &History {
        &self.history
    }

    pub fn history_mut(&mut self) -> &mut History {
        &mut self.history
    }

    /// Keep at most `limit` undo steps. Zero disables undo.
    pub fn set_undo_limit(&mut self, limit: usize) {
        self.undo_limit = limit;
        self.trim_undo();
    }

    /// Replace the whole line, cursor to the end. One undo step.
    pub fn set_line(&mut self, text: &str) {
        self.checkpoint(None);
        self.buffer.clear();
        self.buffer.push_str(text);
        self.cursor = self.buffer.len();
    }

    /// Replace `range` with `text`, cursor to the end of what was inserted.
    /// One undo step.
    ///
    /// Used for completion. The range is clamped to the buffer and widened
    /// to `char` boundaries, so a completer that miscounts cannot panic the
    /// session.
    pub fn replace_range(&mut self, range: Range<usize>, text: &str) {
        let start = self.floor_boundary(range.start.min(self.buffer.len()));
        let end = self
            .ceil_boundary(range.end.min(self.buffer.len()))
            .max(start);
        self.checkpoint(None);
        self.buffer.replace_range(start..end, text);
        self.cursor = start + text.len();
    }

    /// Empty the line, keeping history and the undo stack.
    pub fn clear(&mut self) {
        self.checkpoint(None);
        self.buffer.clear();
        self.cursor = 0;
    }

    /// Take the line, leaving the editor empty and its undo stack fresh.
    /// Does not record history; [`Action::Accept`] does that.
    pub fn take_buffer(&mut self) -> String {
        self.cursor = 0;
        self.undo.clear();
        self.redo.clear();
        self.run = None;
        std::mem::take(&mut self.buffer)
    }

    /// Apply one action. The single dispatch point for editing.
    pub fn apply(&mut self, action: Action) -> EditOutcome {
        match action {
            Action::Insert(c) => {
                self.insert(c);
                EditOutcome::Continue
            }

            Action::Accept => {
                let line = self.take_buffer();
                self.history.push(&line);
                EditOutcome::Accept(line)
            }

            Action::Complete => EditOutcome::Complete,

            // Motion. Each breaks the undo run, so a word typed either side
            // of a cursor move is two undo steps.
            Action::MoveLeft => self.move_to(self.prev_grapheme(self.cursor)),
            Action::MoveRight => self.move_to(self.next_grapheme(self.cursor)),
            Action::MoveWordLeft => self.move_to(self.prev_word(self.cursor)),
            Action::MoveWordRight => self.move_to(self.next_word(self.cursor)),
            Action::MoveStart => self.move_to(0),
            Action::MoveEnd => self.move_to(self.buffer.len()),

            Action::HistoryPrev => {
                self.run = None;
                match self.history.older(&self.buffer, self.cursor) {
                    Some(line) => {
                        self.set_recalled(&line);
                        EditOutcome::Continue
                    }
                    None => EditOutcome::Ignored,
                }
            }
            Action::HistoryNext => {
                self.run = None;
                match self.history.newer() {
                    Some(line) => {
                        self.set_recalled(&line);
                        EditOutcome::Continue
                    }
                    None => EditOutcome::Ignored,
                }
            }

            Action::DeleteBack => self.delete(EditRun::DeleteBack, self.prev_grapheme(self.cursor)),
            Action::DeleteForward => {
                self.delete(EditRun::DeleteForward, self.next_grapheme(self.cursor))
            }
            Action::DeleteWordBack => self.delete(EditRun::DeleteBack, self.prev_word(self.cursor)),
            Action::DeleteWordForward => {
                self.delete(EditRun::DeleteForward, self.next_word(self.cursor))
            }
            Action::KillToStart => self.kill(0),
            Action::KillToEnd => self.kill(self.buffer.len()),

            Action::Undo => {
                if self.undo_step() {
                    EditOutcome::Continue
                } else {
                    EditOutcome::Ignored
                }
            }
            Action::Redo => {
                if self.redo_step() {
                    EditOutcome::Continue
                } else {
                    EditOutcome::Ignored
                }
            }

            Action::Cancel => {
                if self.buffer.is_empty() {
                    EditOutcome::Ignored
                } else {
                    self.begin_edit(None);
                    self.buffer.clear();
                    self.cursor = 0;
                    EditOutcome::Continue
                }
            }

            Action::ClearScreen => EditOutcome::ClearScreen,
            Action::Interrupt => EditOutcome::Interrupt,

            // Readline's rule: end of input only on an empty line, so
            // Ctrl+D mid-line is the forward delete it is everywhere else.
            Action::Eof => {
                if self.buffer.is_empty() {
                    EditOutcome::Eof
                } else {
                    self.delete(EditRun::DeleteForward, self.next_grapheme(self.cursor))
                }
            }

            Action::Ignore => EditOutcome::Ignored,
        }
    }

    // depth: editing primitives.

    fn insert(&mut self, c: char) {
        self.begin_edit(Some(EditRun::Insert));
        self.buffer.insert(self.cursor, c);
        self.cursor += c.len_utf8();
        // Break the run at whitespace so undo takes back a word at a time.
        if c.is_whitespace() {
            self.run = None;
        }
    }

    /// Delete between the cursor and `target`, in whichever direction that
    /// is, and leave the cursor at the lower end.
    fn delete(&mut self, run: EditRun, target: usize) -> EditOutcome {
        if target == self.cursor {
            return EditOutcome::Ignored;
        }
        self.begin_edit(Some(run));
        let (start, end) = if target < self.cursor {
            (target, self.cursor)
        } else {
            (self.cursor, target)
        };
        self.buffer.replace_range(start..end, "");
        self.cursor = start;
        EditOutcome::Continue
    }

    /// Like [`delete`](Self::delete) but never coalesces: two kills are two
    /// undo steps.
    fn kill(&mut self, target: usize) -> EditOutcome {
        if target == self.cursor {
            return EditOutcome::Ignored;
        }
        self.begin_edit(None);
        let (start, end) = if target < self.cursor {
            (target, self.cursor)
        } else {
            (self.cursor, target)
        };
        self.buffer.replace_range(start..end, "");
        self.cursor = start;
        EditOutcome::Continue
    }

    fn move_to(&mut self, target: usize) -> EditOutcome {
        self.run = None;
        if target == self.cursor {
            return EditOutcome::Ignored;
        }
        self.cursor = target;
        EditOutcome::Continue
    }

    /// Install a line recalled from history without ending the walk that
    /// produced it.
    fn set_recalled(&mut self, line: &str) {
        self.checkpoint(None);
        self.buffer.clear();
        self.buffer.push_str(line);
        self.cursor = self.buffer.len();
    }

    /// Snapshot for undo, and end any history walk: editing the recalled
    /// line means the walk is over.
    fn begin_edit(&mut self, run: Option<EditRun>) {
        self.checkpoint(run);
        self.history.end_navigation();
    }

    // depth: undo stack.

    fn checkpoint(&mut self, run: Option<EditRun>) {
        if run.is_some() && self.run == run {
            return; // same run: the snapshot from its first edit still stands
        }
        self.run = run;
        if self.undo_limit == 0 {
            return;
        }
        self.undo.push_back(Snapshot {
            buffer: self.buffer.clone(),
            cursor: self.cursor,
        });
        self.redo.clear();
        self.trim_undo();
    }

    fn undo_step(&mut self) -> bool {
        let Some(previous) = self.undo.pop_back() else {
            return false;
        };
        self.redo.push(Snapshot {
            buffer: std::mem::replace(&mut self.buffer, previous.buffer),
            cursor: self.cursor,
        });
        self.cursor = previous.cursor;
        self.run = None;
        self.history.end_navigation();
        true
    }

    fn redo_step(&mut self) -> bool {
        let Some(next) = self.redo.pop() else {
            return false;
        };
        self.undo.push_back(Snapshot {
            buffer: std::mem::replace(&mut self.buffer, next.buffer),
            cursor: self.cursor,
        });
        self.cursor = next.cursor;
        self.run = None;
        self.history.end_navigation();
        true
    }

    fn trim_undo(&mut self) {
        while self.undo.len() > self.undo_limit {
            self.undo.pop_front();
        }
    }

    // depth: boundaries. Byte indices throughout; lines are short enough
    // that scanning from the cursor beats keeping a parallel index.

    fn prev_grapheme(&self, at: usize) -> usize {
        self.buffer[..at]
            .grapheme_indices(true)
            .next_back()
            .map(|(index, _)| index)
            .unwrap_or(0)
    }

    fn next_grapheme(&self, at: usize) -> usize {
        self.buffer[at..]
            .graphemes(true)
            .next()
            .map(|g| at + g.len())
            .unwrap_or(at)
    }

    /// The start of the word at or before `at`: back over whitespace, then
    /// back over the word. A "word" is a run of non-whitespace, which is
    /// what a shell prompt means by it.
    fn prev_word(&self, at: usize) -> usize {
        let mut index = at;
        while index > 0 {
            let previous = self.prev_grapheme(index);
            if !self.buffer[previous..index].starts_with(char::is_whitespace) {
                break;
            }
            index = previous;
        }
        while index > 0 {
            let previous = self.prev_grapheme(index);
            if self.buffer[previous..index].starts_with(char::is_whitespace) {
                break;
            }
            index = previous;
        }
        index
    }

    /// Past the end of the word at or after `at`, and past the whitespace
    /// that follows it.
    fn next_word(&self, at: usize) -> usize {
        let length = self.buffer.len();
        let mut index = at;
        while index < length {
            let next = self.next_grapheme(index);
            if self.buffer[index..next].starts_with(char::is_whitespace) {
                break;
            }
            index = next;
        }
        while index < length {
            let next = self.next_grapheme(index);
            if !self.buffer[index..next].starts_with(char::is_whitespace) {
                break;
            }
            index = next;
        }
        index
    }

    fn floor_boundary(&self, mut index: usize) -> usize {
        while index > 0 && !self.buffer.is_char_boundary(index) {
            index -= 1;
        }
        index
    }

    fn ceil_boundary(&self, mut index: usize) -> usize {
        while index < self.buffer.len() && !self.buffer.is_char_boundary(index) {
            index += 1;
        }
        index
    }
}
