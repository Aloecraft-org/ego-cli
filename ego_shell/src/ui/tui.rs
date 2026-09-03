use crate::{Command, Response, ShellContext, StatusInfo};
use std::error::Error;
use std::io;

#[cfg(not(target_arch = "wasm32"))]
use crossterm::event::EventStream;

#[cfg(all(target_arch = "wasm32", target_os = "unknown"))]
use ratatui_xterm_js::EventStream;

// FIX: Allow these to be used on ALL targets (WASM now has them via the cleaned dependency)
#[cfg(not(all(target_arch = "wasm32", target_env = "p2")))]
use crossterm::event::{Event, KeyCode, KeyEventKind};

#[cfg(not(all(target_arch = "wasm32", target_env = "p2")))]
use futures::StreamExt;
#[cfg(not(all(target_arch = "wasm32", target_env = "p2")))]
use ratatui::prelude::*;
#[cfg(not(all(target_arch = "wasm32", target_env = "p2")))]
use ratatui::widgets::*;

#[cfg(all(target_arch = "wasm32", not(target_env = "p2")))]
use ratatui_xterm_js::{TerminalHandle, XtermJsBackend, init_terminal};
#[cfg(all(target_arch = "wasm32", target_os="unknown"))]
use wasm_bindgen::prelude::*;

#[cfg(not(all(target_arch = "wasm32", target_env = "p2")))]
struct TuiState {
    input: String,
    output_history: Vec<String>,
    status: Option<StatusInfo>,
}

#[cfg(all(target_arch = "wasm32", not(target_env = "p2")))]
pub async fn run_wasm_tui() -> Result<(), JsValue> {
    console_error_panic_hook::set_once();
    
    let elem = web_sys::window()
        .unwrap()
        .document()
        .unwrap()
        .get_element_by_id("terminal")
        .unwrap();

    init_terminal(
        ratatui_xterm_js::xterm::TerminalOptions::new()
            .with_rows(30)
            .with_cursor_blink(true)
            .with_font_size(14)
            .with_font_family("'Fira Code', monospace"),
        elem.dyn_into()?,
    );

    let handle = TerminalHandle::default();
    
    // In WASM, we create the context here because this is the entry point
    let ctx = crate::setup().await;
    
    run(ctx, handle, XtermJsBackend::new).await
        .map_err(|e| JsValue::from_str(&e.to_string()))?;
    Ok(())
}

#[cfg(not(all(target_arch = "wasm32", target_env = "p2")))]
pub async fn run_native(ctx: ShellContext) -> Result<(), Box<dyn Error>> {
    let stdout = io::stdout();
    run(ctx, stdout, ratatui::backend::CrosstermBackend::new).await
}

#[cfg(not(all(target_arch = "wasm32", target_env = "p2")))]
async fn run<W, F, B>(mut ctx: ShellContext, mut out: W, create_backend: F) -> Result<(), Box<dyn Error>>
where
    W: io::Write,
    B: ratatui::backend::Backend + io::Write,
    F: FnOnce(W) -> B,
{
    crossterm::terminal::enable_raw_mode()
        .map_err(|e| io::Error::new(io::ErrorKind::Other, e))?;
    crossterm::execute!(out, crossterm::terminal::EnterAlternateScreen)
        .map_err(|e| io::Error::new(io::ErrorKind::Other, e))?;
    
    let backend = create_backend(out);
    let mut terminal = ratatui::Terminal::new(backend)?;

    let mut state = TuiState {
        input: String::new(),
        output_history: vec!["EgoShell TUI v0.1.0".to_string()],
        status: None,
    };

    // Request initial status
    ctx.cmd_tx.send(Command::Status).ok();

    let mut events = EventStream::default();
    
    loop {
        terminal.draw(|f| ui(f, &state))?;

        tokio::select! {
            event = events.next() => {
                if let Some(Ok(Event::Key(key))) = event {
                    if key.kind == KeyEventKind::Press {
                        match key.code {
                            KeyCode::Char(c) => {
                                state.input.push(c);
                            },
                            KeyCode::Backspace => { state.input.pop(); },
                            KeyCode::Enter => {
                                let input = state.input.clone();
                                state.input.clear();
                                
                                if let Some(cmd) = parse_command(&input) {
                                    let is_exit = matches!(cmd, Command::Exit);
                                    state.output_history.push(format!("> {}", input));
                                    ctx.cmd_tx.send(cmd).ok();
                                    if is_exit {
                                        break;
                                    }
                                } else {
                                    state.output_history.push(format!("> {} (unknown)", input));
                                }
                                
                                // Keep history manageable
                                if state.output_history.len() > 100 {
                                    state.output_history.remove(0);
                                }
                            }
                            KeyCode::Esc => break,
                            _ => {}
                        }
                    }
                }
            }
            
            Ok(resp) = ctx.resp_rx.recv() => {
                match resp {
                    Response::Output(text) => {
                        state.output_history.push(text);
                    }
                    Response::Status(info) => {
                        state.status = Some(info);
                    }
                    Response::Error(err) => {
                        state.output_history.push(format!("ERROR: {}", err));
                    }
                    Response::Exit(msg) => {
                        state.output_history.push(msg);
                        break;
                    }
                }
                
                if state.output_history.len() > 100 {
                    state.output_history.remove(0);
                }
            }
        }
    }

    crossterm::terminal::disable_raw_mode()
        .map_err(|e| io::Error::new(io::ErrorKind::Other, e))?;
    crossterm::execute!(
        terminal.backend_mut(),
        crossterm::terminal::LeaveAlternateScreen
    )?;
    
    Ok(())
}

#[cfg(not(all(target_arch = "wasm32", target_env = "p2")))]
fn parse_command(line: &str) -> Option<Command> {
    let parts: Vec<&str> = line.trim().split_whitespace().collect();
    match parts.first()? {
        &"echo" => Some(Command::Echo { text: parts[1..].join(" ") }),
        &"decho" => {
            let secs = parts.get(parts.len() - 1)?.parse().ok()?;
            let text = parts[1..parts.len() - 1].join(" ");
            Some(Command::DelayedEcho { text, delay_secs: secs })
        }
        &"status" => Some(Command::Status),
        &"exit" => Some(Command::Exit),
        _ => None,
    }
}

#[cfg(not(all(target_arch = "wasm32", target_env = "p2")))]
fn ui(f: &mut Frame, state: &TuiState) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Min(5),      // Output
            Constraint::Length(5),   // Status
            Constraint::Length(3),   // Input
        ])
        .split(f.area());

    // Output history
    let output_text: Vec<Line> = state.output_history
        .iter()
        .map(|s| Line::from(s.as_str()))
        .collect();
    
    let output_para = Paragraph::new(output_text)
        .block(Block::default()
            .borders(Borders::ALL)
            .title("Output"))
        .scroll((state.output_history.len().saturating_sub(chunks[0].height as usize - 2) as u16, 0));
    f.render_widget(output_para, chunks[0]);

    // Status panel
    let status_text = if let Some(ref info) = state.status {
        format!(
            "Uptime: {}s | Commands: {} | Platform: {}",
            info.uptime_secs,
            info.commands_processed,
            info.platform
        )
    } else {
        "No status available".to_string()
    };
    
    let status_para = Paragraph::new(status_text)
        .block(Block::default()
            .borders(Borders::ALL)
            .title("Status"));
    f.render_widget(status_para, chunks[1]);

    // Input
    let input_para = Paragraph::new(state.input.as_str())
        .block(Block::default()
            .borders(Borders::ALL)
            .title("Input (ESC to exit)"));
    f.render_widget(input_para, chunks[2]);

    let input_x = chunks[2].x + 1;
    let input_y = chunks[2].y + 1;

    f.set_cursor_position((input_x + state.input.len() as u16, input_y));
}