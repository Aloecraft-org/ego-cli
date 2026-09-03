//! The escape-sequence decoder, on every target.
//!
//! `ego_shell` could only test this in the browser, because its decoder was
//! written against browser strings. This one is platform-free, so the same
//! assertions run natively, under wasmtime, and in a headless browser.

mod common;
use common::test;

use ego_cli::decode::AnsiDecoder;
use ego_cli::key::{KeyCode, KeyPress, Mods};

fn keys(input: &str) -> Vec<KeyPress> {
    AnsiDecoder::new().push(input)
}

#[test]
fn plain_characters() {
    assert_eq!(
        keys("hi!"),
        vec![
            KeyPress::char('h'),
            KeyPress::char('i'),
            KeyPress::char('!')
        ]
    );
}

#[test]
fn non_ascii_characters_survive() {
    assert_eq!(
        keys("héllo→"),
        "héllo→".chars().map(KeyPress::char).collect::<Vec<_>>()
    );
}

#[test]
fn enter_and_tab() {
    assert_eq!(keys("\r"), vec![KeyPress::plain(KeyCode::Enter)]);
    assert_eq!(keys("\n"), vec![KeyPress::plain(KeyCode::Enter)]);
    assert_eq!(keys("\t"), vec![KeyPress::plain(KeyCode::Tab)]);
}

#[test]
fn backspace_is_del_and_ctrl_backspace_is_bs() {
    assert_eq!(keys("\x7f"), vec![KeyPress::plain(KeyCode::Backspace)]);
    assert_eq!(
        keys("\x08"),
        vec![KeyPress::new(KeyCode::Backspace, Mods::CTRL)]
    );
}

/// The whole C0 range falls out of one rule, which is the point: no table
/// entry was needed for Ctrl+W, Ctrl+U or Ctrl+K.
#[test]
fn control_characters_are_ctrl_plus_letter() {
    assert_eq!(keys("\x01"), vec![KeyPress::ctrl('a')]);
    assert_eq!(keys("\x03"), vec![KeyPress::ctrl('c')]);
    assert_eq!(keys("\x04"), vec![KeyPress::ctrl('d')]);
    assert_eq!(keys("\x0b"), vec![KeyPress::ctrl('k')]);
    assert_eq!(keys("\x15"), vec![KeyPress::ctrl('u')]);
    assert_eq!(keys("\x17"), vec![KeyPress::ctrl('w')]);
    assert_eq!(keys("\x1a"), vec![KeyPress::ctrl('z')]);
    assert_eq!(keys("\x1f"), vec![KeyPress::ctrl('_')]);
}

#[test]
fn arrows_in_both_cursor_modes() {
    for (normal, application, code) in [
        ("\x1b[A", "\x1bOA", KeyCode::Up),
        ("\x1b[B", "\x1bOB", KeyCode::Down),
        ("\x1b[C", "\x1bOC", KeyCode::Right),
        ("\x1b[D", "\x1bOD", KeyCode::Left),
    ] {
        assert_eq!(keys(normal), vec![KeyPress::plain(code)], "{normal:?}");
        assert_eq!(
            keys(application),
            vec![KeyPress::plain(code)],
            "{application:?}"
        );
    }
}

#[test]
fn home_and_end_in_every_spelling() {
    for input in ["\x1b[H", "\x1bOH", "\x1b[1~", "\x1b[7~"] {
        assert_eq!(
            keys(input),
            vec![KeyPress::plain(KeyCode::Home)],
            "{input:?}"
        );
    }
    for input in ["\x1b[F", "\x1bOF", "\x1b[4~", "\x1b[8~"] {
        assert_eq!(
            keys(input),
            vec![KeyPress::plain(KeyCode::End)],
            "{input:?}"
        );
    }
}

#[test]
fn delete_and_ctrl_delete() {
    assert_eq!(keys("\x1b[3~"), vec![KeyPress::plain(KeyCode::Delete)]);
    assert_eq!(
        keys("\x1b[3;5~"),
        vec![KeyPress::new(KeyCode::Delete, Mods::CTRL)]
    );
    // rxvt puts the modifier in the final byte instead.
    assert_eq!(
        keys("\x1b[3^"),
        vec![KeyPress::new(KeyCode::Delete, Mods::CTRL)]
    );
}

#[test]
fn ctrl_arrows_in_both_spellings() {
    assert_eq!(
        keys("\x1b[1;5D"),
        vec![KeyPress::new(KeyCode::Left, Mods::CTRL)]
    );
    assert_eq!(
        keys("\x1b[5D"),
        vec![KeyPress::new(KeyCode::Left, Mods::CTRL)]
    );
    assert_eq!(
        keys("\x1b[1;5C"),
        vec![KeyPress::new(KeyCode::Right, Mods::CTRL)]
    );
    assert_eq!(
        keys("\x1b[5C"),
        vec![KeyPress::new(KeyCode::Right, Mods::CTRL)]
    );
}

#[test]
fn modifier_parameter_decodes_every_combination() {
    assert_eq!(
        keys("\x1b[1;2C"),
        vec![KeyPress::new(KeyCode::Right, Mods::SHIFT)]
    );
    assert_eq!(
        keys("\x1b[1;3C"),
        vec![KeyPress::new(KeyCode::Right, Mods::ALT)]
    );
    assert_eq!(
        keys("\x1b[1;6C"),
        vec![KeyPress::new(KeyCode::Right, Mods::CTRL | Mods::SHIFT)]
    );
}

#[test]
fn alt_is_esc_then_the_key() {
    assert_eq!(keys("\x1bb"), vec![KeyPress::alt('b')]);
    assert_eq!(keys("\x1bf"), vec![KeyPress::alt('f')]);
    assert_eq!(keys("\x1bd"), vec![KeyPress::alt('d')]);
    assert_eq!(
        keys("\x1b\x7f"),
        vec![KeyPress::new(KeyCode::Backspace, Mods::ALT)]
    );
}

#[test]
fn shift_tab_is_csi_z() {
    assert_eq!(
        keys("\x1b[Z"),
        vec![KeyPress::new(KeyCode::Tab, Mods::SHIFT)]
    );
}

#[test]
fn function_keys() {
    assert_eq!(keys("\x1bOP"), vec![KeyPress::plain(KeyCode::F(1))]);
    assert_eq!(keys("\x1b[15~"), vec![KeyPress::plain(KeyCode::F(5))]);
    assert_eq!(keys("\x1b[24~"), vec![KeyPress::plain(KeyCode::F(12))]);
}

#[test]
fn a_lone_escape_at_the_end_of_a_chunk_is_the_escape_key() {
    assert_eq!(keys("\x1b"), vec![KeyPress::plain(KeyCode::Escape)]);
    assert_eq!(
        keys("ab\x1b"),
        vec![
            KeyPress::char('a'),
            KeyPress::char('b'),
            KeyPress::plain(KeyCode::Escape)
        ]
    );
}

/// The bug the `str::replace` decoder could not fix: a sequence arriving in
/// pieces.
#[test]
fn a_sequence_split_across_chunks_still_decodes() {
    let mut decoder = AnsiDecoder::new();
    assert!(decoder.push("\x1b[").is_empty());
    assert!(decoder.push("1;").is_empty());
    assert_eq!(
        decoder.push("5D"),
        vec![KeyPress::new(KeyCode::Left, Mods::CTRL)]
    );
}

#[test]
fn several_sequences_in_one_chunk() {
    assert_eq!(
        keys("ls\x1b[D\x1b[Ca\r"),
        vec![
            KeyPress::char('l'),
            KeyPress::char('s'),
            KeyPress::plain(KeyCode::Left),
            KeyPress::plain(KeyCode::Right),
            KeyPress::char('a'),
            KeyPress::plain(KeyCode::Enter),
        ]
    );
}

#[test]
fn private_reports_are_not_keys() {
    // A device-status reply is an answer to a question, not something the
    // human pressed.
    assert_eq!(keys("\x1b[?1;2c"), vec![]);
    assert_eq!(
        keys("\x1b[?1;2ca"),
        vec![KeyPress::char('a')],
        "and the machine recovers"
    );
}

#[test]
fn flush_resolves_a_pending_escape() {
    let mut decoder = AnsiDecoder::new();
    assert!(decoder.push("\x1b[").is_empty());
    assert_eq!(decoder.flush(), vec![], "a half-read CSI is discarded");

    let mut decoder = AnsiDecoder::new();
    decoder.push("a");
    assert_eq!(decoder.flush(), vec![]);
}
