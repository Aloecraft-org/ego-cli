use crate::command::{Command, Response, StatusInfo};
use aloeplatform;
use tokio::sync::{broadcast, mpsc};

pub async fn command_processor(
    mut cmd_rx: mpsc::UnboundedReceiver<Command>,
    resp_tx: broadcast::Sender<Response>,
) {
    let start_time = aloeplatform::time::SystemTime::now()
        .duration_since(aloeplatform::time::UNIX_EPOCH)
        .unwrap_or(std::time::Duration::ZERO)
        .as_secs();

    let mut commands_processed = 0u64;

    while let Some(cmd) = cmd_rx.recv().await {
        let is_exit = matches!(cmd, Command::Exit);
        commands_processed += 1;

        let response = match cmd {
            Command::Echo { text } => Response::Output(text),

            Command::DelayedEcho { text, delay_secs } => {
                aloeplatform::time::sleep(std::time::Duration::from_secs(delay_secs)).await;
                Response::Output(text)
            }

            Command::Status => {
                let now_duration = aloeplatform::time::SystemTime::now()
                    .duration_since(aloeplatform::time::UNIX_EPOCH)
                    .unwrap_or(std::time::Duration::ZERO)
                    .as_secs();
                let uptime = now_duration.saturating_sub(start_time);

                Response::Status(StatusInfo {
                    uptime_secs: uptime,
                    commands_processed,
                    platform: platform_string(),
                    timestamp: now_duration,
                })
            }
            Command::Exit => Response::Exit("Goodbye".to_string()),
            Command::Unknown(line) => Response::Output(format!("Unknown command: {}", line)),
        };

        resp_tx.send(response).ok();
        
        if is_exit {
            break;
        }
        tokio::task::yield_now().await;
    }
}

fn platform_string() -> String {
    #[cfg(all(target_arch = "wasm32", target_os = "unknown"))]
    return "wasm32-unknown-unknown".to_string();

    #[cfg(all(target_arch = "wasm32", target_env = "p2"))]
    return "wasm32-wasip2".to_string();

    #[cfg(not(target_arch = "wasm32"))]
    return format!("{}-{}", std::env::consts::ARCH, std::env::consts::OS);
}