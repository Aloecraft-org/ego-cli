use crate::input::normalize::NormalizedKey;
use std::collections::VecDeque;

#[derive(Debug, Clone, PartialEq)]
pub enum EditorAction {
    None,
    RedrawLine,
    Bell,
    Submit(String),
}

#[derive(Clone, Debug, PartialEq)]
struct LineState {
    buffer: String,
    cursor: usize,
}

pub struct LineEditor {
    // Current Line State
    pub input_buffer: String,
    pub cursor_pos: usize,

    // Undo/Redo
    undo_stack: Vec<LineState>,
    redo_stack: Vec<LineState>,

    // Command History
    history: Vec<String>,
    history_index: usize,
    history_draft: String,
}

impl Default for LineEditor {
    fn default() -> Self {
        Self::new()
    }
}

impl LineEditor {
    pub fn new() -> Self {
        Self {
            input_buffer: String::new(),
            cursor_pos: 0,
            undo_stack: Vec::new(),
            redo_stack: Vec::new(),
            history: Vec::new(),
            history_index: 0,
            history_draft: String::new(),
        }
    }

    pub fn handle_input(&mut self, key: NormalizedKey) -> EditorAction {
        match key {
            NormalizedKey::Enter => self.handle_enter(),
            
            NormalizedKey::Char(c) => {
                self.handle_char(c);
                EditorAction::RedrawLine
            }

            // Basic Navigation
            NormalizedKey::Left => { self.handle_left(); EditorAction::RedrawLine }
            NormalizedKey::Right => { self.handle_right(); EditorAction::RedrawLine }
            NormalizedKey::Home => { self.handle_home(); EditorAction::RedrawLine }
            NormalizedKey::End => { self.handle_end(); EditorAction::RedrawLine }
            NormalizedKey::Up => { self.handle_up(); EditorAction::RedrawLine }
            NormalizedKey::Down => { self.handle_down(); EditorAction::RedrawLine }

            // Ctrl Navigation
            NormalizedKey::CtrlLeft => { self.handle_ctrl_left(); EditorAction::RedrawLine }
            NormalizedKey::CtrlRight => { self.handle_ctrl_right(); EditorAction::RedrawLine }

            // Editing
            NormalizedKey::Backspace => { self.handle_backspace(); EditorAction::RedrawLine }
            NormalizedKey::Delete => { self.handle_delete(); EditorAction::RedrawLine }
            NormalizedKey::CtrlBackspace => { self.handle_ctrl_backspace(); EditorAction::RedrawLine }
            NormalizedKey::CtrlDelete => { self.handle_ctrl_delete(); EditorAction::RedrawLine }

            // Undo/Redo
            NormalizedKey::Undo => { self.handle_undo(); EditorAction::RedrawLine }
            NormalizedKey::Redo => { self.handle_redo(); EditorAction::RedrawLine }

            // Signals (Not handled by LineEditor logic yet)
            NormalizedKey::CtrlC => EditorAction::None,
            NormalizedKey::CtrlD => EditorAction::None,
        }
    }

    // --- Core Logic ---

    fn handle_enter(&mut self) -> EditorAction {
        let cmd = self.input_buffer.clone();

        // Add to history (skip empty or duplicates of the immediate previous command)
        if !cmd.trim().is_empty() {
            if self.history.last() != Some(&cmd) {
                self.history.push(cmd.clone());
            }
        }

        // Reset pointers
        self.history_index = self.history.len();
        self.history_draft.clear();

        // Clear Line
        self.input_buffer.clear();
        self.cursor_pos = 0;
        self.undo_stack.clear();
        self.redo_stack.clear();

        EditorAction::Submit(cmd)
    }

    fn handle_char(&mut self, c: char) {
        self.save_snapshot();
        if self.cursor_pos == self.input_buffer.len() {
            self.input_buffer.push(c);
        } else {
            // Safety check for char boundary
            if !self.input_buffer.is_char_boundary(self.cursor_pos) {
                self.cursor_pos = self.input_buffer.len();
                self.input_buffer.push(c);
            } else {
                self.input_buffer.insert(self.cursor_pos, c);
            }
        }
        self.cursor_pos += c.len_utf8();
    }

    fn handle_backspace(&mut self) {
        self.save_snapshot();
        if self.cursor_pos > 0 {
            let mut prev = self.cursor_pos;
            loop {
                prev -= 1;
                if self.input_buffer.is_char_boundary(prev) {
                    break;
                }
                if prev == 0 {
                    break;
                }
            }
            self.input_buffer.remove(prev);
            self.cursor_pos = prev;
        }
    }

    fn handle_delete(&mut self) {
        self.save_snapshot();
        if self.cursor_pos < self.input_buffer.len() {
            self.input_buffer.remove(self.cursor_pos);
        }
    }

    // --- Navigation ---

    fn handle_left(&mut self) {
        if self.cursor_pos > 0 {
            let mut new_pos = self.cursor_pos;
            while new_pos > 0 {
                new_pos -= 1;
                if self.input_buffer.is_char_boundary(new_pos) {
                    break;
                }
            }
            self.cursor_pos = new_pos;
        }
    }

    fn handle_right(&mut self) {
        if let Some(c) = self.input_buffer[self.cursor_pos..].chars().next() {
            self.cursor_pos += c.len_utf8();
        }
    }

    fn handle_home(&mut self) {
        self.cursor_pos = 0;
    }

    fn handle_end(&mut self) {
        self.cursor_pos = self.input_buffer.len();
    }

    fn handle_ctrl_left(&mut self) {
        if self.cursor_pos == 0 { return; }
        
        let chars: Vec<char> = self.input_buffer.chars().collect();
        // Convert byte index to char index for easier processing logic
        // This is expensive O(N) but safe. 
        // Given terminal input lines are short, this is acceptable.
        
        let char_indices: Vec<(usize, char)> = self.input_buffer.char_indices().collect();
        let current_char_idx = char_indices.iter().position(|(i, _)| *i == self.cursor_pos).unwrap_or(char_indices.len());
        
        if current_char_idx == 0 { return; }

        let mut new_char_idx = current_char_idx;
        
        // 1. Skip backward over whitespace
        while new_char_idx > 0 && char_indices[new_char_idx - 1].1.is_whitespace() {
            new_char_idx -= 1;
        }
        // 2. Skip backward over word
        while new_char_idx > 0 && !char_indices[new_char_idx - 1].1.is_whitespace() {
            new_char_idx -= 1;
        }
        
        self.cursor_pos = char_indices.get(new_char_idx).map(|(i, _)| *i).unwrap_or(self.input_buffer.len());
    }

    fn handle_ctrl_right(&mut self) {
        let len = self.input_buffer.len();
        if self.cursor_pos >= len { return; }

        let char_indices: Vec<(usize, char)> = self.input_buffer.char_indices().collect();
        let current_char_idx = char_indices.iter().position(|(i, _)| *i == self.cursor_pos).unwrap_or(char_indices.len());
        
        let mut new_char_idx = current_char_idx;
        let max_idx = char_indices.len();

        // 1. Skip forward over word
        while new_char_idx < max_idx && !char_indices[new_char_idx].1.is_whitespace() {
            new_char_idx += 1;
        }
        // 2. Skip forward over whitespace
        while new_char_idx < max_idx && char_indices[new_char_idx].1.is_whitespace() {
            new_char_idx += 1;
        }

        self.cursor_pos = char_indices.get(new_char_idx).map(|(i, _)| *i).unwrap_or(len);
    }

    // --- Advanced Editing ---

    fn handle_ctrl_backspace(&mut self) {
        self.save_snapshot();
        if self.cursor_pos == 0 { return; }

        // Reuse logic from ctrl_left to find start point
        let start_cursor = self.cursor_pos;
        self.handle_ctrl_left(); 
        let end_cursor = self.cursor_pos;
        
        // Restore cursor to original pos to perform delete, but wait...
        // Actually we want to delete from end_cursor to start_cursor.
        // handle_ctrl_left moved cursor to the *left*. So end_cursor < start_cursor.
        // We are at new position.
        
        self.input_buffer.replace_range(end_cursor..start_cursor, "");
        // Cursor stays at new position (end_cursor)
    }

    fn handle_ctrl_delete(&mut self) {
        self.save_snapshot();
        if self.cursor_pos >= self.input_buffer.len() { return; }

        let start_cursor = self.cursor_pos;
        self.handle_ctrl_right();
        let end_cursor = self.cursor_pos;

        // handle_ctrl_right moved cursor right.
        self.input_buffer.replace_range(start_cursor..end_cursor, "");
        self.cursor_pos = start_cursor;
    }

    // --- History ---

    fn handle_up(&mut self) {
        if self.history_index > 0 {
            if self.history_index == self.history.len() {
                self.history_draft = self.input_buffer.clone();
            }
            self.history_index -= 1;
            self.input_buffer = self.history[self.history_index].clone();
            self.cursor_pos = self.input_buffer.len();
            self.undo_stack.clear();
            self.redo_stack.clear();
        }
    }

    fn handle_down(&mut self) {
        if self.history_index < self.history.len() {
            self.history_index += 1;
            if self.history_index == self.history.len() {
                self.input_buffer = self.history_draft.clone();
            } else {
                self.input_buffer = self.history[self.history_index].clone();
            }
            self.cursor_pos = self.input_buffer.len();
            self.undo_stack.clear();
            self.redo_stack.clear();
        }
    }

    // --- Undo / Redo ---

    fn save_snapshot(&mut self) {
        let current_state = LineState {
            buffer: self.input_buffer.clone(),
            cursor: self.cursor_pos,
        };

        if let Some(last) = self.undo_stack.last() {
            if *last == current_state {
                return;
            }
        }

        self.undo_stack.push(current_state);
        self.redo_stack.clear();

        if self.undo_stack.len() > 50 {
            self.undo_stack.remove(0);
        }
    }

    fn handle_undo(&mut self) {
        if let Some(previous) = self.undo_stack.pop() {
            let current = LineState {
                buffer: self.input_buffer.clone(),
                cursor: self.cursor_pos,
            };
            self.redo_stack.push(current);
            self.restore_state(previous);
        }
    }

    fn handle_redo(&mut self) {
        if let Some(future) = self.redo_stack.pop() {
            let current = LineState {
                buffer: self.input_buffer.clone(),
                cursor: self.cursor_pos,
            };
            self.undo_stack.push(current);
            self.restore_state(future);
        }
    }

    fn restore_state(&mut self, state: LineState) {
        self.input_buffer = state.buffer;
        self.cursor_pos = state.cursor;
    }
}
