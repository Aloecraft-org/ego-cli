use crate::{Command, Response, ShellContext};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use aloeplatform;

pub async fn run(mut ctx: ShellContext) {
    println!("EgoShell v0.1.0 (Lite)");
    println!("Commands: echo <text> | decho <text> <secs> | status | exit");

    #[cfg(not(all(target_arch = "wasm32", target_os = "unknown")))]
    {
        let stdin = aloeplatform::io::stdin();
        let mut stdout = aloeplatform::io::stdout();
        let mut lines = BufReader::new(stdin).lines();

        stdout.write_all(b"> ").await.ok();
        stdout.flush().await.ok();

        loop {
            tokio::select! {
                biased;

                Ok(resp) = ctx.resp_rx.recv() => {
                    print_response(resp);
                    stdout.write_all(b"> ").await.ok();
                    stdout.flush().await.ok();
                }

                line = lines.next_line() => {
                    match line {
                        Ok(Some(line)) => {
                            if let Some(cmd) = parse_command(&line) {
                                let is_exit = matches!(cmd, Command::Exit);
                                ctx.cmd_tx.send(cmd).ok();

                                if is_exit {
                                    if let Ok(resp) = ctx.resp_rx.recv().await {
                                        print_response(resp);
                                    }
                                    break;
                                }
                            }
                        }
                        Ok(None) | Err(_) => {
                            break;
                        }
                    }
                    stdout.write_all(b"> ").await.ok();
                    stdout.flush().await.ok();
                }
            }
        }
    }

    #[cfg(all(target_arch = "wasm32", target_os = "unknown"))]
    {
        // Browser mode: just listen for responses (input comes via JS bridge)
        while let Ok(resp) = ctx.resp_rx.recv().await {
            print_response(resp);
        }
    }
}

fn parse_command(line: &str) -> Option<Command> {
    let parts: Vec<&str> = line.trim().split_whitespace().collect();
    if parts.is_empty() {
        return None;
    }

    match parts.first()? {
        &"echo" => Some(Command::Echo {
            text: parts[1..].join(" "),
        }),
        &"decho" => {
            let secs = parts.get(parts.len() - 1)?.parse().ok()?;
            let text = parts[1..parts.len() - 1].join(" ");
            Some(Command::DelayedEcho {
                text,
                delay_secs: secs,
            })
        }
        &"status" => Some(Command::Status),
        &"exit" => Some(Command::Exit),
        _ => Some(Command::Unknown(line.to_string())),
    }
}

fn print_response(resp: Response) {
    match resp {
        Response::Output(text) => println!("{}", text),
        Response::Status(info) => println!("{:#?}", info),
        Response::Error(err) => eprintln!("Error: {}", err),
        Response::Exit(text) => println!("{}", text),
    }
}