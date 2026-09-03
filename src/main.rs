//! A prompt built out of `ego_cli`, on whichever platform it was compiled
//! for.
//!
//! # Surface
//!
//! Entry points: `main` on native and WASI; `start` in the browser, called
//! from JavaScript with the page's xterm.js terminal. `run` is the body all
//! three share.
//!
//! Configurable values: `COMMANDS`, which is both the completion list and
//! the highlighter's idea of a valid command.
//!
//! Fan-out points: the `match` in `dispatch` is the command table.
//!
//! Run it:
//!
//! ```text
//! cargo run                                  # native
//! cargo run --target wasm32-wasip2           # WASI, under wasmtime
//! trunk serve www/index.html                 # browser
//! ```

use std::borrow::Cow;

use ego_cli::extend::{Highlighter, WordCompleter};
use ego_cli::style::{self, Color};
use ego_cli::{ReadOutcome, Session, Terminal};

/// The commands this demo knows: what Tab offers, and what the highlighter
/// paints green.
const COMMANDS: &[&str] = &["caps", "echo", "exit", "help", "history"];

#[cfg(not(target_arch = "wasm32"))]
#[tokio::main]
async fn main() {
    if let Err(error) = run(ego_cli::term::platform().expect("open terminal")).await {
        eprintln!("ego-cli: {error}");
        std::process::exit(1);
    }
}

// WASI Preview 2 is single-threaded: there is no thread to move a task to.
#[cfg(all(target_arch = "wasm32", target_env = "p2"))]
#[tokio::main(flavor = "current_thread")]
async fn main() {
    if let Err(error) = run(ego_cli::term::platform().expect("open terminal")).await {
        eprintln!("ego-cli: {error}");
    }
}

// The browser has no `main` to speak of: the page calls `start` once it has
// an xterm.js terminal to hand over.
#[cfg(all(target_arch = "wasm32", target_os = "unknown"))]
fn main() {}

/// Take over `terminal` and run the prompt. Called from JavaScript:
///
/// ```js
/// import init, { start } from './ego-cli-demo.js';
/// await init();
/// await start(term);
/// ```
#[cfg(all(target_arch = "wasm32", target_os = "unknown"))]
#[wasm_bindgen::prelude::wasm_bindgen]
pub async fn start(terminal: wasm_bindgen::JsValue) -> Result<(), wasm_bindgen::JsValue> {
    use ego_cli::term::browser::XtermTerminal;
    run(XtermTerminal::attach(terminal))
        .await
        .map_err(|error| wasm_bindgen::JsValue::from_str(&error.to_string()))
}

async fn run<T: Terminal>(terminal: T) -> ego_cli::Result<()> {
    ego_platform::init();

    let mut session = Session::new(terminal);
    session.set_prompt(format!("{}> ", style::paint("ego", Color::Green)));
    session.set_completer(WordCompleter::new(COMMANDS.iter().copied()));
    session.set_highlighter(CommandHighlighter);

    let capabilities = session.capabilities();
    let greeting = if capabilities.raw_mode {
        "ego-cli demo. Tab completes, Up recalls, Ctrl+Z undoes. `help` for more.\n"
    } else {
        "ego-cli demo. This platform has no raw mode, so this is a plain \
         line reader: history is recorded but not walkable. `help` for more.\n"
    };
    session.print(greeting).await?;

    loop {
        match session.read_line().await? {
            ReadOutcome::Line(line) => {
                if dispatch(&mut session, &line).await? {
                    break;
                }
            }
            // Ctrl+C: the line is gone, the session is not.
            ReadOutcome::Interrupted => continue,
            ReadOutcome::Eof => {
                session.print("\n").await?;
                break;
            }
        }
    }
    Ok(())
}

/// Run one command. Returns whether the session should end.
async fn dispatch<T: Terminal>(session: &mut Session<T>, line: &str) -> ego_cli::Result<bool> {
    let line = line.trim();
    let (command, rest) = match line.split_once(char::is_whitespace) {
        Some((command, rest)) => (command, rest.trim()),
        None => (line, ""),
    };

    match command {
        "" => {}
        "exit" => return Ok(true),
        "echo" => session.print(&format!("{rest}\n")).await?,
        "help" => {
            session
                .print(&format!("commands: {}\n", COMMANDS.join(", ")))
                .await?
        }
        "history" => {
            let listing: String = session
                .history()
                .entries()
                .enumerate()
                .map(|(index, entry)| format!("{:>4}  {entry}\n", index + 1))
                .collect();
            session.print(&listing).await?;
        }
        "caps" => {
            let capabilities = session.capabilities();
            let size = session.terminal().size();
            session
                .print(&format!(
                    "raw mode: {}\nansi: {}\nresize events: {}\nsize: {}x{}\n",
                    capabilities.raw_mode,
                    capabilities.ansi,
                    capabilities.resize_events,
                    size.cols,
                    size.rows,
                ))
                .await?
        }
        other => {
            session
                .print(&format!("unknown command: {other}\n"))
                .await?
        }
    }
    Ok(false)
}

/// Paints the command word green when this demo knows it, red when it does
/// not — the whole of what a [`Highlighter`] has to do.
struct CommandHighlighter;

impl Highlighter for CommandHighlighter {
    fn highlight<'l>(&self, line: &'l str) -> Cow<'l, str> {
        let start = line.len() - line.trim_start().len();
        let command = line[start..]
            .split_once(char::is_whitespace)
            .map(|(head, _)| head)
            .unwrap_or(&line[start..]);
        if command.is_empty() {
            return Cow::Borrowed(line);
        }

        let colour = if COMMANDS.contains(&command) {
            Color::Green
        } else {
            Color::Red
        };
        let end = start + command.len();
        Cow::Owned(format!(
            "{}{}{}{}{}",
            &line[..start],
            style::fg(colour),
            command,
            style::fg(Color::Default),
            &line[end..],
        ))
    }
}
