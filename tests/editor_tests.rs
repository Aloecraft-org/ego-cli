//! Line editing: motion, deletion, undo, history recall.

mod common;
use common::test;

use ego_cli::editor::{EditOutcome, LineEditor};
use ego_cli::keymap::Action;

/// Type `text` a character at a time.
fn typed(text: &str) -> LineEditor {
    let mut editor = LineEditor::new();
    for c in text.chars() {
        editor.apply(Action::Insert(c));
    }
    editor
}

#[test]
fn insertion_moves_the_cursor() {
    let editor = typed("hi");
    assert_eq!(editor.buffer(), "hi");
    assert_eq!(editor.cursor(), 2);
}

#[test]
fn insertion_at_the_cursor_not_the_end() {
    let mut editor = typed("ac");
    editor.apply(Action::MoveLeft);
    editor.apply(Action::Insert('b'));
    assert_eq!(editor.buffer(), "abc");
    assert_eq!(editor.cursor(), 2);
}

#[test]
fn backspace_and_delete() {
    let mut editor = typed("hello");
    editor.apply(Action::DeleteBack);
    assert_eq!(editor.buffer(), "hell");

    editor.apply(Action::MoveStart);
    editor.apply(Action::DeleteForward);
    assert_eq!(editor.buffer(), "ell");
    assert_eq!(editor.cursor(), 0);
}

#[test]
fn deleting_at_an_edge_does_nothing() {
    let mut editor = LineEditor::new();
    assert_eq!(editor.apply(Action::DeleteBack), EditOutcome::Ignored);
    assert_eq!(editor.apply(Action::DeleteForward), EditOutcome::Ignored);
    assert_eq!(editor.buffer(), "");
}

#[test]
fn motion_lands_on_the_ends() {
    let mut editor = typed("12345");
    editor.apply(Action::MoveStart);
    assert_eq!(editor.cursor(), 0);
    assert_eq!(editor.apply(Action::MoveLeft), EditOutcome::Ignored);

    editor.apply(Action::MoveEnd);
    assert_eq!(editor.cursor(), 5);
    assert_eq!(editor.apply(Action::MoveRight), EditOutcome::Ignored);
}

#[test]
fn word_motion_skips_whitespace_then_the_word() {
    let mut editor = typed("one two  three");
    editor.apply(Action::MoveWordLeft);
    assert_eq!(&editor.buffer()[editor.cursor()..], "three");

    editor.apply(Action::MoveWordLeft);
    assert_eq!(&editor.buffer()[editor.cursor()..], "two  three");

    editor.apply(Action::MoveWordRight);
    assert_eq!(&editor.buffer()[editor.cursor()..], "three");
}

#[test]
fn ctrl_backspace_takes_the_word_and_the_space_after_it() {
    let mut editor = typed("hello world");
    editor.apply(Action::DeleteWordBack);
    assert_eq!(editor.buffer(), "hello ");

    editor.apply(Action::DeleteWordBack);
    assert_eq!(editor.buffer(), "");
}

#[test]
fn ctrl_delete_takes_the_word_ahead() {
    let mut editor = typed("hello world");
    editor.apply(Action::MoveStart);
    editor.apply(Action::DeleteWordForward);
    assert_eq!(editor.buffer(), "world");
    assert_eq!(editor.cursor(), 0);
}

#[test]
fn kills_reach_the_ends() {
    let mut editor = typed("hello world");
    editor.apply(Action::MoveWordLeft);
    editor.apply(Action::KillToEnd);
    assert_eq!(editor.buffer(), "hello ");

    editor.apply(Action::KillToStart);
    assert_eq!(editor.buffer(), "");
}

/// Motion moves by grapheme cluster, so a combining accent and the letter
/// it sits on are one press, not two.
#[test]
fn motion_and_deletion_are_grapheme_aware() {
    let mut editor = typed("e\u{301}x"); // "é" as e + combining acute, then x
    assert_eq!(editor.cursor(), editor.buffer().len());

    editor.apply(Action::MoveLeft);
    assert_eq!(&editor.buffer()[editor.cursor()..], "x");

    editor.apply(Action::MoveLeft);
    assert_eq!(editor.cursor(), 0, "one press crosses the whole cluster");

    editor.apply(Action::MoveEnd);
    editor.apply(Action::DeleteBack);
    editor.apply(Action::DeleteBack);
    assert_eq!(editor.buffer(), "");
}

#[test]
fn multibyte_insertion_keeps_the_cursor_on_a_boundary() {
    let mut editor = typed("日本");
    editor.apply(Action::MoveLeft);
    editor.apply(Action::Insert('x'));
    assert_eq!(editor.buffer(), "日x本");
}

// --- undo ---

/// A run of typing is one undo step, which is what an editor does and what
/// `ego_shell`'s per-keystroke snapshot did not.
#[test]
fn undo_takes_back_a_word_at_a_time() {
    let mut editor = typed("git status");
    editor.apply(Action::Undo);
    assert_eq!(editor.buffer(), "git ");

    editor.apply(Action::Undo);
    assert_eq!(editor.buffer(), "");
}

#[test]
fn redo_puts_it_back() {
    let mut editor = typed("abc");
    editor.apply(Action::Undo);
    assert_eq!(editor.buffer(), "");

    editor.apply(Action::Redo);
    assert_eq!(editor.buffer(), "abc");
    assert_eq!(editor.cursor(), 3);
}

#[test]
fn a_cursor_move_breaks_the_undo_run() {
    let mut editor = typed("ab");
    editor.apply(Action::MoveLeft);
    editor.apply(Action::Insert('X'));
    assert_eq!(editor.buffer(), "aXb");

    editor.apply(Action::Undo);
    assert_eq!(editor.buffer(), "ab", "only the X, not the whole line");
}

#[test]
fn undo_on_a_fresh_line_is_ignored() {
    let mut editor = LineEditor::new();
    assert_eq!(editor.apply(Action::Undo), EditOutcome::Ignored);
    assert_eq!(editor.apply(Action::Redo), EditOutcome::Ignored);
}

#[test]
fn a_new_edit_drops_the_redo_stack() {
    let mut editor = typed("abc");
    editor.apply(Action::Undo);
    editor.apply(Action::Insert('z'));
    assert_eq!(editor.apply(Action::Redo), EditOutcome::Ignored);
    assert_eq!(editor.buffer(), "z");
}

#[test]
fn undo_can_be_switched_off() {
    let mut editor = LineEditor::new();
    editor.set_undo_limit(0);
    editor.apply(Action::Insert('a'));
    assert_eq!(editor.apply(Action::Undo), EditOutcome::Ignored);
    assert_eq!(editor.buffer(), "a");
}

// --- accept, history, signals ---

#[test]
fn accept_hands_over_the_line_and_records_it() {
    let mut editor = typed("run");
    match editor.apply(Action::Accept) {
        EditOutcome::Accept(line) => assert_eq!(line, "run"),
        other => panic!("expected Accept, got {other:?}"),
    }
    assert_eq!(editor.buffer(), "");
    assert_eq!(editor.cursor(), 0);
    assert_eq!(editor.history().entries().collect::<Vec<_>>(), ["run"]);
}

#[test]
fn history_recall_walks_back_and_forward() {
    let mut editor = LineEditor::new();
    for line in ["first", "second"] {
        for c in line.chars() {
            editor.apply(Action::Insert(c));
        }
        editor.apply(Action::Accept);
    }

    editor.apply(Action::HistoryPrev);
    assert_eq!(editor.buffer(), "second");
    editor.apply(Action::HistoryPrev);
    assert_eq!(editor.buffer(), "first");
    assert_eq!(editor.apply(Action::HistoryPrev), EditOutcome::Ignored);

    editor.apply(Action::HistoryNext);
    assert_eq!(editor.buffer(), "second");
    editor.apply(Action::HistoryNext);
    assert_eq!(editor.buffer(), "", "back to the line being typed");
}

#[test]
fn history_recall_keeps_a_half_typed_line() {
    let mut editor = typed("done");
    editor.apply(Action::Accept);
    editor.history_mut().set_prefix_search(false);
    for c in "half".chars() {
        editor.apply(Action::Insert(c));
    }

    editor.apply(Action::HistoryPrev);
    assert_eq!(editor.buffer(), "done");
    editor.apply(Action::HistoryNext);
    assert_eq!(editor.buffer(), "half");
}

/// The default: Up walks only the entries that start with what has already
/// been typed, so a prompt with a long history stays usable.
#[test]
fn prefix_search_narrows_the_walk() {
    let mut editor = LineEditor::new();
    for line in ["git status", "cargo test", "git commit"] {
        for c in line.chars() {
            editor.apply(Action::Insert(c));
        }
        editor.apply(Action::Accept);
    }
    for c in "git ".chars() {
        editor.apply(Action::Insert(c));
    }

    editor.apply(Action::HistoryPrev);
    assert_eq!(editor.buffer(), "git commit");
    editor.apply(Action::HistoryPrev);
    assert_eq!(editor.buffer(), "git status", "cargo test is skipped");
    assert_eq!(editor.apply(Action::HistoryPrev), EditOutcome::Ignored);

    editor.apply(Action::HistoryNext);
    assert_eq!(editor.buffer(), "git commit");
    editor.apply(Action::HistoryNext);
    assert_eq!(editor.buffer(), "git ", "back to what was being typed");
}

#[test]
fn ctrl_d_is_end_of_input_only_on_an_empty_line() {
    let mut editor = typed("ab");
    editor.apply(Action::MoveStart);
    assert_eq!(editor.apply(Action::Eof), EditOutcome::Continue);
    assert_eq!(editor.buffer(), "b", "mid-line it deletes forward");

    editor.apply(Action::DeleteForward);
    assert_eq!(editor.apply(Action::Eof), EditOutcome::Eof);
}

#[test]
fn escape_clears_the_line_and_is_undoable() {
    let mut editor = typed("half typed");
    assert_eq!(editor.apply(Action::Cancel), EditOutcome::Continue);
    assert_eq!(editor.buffer(), "");

    editor.apply(Action::Undo);
    assert_eq!(editor.buffer(), "half typed");
}

#[test]
fn completion_replaces_a_range_and_clamps_a_bad_one() {
    let mut editor = typed("ech");
    editor.replace_range(0..3, "echo");
    assert_eq!(editor.buffer(), "echo");
    assert_eq!(editor.cursor(), 4);

    // A completer that miscounts must not panic the session.
    editor.replace_range(2..900, "!");
    assert_eq!(editor.buffer(), "ec!");
}
