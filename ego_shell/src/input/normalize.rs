use std::collections::VecDeque;

/// Normalized representation of keyboard input that maps to shell actions
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NormalizedKey {
    // Character input
    Char(char),
    Enter,

    // Basic navigation
    Left,
    Right,
    Home,
    End,
    Up,
    Down,

    // Editing
    Backspace,
    Delete,

    // Ctrl + navigation
    CtrlLeft,
    CtrlRight,

    // Ctrl + editing
    CtrlBackspace,
    CtrlDelete,

    // Undo/Redo
    Undo,
    Redo,

    // Control signals
    CtrlC,
    CtrlD,
}

#[cfg(not(target_arch = "wasm32"))]
pub type Normalizer = CrosstermNormalizer;

#[cfg(all(target_arch = "wasm32", target_os = "unknown"))]
pub type Normalizer = BrowserNormalizer;

#[cfg(all(target_arch = "wasm32", target_env = "p2"))]
pub type Normalizer = WASINormalizer;

/// Trait for normalizing platform-specific input into NormalizedKey events
pub trait InputNormalizer {
    type InputType;

    fn normalize(&mut self) -> Vec<NormalizedKey>;

    fn feed(&mut self, event: Self::InputType);

}

impl Normalizer {
    pub fn new() -> Self {
        #[cfg(not(target_arch = "wasm32"))]
        {
            Self {
                event_queue: VecDeque::new(),
            }
        }

        #[cfg(all(target_arch = "wasm32", target_os = "unknown"))]
        {
            Self {
                input_queue: VecDeque::new() 
            }
        }

        #[cfg(all(target_arch = "wasm32", target_env = "p2"))]
        {
            Self
        }
    }
}


#[cfg(all(target_arch = "wasm32", target_env = "p2"))]
pub struct WASINormalizer;

#[cfg(all(target_arch = "wasm32", target_env = "p2"))]
impl InputNormalizer for  WASINormalizer {
    type InputType = String;

    fn feed(&mut self, _event: Self::InputType) {
        unreachable!("Normalizer not supported on WASI P2");
     }

    fn normalize(&mut self) -> Vec<NormalizedKey> {
        unreachable!("Normalizer not supported on WASI P2");
    }
}

// ============================================================================
// NATIVE IMPLEMENTATION (using crossterm)
// ============================================================================

#[cfg(not(target_arch = "wasm32"))]
pub struct CrosstermNormalizer {
    event_queue: VecDeque<crossterm::event::KeyEvent>,
}


#[cfg(not(target_arch = "wasm32"))]
impl InputNormalizer for CrosstermNormalizer {
    type InputType = crossterm::event::KeyEvent;

    fn feed(&mut self, event: crossterm::event::KeyEvent) {
        self.event_queue.push_back(event);
    }

    fn normalize(&mut self) -> Vec<NormalizedKey> {
        use crossterm::event::{KeyCode, KeyModifiers};

        let mut keys = Vec::new();

        while let Some(key) = self.event_queue.pop_front() {
            let normalized = match key.code {
                // Control signals
                KeyCode::Char('c') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                    Some(NormalizedKey::CtrlC)
                }
                KeyCode::Char('d') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                    Some(NormalizedKey::CtrlD)
                }

                // Command History
                KeyCode::Up => Some(NormalizedKey::Up),
                KeyCode::Down => Some(NormalizedKey::Down),

                // Ctrl + Navigation
                KeyCode::Left if key.modifiers.contains(KeyModifiers::CONTROL) => {
                    Some(NormalizedKey::CtrlLeft)
                }
                KeyCode::Right if key.modifiers.contains(KeyModifiers::CONTROL) => {
                    Some(NormalizedKey::CtrlRight)
                }

                // Ctrl + Editing (native mappings)
                KeyCode::Backspace if key.modifiers.contains(KeyModifiers::CONTROL) => {
                    Some(NormalizedKey::CtrlBackspace)
                }
                KeyCode::Delete if key.modifiers.contains(KeyModifiers::CONTROL) => {
                    Some(NormalizedKey::CtrlDelete)
                }

                // Legacy/Unix mappings (Ctrl+W)
                KeyCode::Char('w') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                    Some(NormalizedKey::CtrlBackspace)
                }

                // Emacs mappings
                KeyCode::Char('d') if key.modifiers.contains(KeyModifiers::ALT) => {
                    Some(NormalizedKey::CtrlDelete)
                }
                KeyCode::Backspace if key.modifiers.contains(KeyModifiers::ALT) => {
                    Some(NormalizedKey::CtrlBackspace)
                }

                // Undo / Redo
                KeyCode::Char('z') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                    Some(NormalizedKey::Undo)
                }
                KeyCode::Char('y') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                    Some(NormalizedKey::Redo)
                }

                // Standard navigation
                KeyCode::Left => Some(NormalizedKey::Left),
                KeyCode::Right => Some(NormalizedKey::Right),
                KeyCode::Home => Some(NormalizedKey::Home),
                KeyCode::End => Some(NormalizedKey::End),

                // Editing
                KeyCode::Delete => Some(NormalizedKey::Delete),
                KeyCode::Backspace => Some(NormalizedKey::Backspace),
                KeyCode::Enter => Some(NormalizedKey::Enter),

                // Character input
                KeyCode::Char(c) => {
                    if c == '\r' || c == '\n' {
                        Some(NormalizedKey::Enter)
                    } else {
                        Some(NormalizedKey::Char(c))
                    }
                }

                _ => None,
            };

            if let Some(key) = normalized {
                keys.push(key);
            }
        }

        keys
    }
}

// ============================================================================
// BROWSER IMPLEMENTATION (WASM with escape sequence parsing)
// ============================================================================

#[cfg(all(target_arch = "wasm32", target_os = "unknown"))]
pub struct BrowserNormalizer {
    input_queue: VecDeque<String>,
}

#[cfg(all(target_arch = "wasm32", target_os = "unknown"))]
impl BrowserNormalizer {
    /// Parse escape sequences from raw terminal input
    fn parse_sequences(&self, input: &str) -> String {
        input
            // --- Ctrl + Editing ---
            .replace("\x1b[3;5~", "\u{E021}") // Ctrl+Delete
            .replace("\x1b[3^", "\u{E021}")
            // Ctrl+Backspace variations
            .replace("\x17", "\u{E020}") // ^W
            .replace("\x1b\x7f", "\u{E020}") // Alt+Bksp
            .replace("\x1b\x08", "\u{E020}") // Alt+^H
            .replace("\x08", "\u{E020}") // ASCII 8 (Ctrl+H)
            // --- Ctrl + Navigation ---
            .replace("\x1b[1;5D", "\u{E010}")
            .replace("\x1b[5D", "\u{E010}")
            .replace("\x1b[1;5C", "\u{E011}")
            .replace("\x1b[5C", "\u{E011}")
            // --- Standard Arrows ---
            .replace("\x1b[D", "\u{E000}")
            .replace("\x1bOD", "\u{E000}")
            .replace("\x1b[C", "\u{E001}")
            .replace("\x1bOC", "\u{E001}")
            // --- Home / End ---
            .replace("\x1b[H", "\u{E002}")
            .replace("\x1bOH", "\u{E002}")
            .replace("\x1b[1~", "\u{E002}")
            .replace("\x1b[F", "\u{E003}")
            .replace("\x1bOF", "\u{E003}")
            .replace("\x1b[4~", "\u{E003}")
            // --- Command History ---
            .replace("\x1b[A", "\u{E040}")
            .replace("\x1bOA", "\u{E040}") // Up
            .replace("\x1b[B", "\u{E041}")
            .replace("\x1bOB", "\u{E041}") // Down
            // --- Undo / Redo ---
            .replace("\x1a", "\u{E030}") // Ctrl+Z
            .replace("\x19", "\u{E031}") // Ctrl+Y
            // --- Forward Delete ---
            .replace("\x1b[3~", "\u{E004}")
    }

    /// Map PUA codepoints and control characters to NormalizedKey
    fn map_char(&self, c: char) -> Option<NormalizedKey> {
        match c {
            // Line endings
            '\r' | '\n' => Some(NormalizedKey::Enter),

            // Backspace
            '\x7f' => Some(NormalizedKey::Backspace),

            // Ctrl+C
            '\x03' => Some(NormalizedKey::CtrlC),

            // PUA mappings - Basic Navigation
            '\u{E000}' => Some(NormalizedKey::Left),
            '\u{E001}' => Some(NormalizedKey::Right),
            '\u{E002}' => Some(NormalizedKey::Home),
            '\u{E003}' => Some(NormalizedKey::End),
            '\u{E004}' => Some(NormalizedKey::Delete),

            // PUA mappings - Ctrl + Navigation
            '\u{E010}' => Some(NormalizedKey::CtrlLeft),
            '\u{E011}' => Some(NormalizedKey::CtrlRight),

            // PUA mappings - Ctrl + Editing
            '\u{E020}' => Some(NormalizedKey::CtrlBackspace),
            '\u{E021}' => Some(NormalizedKey::CtrlDelete),

            // PUA mappings - Undo/Redo
            '\u{E030}' => Some(NormalizedKey::Undo),
            '\u{E031}' => Some(NormalizedKey::Redo),

            // PUA mappings - History
            '\u{E040}' => Some(NormalizedKey::Up),
            '\u{E041}' => Some(NormalizedKey::Down),

            // Regular characters (not control characters)
            c if !c.is_control() => Some(NormalizedKey::Char(c)),

            _ => None,
        }
    }
}

#[cfg(all(target_arch = "wasm32", target_os = "unknown"))]
impl InputNormalizer for BrowserNormalizer {
    type InputType = String;

    fn feed(&mut self, input: String) {
        self.input_queue.push_back(input);
    }

    fn normalize(&mut self) -> Vec<NormalizedKey> {
        let mut keys = Vec::new();

        let mut raw_input = String::new();
        while let Some(s) = self.input_queue.pop_front() {
            raw_input.push_str(&s);
        }

        if !raw_input.is_empty() {
            let clean_input = self.parse_sequences(&raw_input);

            for c in clean_input.chars() {
                if let Some(key) = self.map_char(c) {
                    keys.push(key);
                }
            }
        }

        keys
    }
}



// ============================================================================
// TESTS
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[cfg(target_arch = "wasm32")]
    use wasm_bindgen_test::*;

    #[cfg(target_arch = "wasm32")]
    wasm_bindgen_test_configure!(run_in_browser);

    // Helper to create a browser normalizer with test input
    #[cfg(all(target_arch = "wasm32", target_os = "unknown"))]
    fn make_browser_normalizer(inputs: Vec<&str>) -> BrowserNormalizer {
        let mut input_queue = VecDeque::new();
        for input in inputs {
            input_queue.push_back(input.to_string());
        }
        BrowserNormalizer{input_queue}
    }

    #[cfg(all(target_arch = "wasm32", target_os = "unknown"))]
    #[cfg_attr(all(target_arch = "wasm32", target_os = "unknown"), wasm_bindgen_test)]
    fn test_browser_basic_characters() {
        let mut normalizer = make_browser_normalizer(vec!["hello"]);
        let keys = normalizer.normalize();

        assert_eq!(keys.len(), 5);
        assert_eq!(keys[0], NormalizedKey::Char('h'));
        assert_eq!(keys[1], NormalizedKey::Char('e'));
        assert_eq!(keys[2], NormalizedKey::Char('l'));
        assert_eq!(keys[3], NormalizedKey::Char('l'));
        assert_eq!(keys[4], NormalizedKey::Char('o'));
    }

    #[cfg(all(target_arch = "wasm32", target_os = "unknown"))]
    #[cfg_attr(all(target_arch = "wasm32", target_os = "unknown"), wasm_bindgen_test)]
    fn test_browser_enter_variations() {
        let mut normalizer1 = make_browser_normalizer(vec!["\r"]);
        assert_eq!(normalizer1.normalize(), vec![NormalizedKey::Enter]);

        let mut normalizer2 = make_browser_normalizer(vec!["\n"]);
        assert_eq!(normalizer2.normalize(), vec![NormalizedKey::Enter]);
    }

    #[cfg(all(target_arch = "wasm32", target_os = "unknown"))]
    #[cfg_attr(all(target_arch = "wasm32", target_os = "unknown"), wasm_bindgen_test)]
    fn test_browser_arrow_keys_normal_mode() {
        let mut normalizer = make_browser_normalizer(vec!["\x1b[D"]);
        assert_eq!(normalizer.normalize(), vec![NormalizedKey::Left]);

        let mut normalizer = make_browser_normalizer(vec!["\x1b[C"]);
        assert_eq!(normalizer.normalize(), vec![NormalizedKey::Right]);

        let mut normalizer = make_browser_normalizer(vec!["\x1b[A"]);
        assert_eq!(normalizer.normalize(), vec![NormalizedKey::Up]);

        let mut normalizer = make_browser_normalizer(vec!["\x1b[B"]);
        assert_eq!(normalizer.normalize(), vec![NormalizedKey::Down]);
    }

    #[cfg(all(target_arch = "wasm32", target_os = "unknown"))]
    #[cfg_attr(all(target_arch = "wasm32", target_os = "unknown"), wasm_bindgen_test)]
    fn test_browser_arrow_keys_application_mode() {
        let mut normalizer = make_browser_normalizer(vec!["\x1bOD"]);
        assert_eq!(normalizer.normalize(), vec![NormalizedKey::Left]);

        let mut normalizer = make_browser_normalizer(vec!["\x1bOC"]);
        assert_eq!(normalizer.normalize(), vec![NormalizedKey::Right]);

        let mut normalizer = make_browser_normalizer(vec!["\x1bOA"]);
        assert_eq!(normalizer.normalize(), vec![NormalizedKey::Up]);

        let mut normalizer = make_browser_normalizer(vec!["\x1bOB"]);
        assert_eq!(normalizer.normalize(), vec![NormalizedKey::Down]);
    }

    #[cfg(all(target_arch = "wasm32", target_os = "unknown"))]
    #[cfg_attr(all(target_arch = "wasm32", target_os = "unknown"), wasm_bindgen_test)]
    fn test_browser_home_end_variations() {
        // Home variations
        let mut normalizer = make_browser_normalizer(vec!["\x1b[H"]);
        assert_eq!(normalizer.normalize(), vec![NormalizedKey::Home]);

        let mut normalizer = make_browser_normalizer(vec!["\x1bOH"]);
        assert_eq!(normalizer.normalize(), vec![NormalizedKey::Home]);

        let mut normalizer = make_browser_normalizer(vec!["\x1b[1~"]);
        assert_eq!(normalizer.normalize(), vec![NormalizedKey::Home]);

        // End variations
        let mut normalizer = make_browser_normalizer(vec!["\x1b[F"]);
        assert_eq!(normalizer.normalize(), vec![NormalizedKey::End]);

        let mut normalizer = make_browser_normalizer(vec!["\x1bOF"]);
        assert_eq!(normalizer.normalize(), vec![NormalizedKey::End]);

        let mut normalizer = make_browser_normalizer(vec!["\x1b[4~"]);
        assert_eq!(normalizer.normalize(), vec![NormalizedKey::End]);
    }

    #[cfg(all(target_arch = "wasm32", target_os = "unknown"))]
    #[cfg_attr(all(target_arch = "wasm32", target_os = "unknown"), wasm_bindgen_test)]
    fn test_browser_delete_and_backspace() {
        let mut normalizer = make_browser_normalizer(vec!["\x1b[3~"]);
        assert_eq!(normalizer.normalize(), vec![NormalizedKey::Delete]);

        let mut normalizer = make_browser_normalizer(vec!["\x7f"]);
        assert_eq!(normalizer.normalize(), vec![NormalizedKey::Backspace]);
    }

    #[cfg(all(target_arch = "wasm32", target_os = "unknown"))]
    #[cfg_attr(all(target_arch = "wasm32", target_os = "unknown"), wasm_bindgen_test)]
    fn test_browser_ctrl_navigation() {
        // Ctrl+Left variations
        let mut normalizer = make_browser_normalizer(vec!["\x1b[1;5D"]);
        assert_eq!(normalizer.normalize(), vec![NormalizedKey::CtrlLeft]);

        let mut normalizer = make_browser_normalizer(vec!["\x1b[5D"]);
        assert_eq!(normalizer.normalize(), vec![NormalizedKey::CtrlLeft]);

        // Ctrl+Right variations
        let mut normalizer = make_browser_normalizer(vec!["\x1b[1;5C"]);
        assert_eq!(normalizer.normalize(), vec![NormalizedKey::CtrlRight]);

        let mut normalizer = make_browser_normalizer(vec!["\x1b[5C"]);
        assert_eq!(normalizer.normalize(), vec![NormalizedKey::CtrlRight]);
    }

    #[cfg(all(target_arch = "wasm32", target_os = "unknown"))]
    #[cfg_attr(all(target_arch = "wasm32", target_os = "unknown"), wasm_bindgen_test)]
    fn test_browser_ctrl_editing() {
        // Ctrl+Backspace variations
        let mut normalizer = make_browser_normalizer(vec!["\x17"]); // ^W
        assert_eq!(normalizer.normalize(), vec![NormalizedKey::CtrlBackspace]);

        let mut normalizer = make_browser_normalizer(vec!["\x1b\x7f"]); // Alt+Bksp
        assert_eq!(normalizer.normalize(), vec![NormalizedKey::CtrlBackspace]);

        let mut normalizer = make_browser_normalizer(vec!["\x1b\x08"]); // Alt+^H
        assert_eq!(normalizer.normalize(), vec![NormalizedKey::CtrlBackspace]);

        let mut normalizer = make_browser_normalizer(vec!["\x08"]); // Ctrl+H
        assert_eq!(normalizer.normalize(), vec![NormalizedKey::CtrlBackspace]);

        // Ctrl+Delete variations
        let mut normalizer = make_browser_normalizer(vec!["\x1b[3;5~"]);
        assert_eq!(normalizer.normalize(), vec![NormalizedKey::CtrlDelete]);

        let mut normalizer = make_browser_normalizer(vec!["\x1b[3^"]);
        assert_eq!(normalizer.normalize(), vec![NormalizedKey::CtrlDelete]);
    }

    #[cfg(all(target_arch = "wasm32", target_os = "unknown"))]
    #[cfg_attr(all(target_arch = "wasm32", target_os = "unknown"), wasm_bindgen_test)]
    fn test_browser_undo_redo() {
        let mut normalizer = make_browser_normalizer(vec!["\x1a"]); // Ctrl+Z
        assert_eq!(normalizer.normalize(), vec![NormalizedKey::Undo]);

        let mut normalizer = make_browser_normalizer(vec!["\x19"]); // Ctrl+Y
        assert_eq!(normalizer.normalize(), vec![NormalizedKey::Redo]);
    }

    #[cfg(all(target_arch = "wasm32", target_os = "unknown"))]
    #[cfg_attr(all(target_arch = "wasm32", target_os = "unknown"), wasm_bindgen_test)]
    fn test_browser_ctrl_c() {
        let mut normalizer = make_browser_normalizer(vec!["\x03"]);
        assert_eq!(normalizer.normalize(), vec![NormalizedKey::CtrlC]);
    }

    #[cfg(all(target_arch = "wasm32", target_os = "unknown"))]
    #[cfg_attr(all(target_arch = "wasm32", target_os = "unknown"), wasm_bindgen_test)]
    fn test_browser_mixed_input() {
        let mut normalizer = make_browser_normalizer(vec!["ls", "\x1b[D", "a"]);
        let keys = normalizer.normalize();

        assert_eq!(keys.len(), 4);
        assert_eq!(keys[0], NormalizedKey::Char('l'));
        assert_eq!(keys[1], NormalizedKey::Char('s'));
        assert_eq!(keys[2], NormalizedKey::Left);
        assert_eq!(keys[3], NormalizedKey::Char('a'));
    }

    #[cfg(all(target_arch = "wasm32", target_os = "unknown"))]
    #[cfg_attr(all(target_arch = "wasm32", target_os = "unknown"), wasm_bindgen_test)]
    fn test_browser_control_chars_filtered() {
        // Control characters that don't map should be filtered out
        let mut normalizer = make_browser_normalizer(vec!["\x01\x02hello\x04"]);
        let keys = normalizer.normalize();

        // Should only get the printable characters
        assert_eq!(keys.len(), 5);
        assert_eq!(keys[0], NormalizedKey::Char('h'));
        assert_eq!(keys[4], NormalizedKey::Char('o'));
    }

    #[cfg(all(target_arch = "wasm32", target_os = "unknown"))]
    #[cfg_attr(all(target_arch = "wasm32", target_os = "unknown"), wasm_bindgen_test)]
    fn test_browser_empty_input() {
        let mut normalizer = make_browser_normalizer(vec![]);
        let keys = normalizer.normalize();
        assert_eq!(keys.len(), 0);
    }

    #[cfg(all(target_arch = "wasm32", target_os = "unknown"))]
    #[cfg_attr(all(target_arch = "wasm32", target_os = "unknown"), wasm_bindgen_test)]
    fn test_browser_sequential_sequences() {
        // Test multiple escape sequences in sequence
        let mut normalizer = make_browser_normalizer(vec!["\x1b[D\x1b[C\x1b[A\x1b[B"]);
        let keys = normalizer.normalize();

        assert_eq!(keys.len(), 4);
        assert_eq!(keys[0], NormalizedKey::Left);
        assert_eq!(keys[1], NormalizedKey::Right);
        assert_eq!(keys[2], NormalizedKey::Up);
        assert_eq!(keys[3], NormalizedKey::Down);
    }

    // Note: Native/WASI tests would require mocking crossterm::event,
    // which is challenging. In practice, you'd test the integration
    // with the actual terminal, or use dependency injection for testing.
    // For this example, we've focused on the browser implementation
    // which has deterministic string parsing we can test thoroughly.
}
