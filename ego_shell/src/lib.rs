mod command;
mod processor;
pub mod events;

#[cfg(all(target_arch = "wasm32", target_os = "unknown", not(test)))]
pub mod wasm;

pub use command::{Command, Response, StatusInfo};
pub use processor::command_processor;

pub mod interface; 
pub use interface::ShellController; 

#[cfg(not(all(target_arch = "wasm32", target_os = "unknown", test)))]
pub mod ui;
pub mod input;

// Export the functions directly
#[cfg(all(target_arch = "wasm32", target_os = "unknown", not(test)))]
pub use wasm::{start_shell, send_input};

use aloeplatform;
use tokio::sync::{broadcast, mpsc};

#[cfg(all(target_arch = "wasm32", target_os = "unknown"))]
use wasm_bindgen::prelude::*;

pub struct ShellContext {
    pub cmd_tx: mpsc::UnboundedSender<Command>,
    pub resp_rx: broadcast::Receiver<Response>,
    pub resp_tx: broadcast::Sender<Response>,
}

pub async fn setup() -> ShellContext {
    aloeplatform::init();
    let (cmd_tx, cmd_rx) = mpsc::unbounded_channel();
    let (resp_tx, resp_rx) = broadcast::channel(100);
    
    aloeplatform::spawn(command_processor(cmd_rx, resp_tx.clone()));
    
    ShellContext {
        cmd_tx,
        resp_rx,
        resp_tx,
    }
}

pub async fn start() {
    aloeplatform::init();
    let (cmd_tx, cmd_rx) = mpsc::unbounded_channel();
    let (resp_tx, mut resp_rx) = broadcast::channel(100);
    
    aloeplatform::spawn(command_processor(cmd_rx, resp_tx));
    loop {
        aloeplatform::sleep(std::time::Duration::from_secs(5)).await;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_setup_integration() {
        // Setup the shell environment
        let mut ctx = setup().await;

        // Subscribe to responses
        let mut rx = ctx.resp_tx.subscribe();

        // Send a command
        ctx.cmd_tx.send(Command::Echo { text: "test".into() }).unwrap();

        // Verify response
        // We use a timeout to prevent hanging if logic fails
        let result = aloeplatform::timeout(std::time::Duration::from_secs(1), async {
            loop {
                if let Ok(resp) = rx.recv().await {
                    match resp {
                        Response::Output(s) => {
                            if s == "test" { return true; }
                        }
                        _ => {}
                    }
                }
            }
        }).await;

        assert!(result.is_ok(), "Timed out waiting for response");
        assert!(result.unwrap(), "Did not receive expected output");
    }
}