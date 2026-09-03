#[cfg(not(target_arch = "wasm32"))]
pub use crossterm::event::EventStream;

#[cfg(all(target_arch = "wasm32", target_os = "unknown"))]
pub use ratatui_xterm_js::EventStream;

#[cfg(all(target_arch = "wasm32", target_env = "p2"))]
pub struct EventStream;

#[cfg(all(target_arch = "wasm32", target_env = "p2"))]
impl EventStream { 
    pub fn default() -> Self { Self { } }
    pub fn next(&mut self) -> futures::stream::Next<'_, Self> { 
        unreachable!("EventStream::next() should never be called on WASI P2")
    }
}