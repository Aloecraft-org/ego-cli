use egoshell::{ui, setup};
use ego2_proto::ego2_shell::Ego2ShellMode;
use std::env;

#[cfg(not(target_arch = "wasm32"))]
#[tokio::main(flavor = "multi_thread")]
async fn main() {
    real_main().await;
}

// WASI needs main to run
#[cfg(all(target_arch = "wasm32", target_env = "p2"))]
#[tokio::main(flavor = "current_thread")]
async fn main() {
    real_main().await;
}

// Browser (unknown-unknown) should have an empty main.
// The logic is driven by the exported 'startShell' function in wasm.rs
#[cfg(all(target_arch = "wasm32", target_os = "unknown"))]
fn main() {}

async fn real_main() {
    let args: Vec<String> = env::args().collect();
    let mut mode = Ego2ShellMode::Tui; // Default

    // Simple arg parsing
    for i in 0..args.len() {
        if args[i] == "--shell" && i + 1 < args.len() {
            mode = match args[i+1].as_str() {
                "tui" => Ego2ShellMode::Tui,
                "lite" => Ego2ShellMode::Lite,
                "none" => Ego2ShellMode::None,
                _ => Ego2ShellMode::Tui,
            };
        }
    }

    // WASI P2 Fallback: TUI is not compiled there, so we force Lite
    #[cfg(all(target_arch = "wasm32", target_env = "p2"))]
    if matches!(mode, Ego2ShellMode::Tui) {
        println!("TUI not supported on this platform, falling back to Lite.");
        mode = Ego2ShellMode::Lite;
    }

    let ctx = setup().await;

    match mode {
        Ego2ShellMode::Tui => {
            // We must gate this call because run_native doesn't exist on WASI P2
            #[cfg(not(all(target_arch = "wasm32", target_env = "p2")))]
            {
                 if let Err(e) = ui::tui::run_native(ctx).await {
                    eprintln!("TUI Error: {}", e);
                 }
            }
            #[cfg(all(target_arch = "wasm32", target_env = "p2"))]
            {
                ui::cli::run(ctx).await;
            }
        },
        Ego2ShellMode::Lite => {
            ui::cli::run(ctx).await;
        },
        Ego2ShellMode::None => {
            loop { aloeplatform::sleep(std::time::Duration::from_secs(60)).await; }
        }
    }
}