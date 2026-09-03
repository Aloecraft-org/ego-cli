//! Whole sessions, driven through the in-memory terminal: decoder, keymap,
//! editor, renderer and completion together.
//!
//! Input is written the way a terminal actually sends it, so these are also
//! the end-to-end test of the escape decoding.

mod common;
use common::async_test;

use ego_cli::extend::{Highlighter, WordCompleter};
use ego_cli::style::{self, Color};
use ego_cli::term::mem::MemTerminal;
use ego_cli::{ReadOutcome, Session, Size};

use std::borrow::Cow;

fn session(input: &str) -> Session<MemTerminal> {
    let mut term = MemTerminal::raw(Size::new(80, 24));
    term.push_input(input);
    Session::new(term)
}

async fn read(input: &str) -> ReadOutcome {
    session(input).read_line().await.unwrap()
}

#[async_test]
async fn a_typed_line_comes_back() {
    assert_eq!(read("hello\r").await, ReadOutcome::Line("hello".into()));
}

#[async_test]
async fn the_prompt_and_the_line_are_drawn() {
    let mut session = session("hi\r");
    session.set_prompt("$ ");
    session.read_line().await.unwrap();
    let output = session.terminal().output();
    assert!(output.contains("$ "), "{output:?}");
    assert!(output.contains("hi"), "{output:?}");
}

#[async_test]
async fn arrows_edit_the_line_before_it_is_accepted() {
    // "ac", Left, "b" -> "abc"
    assert_eq!(read("ac\x1b[Db\r").await, ReadOutcome::Line("abc".into()));
}

#[async_test]
async fn ctrl_arrows_move_by_word() {
    // "one two", Ctrl+Left, "X" -> "one Xtwo"
    assert_eq!(
        read("one two\x1b[1;5DX\r").await,
        ReadOutcome::Line("one Xtwo".into())
    );
}

#[async_test]
async fn home_and_end_reach_the_ends() {
    assert_eq!(
        read("bc\x1b[Ha\x1b[Fd\r").await,
        ReadOutcome::Line("abcd".into())
    );
}

#[async_test]
async fn ctrl_w_takes_the_previous_word() {
    assert_eq!(
        read("hello world\x17\r").await,
        ReadOutcome::Line("hello ".into())
    );
}

#[async_test]
async fn ctrl_u_and_ctrl_k_kill_to_the_ends() {
    assert_eq!(
        read("junk\x15kept\r").await,
        ReadOutcome::Line("kept".into())
    );
    // "keep this", Home, Ctrl+K
    assert_eq!(
        read("keep this\x1b[H\x0b\r").await,
        ReadOutcome::Line("".into())
    );
}

#[async_test]
async fn ctrl_z_undoes_and_ctrl_y_redoes() {
    assert_eq!(
        read("keep drop\x1a\r").await,
        ReadOutcome::Line("keep ".into())
    );
    assert_eq!(
        read("keep drop\x1a\x19\r").await,
        ReadOutcome::Line("keep drop".into())
    );
}

/// A terminal sends a bare Escape as its own write; `ESC` immediately
/// followed by a character is Alt+that-character, and the decoder is right
/// to read it that way. So this scripts two chunks, as a terminal would.
#[async_test]
async fn escape_clears_the_line() {
    let mut term = MemTerminal::raw(Size::new(80, 24));
    term.push_input("wrong");
    term.push_input("\x1b");
    term.push_input("right\r");
    let mut session = Session::new(term);
    assert_eq!(
        session.read_line().await.unwrap(),
        ReadOutcome::Line("right".into())
    );
}

#[async_test]
async fn alt_b_and_alt_f_move_by_word() {
    // "one two", Alt+b, "X" -> "one Xtwo"
    assert_eq!(
        read("one two\x1bbX\r").await,
        ReadOutcome::Line("one Xtwo".into())
    );
}

#[async_test]
async fn history_is_recalled_across_reads() {
    let mut session = session("first\rsecond\r\x1b[A\x1b[A\r");
    assert_eq!(
        session.read_line().await.unwrap(),
        ReadOutcome::Line("first".into())
    );
    assert_eq!(
        session.read_line().await.unwrap(),
        ReadOutcome::Line("second".into())
    );
    assert_eq!(
        session.read_line().await.unwrap(),
        ReadOutcome::Line("first".into()),
        "two Ups reach the older entry"
    );
}

#[async_test]
async fn ctrl_c_abandons_the_line_but_not_the_session() {
    let mut session = session("throw away\x03next\r");
    assert_eq!(session.read_line().await.unwrap(), ReadOutcome::Interrupted);
    assert!(session.terminal().output().contains("^C"));
    assert_eq!(
        session.read_line().await.unwrap(),
        ReadOutcome::Line("next".into())
    );
    assert!(
        session.history().entries().collect::<Vec<_>>() == ["next"],
        "an interrupted line is not remembered"
    );
}

#[async_test]
async fn ctrl_d_ends_input_only_on_an_empty_line() {
    assert_eq!(read("\x04").await, ReadOutcome::Eof);
    // Mid-line it is a forward delete: "abc", Home, Ctrl+D
    assert_eq!(
        read("abc\x1b[H\x04\r").await,
        ReadOutcome::Line("bc".into())
    );
}

#[async_test]
async fn a_closed_terminal_is_end_of_input() {
    assert_eq!(read("half typed").await, ReadOutcome::Eof);
}

// --- raw mode ---

#[async_test]
async fn raw_mode_is_entered_and_always_restored() {
    let mut session = session("hi\r");
    session.read_line().await.unwrap();
    assert_eq!(session.terminal().raw_calls(), [true, false]);
    assert!(!session.terminal().is_raw());
}

// --- completion ---

#[async_test]
async fn tab_completes_a_single_candidate() {
    let mut session = session("ec\t\r");
    session.set_completer(WordCompleter::new(["echo", "exit", "status"]));
    assert_eq!(
        session.read_line().await.unwrap(),
        ReadOutcome::Line("echo".into())
    );
}

#[async_test]
async fn tab_inserts_as_much_as_the_candidates_agree_on() {
    let mut session = session("e\t\r");
    session.set_completer(WordCompleter::new(["echoes", "echo"]));
    assert_eq!(
        session.read_line().await.unwrap(),
        ReadOutcome::Line("echo".into())
    );
}

#[async_test]
async fn an_ambiguous_tab_lists_the_choices() {
    let mut session = session("e\t\r");
    session.set_completer(WordCompleter::new(["echo", "exit"]));
    session.read_line().await.unwrap();

    let output = session.terminal().output();
    assert!(output.contains("echo"), "{output:?}");
    assert!(output.contains("exit"), "{output:?}");
}

#[async_test]
async fn tab_with_nothing_to_offer_leaves_the_line_alone() {
    let mut session = session("zz\t\r");
    session.set_completer(WordCompleter::new(["echo"]));
    assert_eq!(
        session.read_line().await.unwrap(),
        ReadOutcome::Line("zz".into())
    );
}

#[async_test]
async fn completion_applies_to_the_token_under_the_cursor() {
    let mut session = session("echo ec\t\r");
    session.set_completer(WordCompleter::new(["echo"]));
    assert_eq!(
        session.read_line().await.unwrap(),
        ReadOutcome::Line("echo echo".into())
    );
}

// --- highlighting ---

struct Green;

impl Highlighter for Green {
    fn highlight<'l>(&self, line: &'l str) -> Cow<'l, str> {
        Cow::Owned(style::paint(line, Color::Green))
    }
}

#[async_test]
async fn a_highlighter_colours_the_line_without_changing_it() {
    let mut session = session("hi\r");
    session.set_highlighter(Green);
    assert_eq!(
        session.read_line().await.unwrap(),
        ReadOutcome::Line("hi".into()),
        "the line the host gets is the plain one"
    );
    assert!(
        session
            .terminal()
            .output()
            .contains(style::fg(Color::Green))
    );
}

// --- resize ---

#[async_test]
async fn a_resize_is_taken_up_mid_line() {
    let mut term = MemTerminal::raw(Size::new(80, 24));
    term.push_input("hi");
    term.push_resize(Size::new(40, 10));
    term.push_input("\r");
    let mut session = Session::new(term);

    assert_eq!(
        session.read_line().await.unwrap(),
        ReadOutcome::Line("hi".into())
    );
}

// --- the line-at-a-time platform ---

fn cooked_session(lines: &[&str]) -> Session<MemTerminal> {
    let mut term = MemTerminal::cooked(Size::DEFAULT);
    for line in lines {
        term.push_line(line);
    }
    Session::new(term)
}

#[async_test]
async fn a_line_at_a_time_terminal_still_reads_lines() {
    let mut session = cooked_session(&["one", "two"]);
    assert_eq!(
        session.read_line().await.unwrap(),
        ReadOutcome::Line("one".into())
    );
    assert_eq!(
        session.read_line().await.unwrap(),
        ReadOutcome::Line("two".into())
    );
    assert_eq!(session.read_line().await.unwrap(), ReadOutcome::Eof);
}

#[async_test]
async fn a_line_at_a_time_terminal_still_records_history() {
    let mut session = cooked_session(&["one", "two"]);
    session.read_line().await.unwrap();
    session.read_line().await.unwrap();
    assert_eq!(
        session.history().entries().collect::<Vec<_>>(),
        ["one", "two"]
    );
}

#[async_test]
async fn a_line_at_a_time_terminal_writes_a_plain_prompt() {
    let mut session = cooked_session(&["one"]);
    session.set_prompt(style::paint("$ ", Color::Green));
    session.read_line().await.unwrap();

    let output = session.terminal().output();
    assert_eq!(output, "$ ", "no escapes where none are understood");
}

#[async_test]
async fn a_line_at_a_time_terminal_never_goes_raw() {
    let mut session = cooked_session(&["one"]);
    session.read_line().await.unwrap();
    assert!(!session.terminal().is_raw());
}
