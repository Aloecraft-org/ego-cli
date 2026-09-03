//! Command history: recording, walking, and surviving the process.
//!
//! # Surface
//!
//! Entry points: [`History::new`], [`History::push`], [`History::older`],
//! [`History::newer`], [`History::end_navigation`], [`History::entries`],
//! [`History::clear`], [`History::encode`], [`History::decode`],
//! [`History::load`], [`History::save`].
//!
//! Configurable values: [`DEFAULT_LIMIT`], and the four switches on
//! [`History`] — [`History::set_limit`],
//! [`History::set_dedup_adjacent`], [`History::set_ignore_leading_space`],
//! [`History::set_prefix_search`].
//!
//! Fan-out points: none; this module has one type.
//!
//! # Prefix search
//!
//! With [`prefix_search`](History::set_prefix_search) on (the default),
//! Up walks only the entries starting with what is already left of the
//! cursor. Type `git ` and Up visits your git commands; press Up on an
//! empty line and every entry matches, which is exactly the plain
//! behaviour. It costs nothing to turn off and is the single most useful
//! thing a history does.
//!
//! # Persistence
//!
//! [`load`](History::load) and [`save`](History::save) go through
//! `ego_platform::BlobStore`, so the same two lines persist to a directory
//! natively, to a directory under wasmtime's preopens on WASI, and to
//! IndexedDB in the browser. The wire format is the obvious one: entries
//! separated by newlines, oldest first, UTF-8. An entry can never contain a
//! newline — a line editor cannot produce one — so the format needs no
//! escaping and stays greppable.

use std::collections::VecDeque;
use std::io;

use ego_platform::BlobStore;

pub const DEFAULT_LIMIT: usize = 1000;

/// Recorded lines, newest last, plus the cursor a session walks them with.
#[derive(Clone, Debug)]
pub struct History {
    entries: VecDeque<String>,
    limit: usize,
    dedup_adjacent: bool,
    ignore_leading_space: bool,
    prefix_search: bool,

    // Navigation state. `pos` is None when the editor is showing the line
    // the human is typing rather than a recalled one.
    pos: Option<usize>,
    draft: String,
    prefix: String,
}

impl Default for History {
    fn default() -> Self {
        Self::new()
    }
}

impl History {
    pub fn new() -> Self {
        Self {
            entries: VecDeque::new(),
            limit: DEFAULT_LIMIT,
            dedup_adjacent: true,
            ignore_leading_space: true,
            prefix_search: true,
            pos: None,
            draft: String::new(),
            prefix: String::new(),
        }
    }

    /// Keep at most `limit` entries, dropping the oldest. Zero disables
    /// recording entirely.
    pub fn set_limit(&mut self, limit: usize) {
        self.limit = limit;
        self.trim();
    }

    /// Skip a line identical to the one before it. On by default.
    pub fn set_dedup_adjacent(&mut self, dedup: bool) {
        self.dedup_adjacent = dedup;
    }

    /// Skip a line that starts with a space, the shell convention for "do
    /// not remember this". On by default.
    pub fn set_ignore_leading_space(&mut self, ignore: bool) {
        self.ignore_leading_space = ignore;
    }

    /// Restrict Up/Down to entries starting with the text left of the
    /// cursor. On by default; see the module docs.
    pub fn set_prefix_search(&mut self, enabled: bool) {
        self.prefix_search = enabled;
    }

    /// Record an accepted line, unless a switch above says not to.
    pub fn push(&mut self, line: &str) {
        self.end_navigation();
        if self.limit == 0 || line.trim().is_empty() {
            return;
        }
        if self.ignore_leading_space && line.starts_with(' ') {
            return;
        }
        if self.dedup_adjacent && self.entries.back().map(String::as_str) == Some(line) {
            return;
        }
        self.entries.push_back(line.to_string());
        self.trim();
    }

    /// The next matching entry going back, or `None` at the oldest one.
    ///
    /// The first call in a walk captures `current` as the draft to come back
    /// to and `current[..cursor]` as the prefix to match.
    pub fn older(&mut self, current: &str, cursor: usize) -> Option<String> {
        if self.pos.is_none() {
            self.draft = current.to_string();
            self.prefix = if self.prefix_search {
                // The caller's cursor is a byte index and need not land on a
                // `char` boundary; slicing off one would panic.
                let mut end = cursor.min(current.len());
                while end > 0 && !current.is_char_boundary(end) {
                    end -= 1;
                }
                current[..end].to_string()
            } else {
                String::new()
            };
        }

        let mut index = self.pos.unwrap_or(self.entries.len());
        while index > 0 {
            index -= 1;
            if self.entries[index].starts_with(&self.prefix) {
                self.pos = Some(index);
                return Some(self.entries[index].clone());
            }
        }
        None
    }

    /// The next matching entry coming forward, or the draft once the walk
    /// reaches the end.
    ///
    /// `None` when no walk is in progress.
    pub fn newer(&mut self) -> Option<String> {
        let current = self.pos?;
        for index in (current + 1)..self.entries.len() {
            if self.entries[index].starts_with(&self.prefix) {
                self.pos = Some(index);
                return Some(self.entries[index].clone());
            }
        }
        self.pos = None;
        self.prefix.clear();
        Some(std::mem::take(&mut self.draft))
    }

    /// Forget where a walk had got to; the next [`older`](Self::older) starts
    /// from the newest entry. Called for you whenever the line is edited.
    pub fn end_navigation(&mut self) {
        self.pos = None;
        self.draft.clear();
        self.prefix.clear();
    }

    /// Whether a walk is in progress.
    pub fn navigating(&self) -> bool {
        self.pos.is_some()
    }

    /// Every entry, oldest first.
    pub fn entries(&self) -> impl Iterator<Item = &str> {
        self.entries.iter().map(String::as_str)
    }

    pub fn len(&self) -> usize {
        self.entries.len()
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    pub fn clear(&mut self) {
        self.entries.clear();
        self.end_navigation();
    }

    // depth: persistence.

    /// The entries as newline-separated UTF-8, oldest first.
    pub fn encode(&self) -> Vec<u8> {
        let mut out = String::new();
        for entry in &self.entries {
            out.push_str(entry);
            out.push('\n');
        }
        out.into_bytes()
    }

    /// Replace the entries with those in `bytes`, keeping every switch.
    ///
    /// Invalid UTF-8 is replaced rather than rejected: a truncated history
    /// file is worth less than a session that refuses to start.
    pub fn decode(&mut self, bytes: &[u8]) {
        let text = String::from_utf8_lossy(bytes);
        self.entries = text
            .lines()
            .filter(|line| !line.trim().is_empty())
            .map(str::to_string)
            .collect();
        self.trim();
        self.end_navigation();
    }

    /// Load the entries stored under `key`, leaving them untouched if there
    /// is nothing there yet.
    pub async fn load(&mut self, store: &dyn BlobStore, key: &str) -> io::Result<()> {
        if let Some(bytes) = store.get(key).await? {
            self.decode(&bytes);
        }
        Ok(())
    }

    /// Store the entries under `key`, replacing atomically.
    pub async fn save(&self, store: &dyn BlobStore, key: &str) -> io::Result<()> {
        store.put(key, self.encode()).await
    }

    fn trim(&mut self) {
        while self.entries.len() > self.limit {
            self.entries.pop_front();
        }
        // Dropping the oldest entries invalidates any index into them.
        if self.pos.is_some() {
            self.end_navigation();
        }
    }
}
