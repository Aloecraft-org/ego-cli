#[cfg(all(target_arch = "wasm32", target_os = "unknown"))]
use wasm_bindgen::prelude::*;

#[cfg(all(target_arch = "wasm32", target_os = "unknown"))]
use crate::{Command, Response, setup, ui};

#[cfg(all(target_arch = "wasm32", target_os = "unknown"))]
use tokio::sync::mpsc;

#[cfg(all(target_arch = "wasm32", target_os = "unknown"))]
use std::sync::OnceLock;

// Global singleton for the command sender (Lite mode only)
#[cfg(all(target_arch = "wasm32", target_os = "unknown"))]
static SHELL_TX: OnceLock<mpsc::UnboundedSender<Command>> = OnceLock::new();

#[cfg(all(target_arch = "wasm32", target_os = "unknown"))]
thread_local! {
    pub static WASM_TERMINAL: std::cell::RefCell<Option<wasm_term::Terminal>> = std::cell::RefCell::new(None);
    pub static WASM_FIT_ADDON: std::cell::RefCell<Option<wasm_term::FitAddon>> = std::cell::RefCell::new(None);
    pub static WASM_CLOSURE: std::cell::RefCell<Option<Closure<dyn FnMut(String)>>> = std::cell::RefCell::new(None);
    pub static WASM_KEY_HANDLER: std::cell::RefCell<Option<Closure<dyn FnMut(JsValue) -> bool>>> = std::cell::RefCell::new(None);
}

#[cfg(all(target_arch = "wasm32", target_os = "unknown"))]
pub mod wasm_term {
    use wasm_bindgen::prelude::*;
    use web_sys::Element;

    #[wasm_bindgen(module = "xterm")]
    extern "C" {
        pub type Terminal;
        #[wasm_bindgen(constructor)]
        pub fn new(options: &JsValue) -> Terminal;
        #[wasm_bindgen(method)]
        pub fn open(this: &Terminal, parent: &Element);
        #[wasm_bindgen(method)]
        pub fn write(this: &Terminal, data: &str);
        #[wasm_bindgen(method, js_name = onData)]
        pub fn on_data(this: &Terminal, cb: &Closure<dyn FnMut(String)>);
        #[wasm_bindgen(method)]
        pub fn attachCustomKeyEventHandler(
            this: &Terminal,
            handler: &Closure<dyn FnMut(JsValue) -> bool>,
        );
        #[wasm_bindgen(method)]
        pub fn loadAddon(this: &Terminal, addon: &JsValue);
        #[wasm_bindgen(method)]
        pub fn clear(this: &Terminal);
    }

    #[wasm_bindgen(module = "@xterm/addon-fit")]
    extern "C" {
        pub type FitAddon;
        #[wasm_bindgen(constructor)]
        pub fn new() -> FitAddon;
        #[wasm_bindgen(method)]
        pub fn fit(this: &FitAddon);
    }

    #[wasm_bindgen(module = "xterm-addon-unicode11")]
    extern "C" {
        pub type Unicode11Addon;
        #[wasm_bindgen(constructor)]
        pub fn new() -> Unicode11Addon;
    }
}



#[cfg(all(target_arch = "wasm32", target_os = "unknown"))]
#[wasm_bindgen(js_name = startShell)]
pub async fn start_shell(mode: String) -> Result<(), JsValue> {
    console_error_panic_hook::set_once();
    
    match mode.as_str() {
        "tui" => {
            // Checkpoint 5: Dispatch to TUI module
            ui::tui::run_wasm_tui().await?;
        }
        "lite" | _ => {
            // Default to Lite
            init_cli_lite().await;
        }
    }
    Ok(())
}

#[cfg(all(target_arch = "wasm32", target_os = "unknown"))]
#[wasm_bindgen(js_name = sendInput)]
pub fn send_input(input: String) {
    if let Some(tx) = SHELL_TX.get() {
        if let Some(cmd) = parse_command(&input) {
            tx.send(cmd).ok();
        }
    } else {
        web_sys::console::error_1(&"Shell not initialized!".into());
    }
}

// Renamed from init_shell to be internal
#[cfg(all(target_arch = "wasm32", target_os = "unknown"))]
async fn init_cli_lite() {
    // Safety check
    if SHELL_TX.get().is_some() { return; }
    
    let ctx = setup().await;
    let (cmd_tx, mut resp_rx) = (ctx.cmd_tx, ctx.resp_rx);
    
    let _ = SHELL_TX.set(cmd_tx);
    
    // print_to_xterm("> ");
    
    wasm_bindgen_futures::spawn_local(async move {
        while let Ok(resp) = resp_rx.recv().await {
            let output = match resp {
                Response::Output(text) => text,
                Response::Status(info) => format!("{:#?}", info),
                Response::Error(err) => format!("Error: {}", err),
                Response::Exit(msg) => msg,
            };
            
            web_sys::console::log_1(&wasm_bindgen::JsValue::from_str(&output));
            
            let xterm_output = output.replace("\n", "\r\n");
            print_line_to_xterm(&xterm_output);
            print_to_xterm("> ");
        }
    });
}

// ... Helper functions (print_line_to_xterm, etc) remain unchanged ...
// ... I will not output them unless requested as they are existing helpers ...

#[cfg(all(target_arch = "wasm32", target_os = "unknown"))]
fn print_line_to_xterm(text: &str) {
    if let Some(window) = web_sys::window() {
        if let Ok(term) = js_sys::Reflect::get(&window, &"term".into()) {
            if !term.is_undefined() {
                if let Ok(writeln) = js_sys::Reflect::get(&term, &"writeln".into()) {
                    if let Ok(func) = writeln.dyn_into::<js_sys::Function>() {
                        func.call1(&term, &text.into()).ok();
                    }
                }
            }
        }
    }
}

#[cfg(all(target_arch = "wasm32", target_os = "unknown"))]
fn print_to_xterm(text: &str) {
    if let Some(window) = web_sys::window() {
        if let Ok(term) = js_sys::Reflect::get(&window, &"term".into()) {
            if !term.is_undefined() {
                if let Ok(write) = js_sys::Reflect::get(&term, &"write".into()) {
                    if let Ok(func) = write.dyn_into::<js_sys::Function>() {
                        func.call1(&term, &text.into()).ok();
                    }
                }
            }
        }
    }
}

#[cfg(all(target_arch = "wasm32", target_os = "unknown"))]
fn parse_command(line: &str) -> Option<Command> {
    let parts: Vec<&str> = line.trim().split_whitespace().collect();
    if parts.is_empty() {
        return None;
    }

    match parts.first()? {
        &"echo" => Some(Command::Echo { text: parts[1..].join(" ") }),
        &"decho" => {
            let secs = parts.get(parts.len() - 1)?.parse().ok()?;
            let text = parts[1..parts.len() - 1].join(" ");
            Some(Command::DelayedEcho { text, delay_secs: secs })
        }
        &"status" => Some(Command::Status),
        &"exit" => Some(Command::Exit),
        _ => Some(Command::Unknown(line.to_string())),
    }
}