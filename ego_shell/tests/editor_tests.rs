use egoshell::input::editor::*;
use egoshell::input::NormalizedKey;

mod common;
use common::{async_test, test};

#[test]
fn test_basic_insertion() {
    let mut editor = LineEditor::new();
    
    editor.handle_input(NormalizedKey::Char('h'));
    editor.handle_input(NormalizedKey::Char('i'));
    
    assert_eq!(editor.input_buffer, "hi");
    assert_eq!(editor.cursor_pos, 2);
}

#[test]
fn test_backspace() {
    let mut editor = LineEditor::new();
    editor.input_buffer = "hello".to_string();
    editor.cursor_pos = 5;
    
    editor.handle_input(NormalizedKey::Backspace);
    assert_eq!(editor.input_buffer, "hell");
    assert_eq!(editor.cursor_pos, 4);
    
    // Middle delete
    editor.cursor_pos = 2; // "he|ll"
    editor.handle_input(NormalizedKey::Backspace);
    assert_eq!(editor.input_buffer, "hll"); // "h|ll"
    assert_eq!(editor.cursor_pos, 1);
}

#[test]
fn test_navigation() {
    let mut editor = LineEditor::new();
    editor.input_buffer = "12345".to_string();
    editor.cursor_pos = 0;
    
    editor.handle_input(NormalizedKey::Right);
    assert_eq!(editor.cursor_pos, 1);
    
    editor.handle_input(NormalizedKey::End);
    assert_eq!(editor.cursor_pos, 5);
    
    editor.handle_input(NormalizedKey::Home);
    assert_eq!(editor.cursor_pos, 0);
}

#[test]
fn test_undo_redo() {
    let mut editor = LineEditor::new();
    
    // Type 'a'
    editor.handle_input(NormalizedKey::Char('a'));
    assert_eq!(editor.input_buffer, "a");
    
    // Type 'b'
    editor.handle_input(NormalizedKey::Char('b'));
    assert_eq!(editor.input_buffer, "ab");
    
    // Undo 'b' -> 'a'
    editor.handle_input(NormalizedKey::Undo);
    assert_eq!(editor.input_buffer, "a");
    
    // Undo 'a' -> ''
    editor.handle_input(NormalizedKey::Undo);
    assert_eq!(editor.input_buffer, "");
    
    // Redo -> 'a'
    editor.handle_input(NormalizedKey::Redo);
    assert_eq!(editor.input_buffer, "a");
}

#[test]
fn test_ctrl_backspace() {
    let mut editor = LineEditor::new();
    editor.input_buffer = "hello world".to_string();
    editor.cursor_pos = 11; // End
    
    // Delete "world"
    editor.handle_input(NormalizedKey::CtrlBackspace);
    assert_eq!(editor.input_buffer, "hello ");
    
    // Delete "hello " (Standard Ctrl+Backspace consumes the preceding word AND its trailing whitespace)
    editor.handle_input(NormalizedKey::CtrlBackspace);
    assert_eq!(editor.input_buffer, "");
}

#[test]
fn test_submit() {
    let mut editor = LineEditor::new();
    editor.handle_input(NormalizedKey::Char('r'));
    editor.handle_input(NormalizedKey::Char('u'));
    editor.handle_input(NormalizedKey::Char('n'));
    
    match editor.handle_input(NormalizedKey::Enter) {
        EditorAction::Submit(s) => assert_eq!(s, "run"),
        _ => panic!("Expected Submit action"),
    }
    
    // Buffer should be cleared
    assert_eq!(editor.input_buffer, "");
    assert_eq!(editor.cursor_pos, 0);
    
    // History should have "run"
    editor.handle_input(NormalizedKey::Up);
    assert_eq!(editor.input_buffer, "run");
}