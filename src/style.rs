//! ANSI text: measuring it, and the small palette used to produce it.
//!
//! # Surface
//!
//! Entry points: [`display_width`], [`strip`], [`paint`], [`fg`],
//! [`Color`], and the bare SGR constants [`RESET`], [`BOLD`], [`DIM`],
//! [`ITALIC`], [`UNDERLINE`], [`REVERSE`].
//!
//! Configurable values: none.
//!
//! Fan-out points: [`Color`] is the closed set of colors [`fg`] knows.
//!
//! A [`crate::extend::Highlighter`] returns a line with SGR escapes already
//! in it, so every width the renderer computes has to ignore those escapes.
//! Both halves of that bargain live here: the helpers a highlighter writes
//! with, and the measurement that stays honest in their presence.

use unicode_width::UnicodeWidthChar;

pub const RESET: &str = "\x1b[0m";
pub const BOLD: &str = "\x1b[1m";
pub const DIM: &str = "\x1b[2m";
pub const ITALIC: &str = "\x1b[3m";
pub const UNDERLINE: &str = "\x1b[4m";
pub const REVERSE: &str = "\x1b[7m";

/// The eight ANSI colors, their bright variants, and "whatever the terminal
/// was using".
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Color {
    Black,
    Red,
    Green,
    Yellow,
    Blue,
    Magenta,
    Cyan,
    White,
    BrightBlack,
    BrightRed,
    BrightGreen,
    BrightYellow,
    BrightBlue,
    BrightMagenta,
    BrightCyan,
    BrightWhite,
    Default,
}

/// The SGR sequence that sets `color` as the foreground.
pub fn fg(color: Color) -> &'static str {
    match color {
        Color::Black => "\x1b[30m",
        Color::Red => "\x1b[31m",
        Color::Green => "\x1b[32m",
        Color::Yellow => "\x1b[33m",
        Color::Blue => "\x1b[34m",
        Color::Magenta => "\x1b[35m",
        Color::Cyan => "\x1b[36m",
        Color::White => "\x1b[37m",
        Color::BrightBlack => "\x1b[90m",
        Color::BrightRed => "\x1b[91m",
        Color::BrightGreen => "\x1b[92m",
        Color::BrightYellow => "\x1b[93m",
        Color::BrightBlue => "\x1b[94m",
        Color::BrightMagenta => "\x1b[95m",
        Color::BrightCyan => "\x1b[96m",
        Color::BrightWhite => "\x1b[97m",
        Color::Default => "\x1b[39m",
    }
}

/// `text` wrapped in `color` and a reset.
pub fn paint(text: &str, color: Color) -> String {
    let mut out = String::with_capacity(text.len() + 10);
    out.push_str(fg(color));
    out.push_str(text);
    out.push_str(fg(Color::Default));
    out
}

/// The number of terminal cells `s` occupies, ignoring any escape sequences
/// in it.
///
/// Control characters count as zero: a line editor never draws them, and a
/// highlighter has no business emitting them.
pub fn display_width(s: &str) -> usize {
    let mut width = 0;
    for chunk in Segments::new(s) {
        if let Segment::Text(t) = chunk {
            width += t.chars().filter_map(UnicodeWidthChar::width).sum::<usize>();
        }
    }
    width
}

/// `s` with every escape sequence removed.
pub fn strip(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for chunk in Segments::new(s) {
        if let Segment::Text(t) = chunk {
            out.push_str(t);
        }
    }
    out
}

// depth: escape-sequence segmentation, shared by display_width and strip.

enum Segment<'a> {
    Text(&'a str),
    Escape,
}

/// Splits a string into printable runs and escape sequences.
///
/// Recognizes the two forms a highlighter can plausibly emit: CSI
/// (`ESC [ params final`) and the two-character escapes (`ESC` followed by a
/// single byte). An unterminated sequence at the end of the string swallows
/// the rest, which is the safe direction to be wrong: an unfinished escape
/// draws nothing either.
struct Segments<'a> {
    rest: &'a str,
}

impl<'a> Segments<'a> {
    fn new(s: &'a str) -> Self {
        Self { rest: s }
    }
}

impl<'a> Iterator for Segments<'a> {
    type Item = Segment<'a>;

    fn next(&mut self) -> Option<Segment<'a>> {
        if self.rest.is_empty() {
            return None;
        }
        if let Some(tail) = self.rest.strip_prefix('\x1b') {
            let consumed = match tail.as_bytes().first() {
                // CSI: parameters and intermediates, then a final byte.
                Some(b'[') => tail[1..]
                    .bytes()
                    .position(|b| (0x40..=0x7e).contains(&b))
                    .map(|i| i + 3)
                    .unwrap_or(self.rest.len()),
                // Any other two-character escape.
                Some(_) => 1 + tail.chars().next().map(char::len_utf8).unwrap_or(0),
                None => 1,
            };
            self.rest = &self.rest[consumed.min(self.rest.len())..];
            return Some(Segment::Escape);
        }
        let end = self.rest.find('\x1b').unwrap_or(self.rest.len());
        let (text, rest) = self.rest.split_at(end);
        self.rest = rest;
        Some(Segment::Text(text))
    }
}
