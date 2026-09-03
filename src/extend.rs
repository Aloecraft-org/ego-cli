//! The two places a host plugs its own behaviour in.
//!
//! # Surface
//!
//! Entry points: [`Completer`] and [`Completion`]; [`Highlighter`]; the
//! do-nothing defaults [`NoCompleter`] and [`NoHighlighter`]; the ready-made
//! [`WordCompleter`].
//!
//! Configurable values: none.
//!
//! Fan-out points: [`Completer`] and [`Highlighter`] are the extension
//! points; `session::Session` holds one boxed implementation of each.
//!
//! Both traits are synchronous and take `&self`. That is deliberate: a
//! completer that has to await something (a network round trip, a C library
//! behind `diluvium-sys`) should look it up before the keystroke, not
//! during it — a prompt that stalls mid-Tab is worse than one that offers
//! nothing.

use std::borrow::Cow;
use std::ops::Range;

/// What could go where.
///
/// `start..end` is the byte range of the line the candidates would replace,
/// which is normally the token under the cursor. A completer that returns a
/// range outside the line, or off a `char` boundary, is clamped rather than
/// trusted.
#[derive(Clone, PartialEq, Eq, Debug, Default)]
pub struct Completion {
    pub start: usize,
    pub end: usize,
    pub candidates: Vec<String>,
}

impl Completion {
    /// Nothing to offer.
    pub fn none() -> Self {
        Self::default()
    }

    pub fn new(range: Range<usize>, candidates: Vec<String>) -> Self {
        Self {
            start: range.start,
            end: range.end,
            candidates,
        }
    }

    pub fn is_empty(&self) -> bool {
        self.candidates.is_empty()
    }

    pub fn range(&self) -> Range<usize> {
        self.start..self.end
    }

    /// The longest prefix every candidate shares, on a `char` boundary.
    ///
    /// This is what Tab inserts when there is more than one candidate: as
    /// much as is unambiguous, then the list.
    pub fn common_prefix(&self) -> String {
        let mut candidates = self.candidates.iter();
        let Some(first) = candidates.next() else {
            return String::new();
        };
        let mut length = first.len();
        for candidate in candidates {
            length = first
                .char_indices()
                .zip(candidate.char_indices())
                .take_while(|((_, a), (_, b))| a == b)
                .map(|((index, c), _)| index + c.len_utf8())
                .last()
                .unwrap_or(0)
                .min(length);
        }
        first[..length].to_string()
    }
}

/// Answers "what could follow what has been typed".
pub trait Completer {
    fn complete(&self, line: &str, cursor: usize) -> Completion;
}

/// Offers nothing. The default.
#[derive(Clone, Copy, Debug, Default)]
pub struct NoCompleter;

impl Completer for NoCompleter {
    fn complete(&self, _line: &str, _cursor: usize) -> Completion {
        Completion::none()
    }
}

/// Completes the whitespace-delimited token under the cursor from a fixed
/// list.
///
/// Enough for command names and subcommands, and a working example of the
/// trait for anything that needs more.
#[derive(Clone, Debug, Default)]
pub struct WordCompleter {
    words: Vec<String>,
}

impl WordCompleter {
    pub fn new<I, S>(words: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        Self {
            words: words.into_iter().map(Into::into).collect(),
        }
    }

    pub fn words(&self) -> &[String] {
        &self.words
    }
}

impl Completer for WordCompleter {
    fn complete(&self, line: &str, cursor: usize) -> Completion {
        let cursor = cursor.min(line.len());
        let start = line[..cursor]
            .rfind(char::is_whitespace)
            .map(|index| index + 1)
            .unwrap_or(0);
        let token = &line[start..cursor];
        let candidates: Vec<String> = self
            .words
            .iter()
            .filter(|word| word.starts_with(token))
            .cloned()
            .collect();
        Completion::new(start..cursor, candidates)
    }
}

/// Colours the line as it is typed.
///
/// An implementation returns the same printable characters with SGR escapes
/// added — `crate::style` has the pieces — and nothing else. Adding or
/// removing a printable character here would put the cursor in the wrong
/// place, because the cursor is measured against the line the editor holds.
pub trait Highlighter {
    fn highlight<'l>(&self, line: &'l str) -> Cow<'l, str> {
        Cow::Borrowed(line)
    }

    fn highlight_prompt<'p>(&self, prompt: &'p str) -> Cow<'p, str> {
        Cow::Borrowed(prompt)
    }
}

/// Leaves the line alone. The default.
#[derive(Clone, Copy, Debug, Default)]
pub struct NoHighlighter;

impl Highlighter for NoHighlighter {}
