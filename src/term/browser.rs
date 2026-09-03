//! The browser terminal: an xterm.js `Terminal` the page already owns.
//!
//! # Surface
//!
//! Entry points: [`XtermTerminal::attach`], [`XtermTerminal::handle`].
//!
//! Configurable values: none; the size comes from xterm.js.
//!
//! Fan-out points: none.
//!
//! # Why the page owns the terminal
//!
//! `ego_shell` imported xterm.js itself, with `#[wasm_bindgen(module =
//! "xterm")]`, which fixed the module specifier, the version, and the
//! bundler for everyone downstream. Here the page constructs its own
//! `Terminal` — with whatever addons, theme and font it wants — and hands
//! the object over:
//!
//! ```js
//! const term = new Terminal({ cursorBlink: true });
//! term.open(document.getElementById('terminal'));
//! await start(term);          // your #[wasm_bindgen] entry point
//! ```
//!
//! Nothing here names a module, so no import is generated and any way of
//! getting an xterm.js `Terminal` works: a bundler, an import map, or a
//! `<script>` tag.
//!
//! # Raw mode for free
//!
//! xterm.js has no line discipline, so `onData` already delivers the escape
//! stream a tty would — which is why [`crate::decode`] exists, and why this
//! is the target with the *fewest* concessions rather than the most.
//! [`set_raw`](Terminal::set_raw) is a no-op that reports success.

use std::collections::VecDeque;

use tokio::sync::mpsc;
use wasm_bindgen::prelude::*;

use crate::decode::AnsiDecoder;
use crate::term::{Capabilities, Event, Size, Terminal};
use crate::{Error, Result};

#[wasm_bindgen]
extern "C" {
    /// An xterm.js `Terminal`, as handed over by the page.
    ///
    /// Declared without a module so that nothing is imported: only the
    /// methods below are ever called, on whatever object the page passes.
    pub type XtermHandle;

    #[wasm_bindgen(method, js_name = write)]
    fn js_write(this: &XtermHandle, data: &str);

    #[wasm_bindgen(method, js_name = onData)]
    fn js_on_data(this: &XtermHandle, callback: &Closure<dyn FnMut(String)>);

    #[wasm_bindgen(method, getter)]
    fn cols(this: &XtermHandle) -> u16;

    #[wasm_bindgen(method, getter)]
    fn rows(this: &XtermHandle) -> u16;
}

/// A [`Terminal`] backed by an xterm.js instance.
pub struct XtermTerminal {
    handle: XtermHandle,
    input: mpsc::UnboundedReceiver<String>,
    decoder: AnsiDecoder,
    pending: VecDeque<crate::key::KeyPress>,
    size: Size,
    /// Kept alive for as long as the terminal is: dropping it unregisters
    /// the `onData` handler.
    _on_data: Closure<dyn FnMut(String)>,
}

impl XtermTerminal {
    /// Take over input and output for an xterm.js `Terminal`.
    ///
    /// `terminal` must be an xterm.js `Terminal` — the object is used
    /// duck-typed, so anything with `write`, `onData`, `cols` and `rows`
    /// works, and anything else will throw when first used.
    pub fn attach(terminal: JsValue) -> Self {
        let handle: XtermHandle = terminal.unchecked_into();
        let (tx, rx) = mpsc::unbounded_channel();

        let on_data = Closure::<dyn FnMut(String)>::new(move |chunk: String| {
            // The receiver outlives the closure; a send that fails means
            // the session is gone and the keystroke has nowhere to go.
            let _ = tx.send(chunk);
        });
        handle.js_on_data(&on_data);

        let size = Size::new(handle.cols(), handle.rows());
        Self {
            handle,
            input: rx,
            decoder: AnsiDecoder::new(),
            pending: VecDeque::new(),
            size,
            _on_data: on_data,
        }
    }

    /// The underlying xterm.js object, for calls this crate does not make.
    pub fn handle(&self) -> &JsValue {
        self.handle.as_ref()
    }

    /// xterm.js reports its size on demand rather than by event, so every
    /// read checks. A page that resizes the terminal (the fit addon on a
    /// window resize, say) is noticed on the next keystroke.
    fn resized(&mut self) -> Option<Size> {
        let current = Size::new(self.handle.cols(), self.handle.rows());
        (current != self.size).then(|| {
            self.size = current;
            current
        })
    }
}

impl Terminal for XtermTerminal {
    fn capabilities(&self) -> Capabilities {
        Capabilities {
            raw_mode: true,
            ansi: true,
            resize_events: true,
            // xterm.js writes exactly what it is given.
            line_discipline: false,
        }
    }

    fn size(&self) -> Size {
        self.size
    }

    fn set_raw(&mut self, _enabled: bool) -> Result<()> {
        Ok(()) // xterm.js has no other mode
    }

    async fn next_event(&mut self) -> Result<Event> {
        loop {
            if let Some(size) = self.resized() {
                return Ok(Event::Resize(size));
            }
            if let Some(key) = self.pending.pop_front() {
                return Ok(Event::Key(key));
            }
            match self.input.recv().await {
                Some(chunk) => self.pending.extend(self.decoder.push(&chunk)),
                // Every sender is gone: the closure was dropped, so no more
                // input can arrive.
                None => return Ok(Event::Eof),
            }
        }
    }

    async fn write(&mut self, text: &str) -> Result<()> {
        self.handle.js_write(text);
        Ok(())
    }

    async fn flush(&mut self) -> Result<()> {
        Ok(()) // xterm.js renders on its own schedule
    }
}

impl From<JsValue> for Error {
    fn from(value: JsValue) -> Self {
        Error::Terminal(
            value
                .as_string()
                .unwrap_or_else(|| format!("{:?}", js_sys::Object::from(value))),
        )
    }
}
