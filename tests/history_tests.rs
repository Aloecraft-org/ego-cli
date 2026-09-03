//! History: what gets recorded, how a walk moves, and persistence through
//! `ego_platform`'s blob store.

mod common;
use common::{async_test, test};

use ego_cli::History;
use ego_platform::MemStore;

fn filled(lines: &[&str]) -> History {
    let mut history = History::new();
    for line in lines {
        history.push(line);
    }
    history
}

#[test]
fn blank_lines_are_not_recorded() {
    let history = filled(&["one", "", "   ", "two"]);
    assert_eq!(history.entries().collect::<Vec<_>>(), ["one", "two"]);
}

#[test]
fn an_immediate_repeat_is_not_recorded() {
    let history = filled(&["ls", "ls", "cd", "ls"]);
    assert_eq!(history.entries().collect::<Vec<_>>(), ["ls", "cd", "ls"]);
}

#[test]
fn dedup_can_be_switched_off() {
    let mut history = History::new();
    history.set_dedup_adjacent(false);
    history.push("ls");
    history.push("ls");
    assert_eq!(history.len(), 2);
}

/// The shell convention: a line typed with a leading space is not
/// remembered.
#[test]
fn a_leading_space_hides_a_line() {
    let history = filled(&[" secret --token=xyz", "ls"]);
    assert_eq!(history.entries().collect::<Vec<_>>(), ["ls"]);
}

#[test]
fn the_limit_drops_the_oldest() {
    let mut history = History::new();
    history.set_limit(2);
    for line in ["one", "two", "three"] {
        history.push(line);
    }
    assert_eq!(history.entries().collect::<Vec<_>>(), ["two", "three"]);
}

#[test]
fn a_zero_limit_records_nothing() {
    let mut history = History::new();
    history.set_limit(0);
    history.push("ls");
    assert!(history.is_empty());
}

#[test]
fn walking_back_and_forward_returns_to_the_draft() {
    let mut history = filled(&["one", "two"]);

    assert_eq!(
        history.older("dr", 2).as_deref(),
        None,
        "prefix 'dr' matches nothing"
    );
    history.end_navigation();

    assert_eq!(history.older("", 0).as_deref(), Some("two"));
    assert_eq!(history.older("", 0).as_deref(), Some("one"));
    assert_eq!(history.older("", 0), None);
    assert_eq!(history.newer().as_deref(), Some("two"));
    assert_eq!(history.newer().as_deref(), Some(""));
    assert_eq!(history.newer(), None, "the walk is over");
}

#[test]
fn prefix_search_filters_the_walk() {
    let mut history = filled(&["git status", "cargo test", "git commit"]);
    assert_eq!(history.older("git ", 4).as_deref(), Some("git commit"));
    assert_eq!(history.older("git ", 4).as_deref(), Some("git status"));
    assert_eq!(history.older("git ", 4), None);
}

/// The prefix is what is left of the cursor, not the whole line: recall
/// works mid-edit.
#[test]
fn the_prefix_is_taken_at_the_cursor() {
    let mut history = filled(&["git status"]);
    assert_eq!(history.older("git zzz", 4).as_deref(), Some("git status"));
}

#[test]
fn prefix_search_can_be_switched_off() {
    let mut history = filled(&["one"]);
    history.set_prefix_search(false);
    assert_eq!(history.older("zzz", 3).as_deref(), Some("one"));
}

#[test]
fn editing_ends_the_walk() {
    let mut history = filled(&["one", "two"]);
    assert_eq!(history.older("", 0).as_deref(), Some("two"));
    assert!(history.navigating());

    history.end_navigation();
    assert!(!history.navigating());
    assert_eq!(
        history.older("", 0).as_deref(),
        Some("two"),
        "the next walk starts from the newest again"
    );
}

// --- persistence ---

#[test]
fn the_wire_format_is_one_line_each() {
    let history = filled(&["one", "two"]);
    assert_eq!(history.encode(), b"one\ntwo\n");

    let mut restored = History::new();
    restored.decode(b"one\ntwo\n");
    assert_eq!(restored.entries().collect::<Vec<_>>(), ["one", "two"]);
}

#[test]
fn decoding_survives_a_damaged_file() {
    let mut history = History::new();
    history.decode(b"one\n\n\xff\xfe\ntwo\n");
    let entries: Vec<_> = history.entries().collect();
    assert_eq!(entries.first(), Some(&"one"));
    assert_eq!(entries.last(), Some(&"two"));
}

#[async_test]
async fn a_history_survives_through_a_blob_store() {
    let store = MemStore::new();
    let history = filled(&["one", "two"]);
    history.save(&store, "history").await.unwrap();

    let mut restored = History::new();
    restored.load(&store, "history").await.unwrap();
    assert_eq!(restored.entries().collect::<Vec<_>>(), ["one", "two"]);
}

#[async_test]
async fn loading_from_an_empty_store_changes_nothing() {
    let store = MemStore::new();
    let mut history = filled(&["kept"]);
    history.load(&store, "history").await.unwrap();
    assert_eq!(history.entries().collect::<Vec<_>>(), ["kept"]);
}

#[async_test]
async fn a_saved_history_reloads_under_the_current_limit() {
    let store = MemStore::new();
    filled(&["one", "two", "three"])
        .save(&store, "history")
        .await
        .unwrap();

    let mut restored = History::new();
    restored.set_limit(2);
    restored.load(&store, "history").await.unwrap();
    assert_eq!(restored.entries().collect::<Vec<_>>(), ["two", "three"]);
}
