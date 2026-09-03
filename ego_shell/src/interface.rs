use async_trait::async_trait;
use ego2_proto::ego2_shell::Ego2ShellStatus;


/// The Interface Layer.
/// Decouples the presentation (CLI/TUI) from the logic (ClientController).
#[async_trait]
pub trait ShellController: Send + Sync {
    /// Initialize the shell (clear screen, draw static UI).
    async fn init(&mut self);

    /// Handle a single tick/frame.
    /// Returns the desired next state (e.g., keep Running, or Request Pause).
    async fn tick(&mut self) -> Ego2ShellStatus;

    /// Render the UI based on current state.
    async fn render(&mut self);
    
    /// Receive the Command Bridge (connection to the Engine).
    /// This happens when the Engine starts.
    fn attach_command_interface(&mut self, interface: Box<dyn std::any::Any + Send>);
}