# ego-cli

[![CI](https://github.com/aloecraft-org/ego-cli/actions/workflows/ci.yml/badge.svg)](https://github.com/aloecraft-org/ego-cli/actions/workflows/ci.yml)

Line editing for command-line applications, on native, WASI Preview 2, and
the browser. History with prefix search, word motions, undo and redo, and
two hooks — completion and highlighting — for a host to plug its own
behaviour into.

It is a library. It has no commands, no protocol, and no opinion about what
your prompt is for: you hand it a terminal, it hands you back lines.

```rust
use ego_cli::{ReadOutcome, Session, term};

let mut session = Session::new(term::platform()?);
session.set_prompt("diluvium> ");

loop {
    match session.read_line().await? {
        ReadOutcome::Line(line) => run_command(&line).await,
        ReadOutcome::Interrupted => continue,   // Ctrl+C
        ReadOutcome::Eof => break,              // Ctrl+D
    }
}
```

## What the platform decides

Every convenience here needs the terminal to hand over keys as they are
pressed. Two of the three targets can.

| Target | Keys | How |
|---|---|---|
| Native (`x86_64-unknown-linux-gnu`, macOS, Windows) | yes, on a tty | crossterm sets raw mode and decodes — Windows console records included. A pipe falls back to whole lines. |
| Browser (`wasm32-unknown-unknown`) | yes | xterm.js has no line discipline, so `onData` already delivers the escape stream a tty would. |
| WASI Preview 2 (`wasm32-wasip2`) | no | A component cannot reach the host's termios, so stdin arrives one finished line at a time. |

A `Session` reads the terminal's `Capabilities` and runs whichever loop it
can support, so the same host code works everywhere and gets whatever the
platform can give. WASI still records history; it just cannot walk it,
because there is no Up key to press. A host that wants to tell the user so
can ask:

```rust
if !session.capabilities().raw_mode {
    println!("plain line reader on this platform");
}
```

## Modules

| Module | What it holds |
|---|---|
| `key` | `KeyPress` — a physical key and its modifiers. Never an intent. |
| `decode` | `AnsiDecoder`: an incremental state machine from a terminal's character stream to `KeyPress`. Survives a sequence split across chunks. |
| `keymap` | `Action`, and the `Keymap` that decides which key means which. Rebindable. |
| `editor` | `LineEditor`: buffer, cursor, undo stack, history walk. Pure; no terminal, no async. |
| `history` | Recording, prefix search, and persistence through `ego_platform::BlobStore`. |
| `render` | Prompt and line to ANSI, wrapping and cursor placement included. |
| `style` | ANSI colours, and the width measurement that ignores them. |
| `extend` | `Completer` and `Highlighter`, the two hooks, plus `WordCompleter`. |
| `term` | The `Terminal` trait and the four backends. |
| `session` | The loop between a terminal and an editor. |

## Extending it

**Completion.** Implement `Completer`, or start from `WordCompleter`:

```rust
session.set_completer(WordCompleter::new(["echo", "exit", "status"]));
```

Tab completes a single candidate outright, inserts as much as several
candidates agree on, and lists them when that adds nothing.

**Highlighting.** Implement `Highlighter`. Return the same printable
characters with SGR escapes added — `ego_cli::style` has the pieces — and
the renderer's widths will ignore them, so the cursor stays where it
belongs:

```rust
impl Highlighter for MyHighlighter {
    fn highlight<'l>(&self, line: &'l str) -> Cow<'l, str> {
        Cow::Owned(style::paint(line, Color::Green))
    }
}
```

**Key bindings.** `session.keymap_mut()` binds any `KeyPress` to any
`Action`; `Keymap::empty()` starts from nothing.

Both traits are synchronous and take `&self`, deliberately: a completer
that has to await something — a network round trip, a C library behind
`diluvium-sys` — should look it up before the keystroke, not during it.

## Testing against it

`term::mem::MemTerminal` is a `Terminal` made of two buffers. Script the
input the way a terminal would send it, read back what was drawn — on any
target, without a tty:

```rust
let mut term = MemTerminal::raw(Size::new(80, 24));
term.push_input("ec\t\r");                 // "ec", Tab, Enter
let mut session = Session::new(term);
session.set_completer(WordCompleter::new(["echo"]));
assert_eq!(session.read_line().await?, ReadOutcome::Line("echo".into()));
```

Every test in this repo runs on all three targets through this backend.

## Default keys

| Key | Action |
|---|---|
| Left / Right, Home / End | move |
| Ctrl+Left / Ctrl+Right, Alt+B / Alt+F | move by word |
| Ctrl+A / Ctrl+E | start / end of line |
| Up / Down, Ctrl+P / Ctrl+N | history, filtered by what is left of the cursor |
| Backspace, Delete | delete one grapheme |
| Ctrl+Backspace, Ctrl+W, Alt+Backspace | delete the previous word |
| Ctrl+Delete, Alt+D | delete the next word |
| Ctrl+U / Ctrl+K | kill to start / end |
| Ctrl+Z, Ctrl+_ | undo |
| Ctrl+Y | redo |
| Tab | complete |
| Escape | clear the line |
| Ctrl+L | redraw on a clean screen |
| Ctrl+C | abandon the line |
| Ctrl+D | end of input on an empty line, forward delete otherwise |

Undo coalesces a run of typing into one step, broken at whitespace, so
Ctrl+Z takes back a word rather than a letter.

## Building and running

```sh
make check          # native, WASI and browser
make test           # the same three
make clippy fmt_check

make run            # the demo, natively
make run_wasi       # the demo under wasmtime
make serve          # the demo in a browser (needs `cargo install trunk`)
```

The demo (`src/main.rs`) is the worked integration: both terminal shapes, a
completer, a highlighter, and a `caps` command that prints what the platform
turned out to support.

Prerequisites beyond a stable toolchain: `rustup target add wasm32-wasip2
wasm32-unknown-unknown`, [wasmtime] for WASI, and [trunk] plus
`wasm-bindgen-cli` (pinned to the version in `Cargo.toml`) for the browser.

### Browser tests

`make test_browser` runs the same tests in a real browser via
`wasm-bindgen-test-runner`, which needs a WebDriver on `PATH` or named by
`GECKODRIVER` / `CHROMEDRIVER`. CI installs Firefox with geckodriver and
Chrome with chromedriver explicitly and runs both, so the wasm is exercised
on two engines rather than on whichever driver the runner image happened to
leave on `PATH`.

Two things bite when running these inside a container, neither with an
obvious error message:

- **The driver's major version must match the browser's.** chromedriver
  refuses the session outright; it does say so, but only if you ask it for a
  session by hand.
- **`wasm-bindgen-test-runner` treats any driver output on stderr as a
  failed start.** Its `has_failed()` is true whenever the child has written
  a byte to stderr. In a container without IPv6, chromedriver logs
  `CreatePlatformSocket() failed` there while binding IPv4 perfectly well,
  and the runner kills it and reports `driver failed to bind port during
  startup` — which is not what happened. `CHROMEDRIVER_ARGS=--silent` fixes
  it.

If the driver cannot find the browser on its own, a `webdriver.json` beside
`Cargo.toml` sets the capabilities to launch it with (point
`WASM_BINDGEN_TEST_WEBDRIVER_JSON` elsewhere to use another path).

## In the browser

The page owns the xterm.js terminal; `ego-cli` imports nothing and is handed
the object. So any way of getting an xterm.js `Terminal` works — bundler,
import map, or a `<script>` tag:

```js
import init, { start } from './ego-cli-demo.js';
import { Terminal } from '@xterm/xterm';

const term = new Terminal({ cursorBlink: true });
term.open(document.getElementById('terminal'));

await init();
await start(term);          // your #[wasm_bindgen] entry point
```

See `www/index.html` for the whole page, and `src/main.rs` for the Rust
side of `start`.

## Relationship to ego_shell

`ego_shell/` is in this repo as the transplant source. What changed:

- **A library, not a shell.** The `echo`/`decho`/`status` command
  processor, the response broadcast, and the TUI belonged to an
  application; they are gone. What is left is the line editing, which is
  the part worth reusing.
- **Keys and intents are separate.** `NormalizedKey` mixed `CtrlLeft` with
  `Undo`, which left no room for rebinding and needed a new variant per
  modifier combination. Now a `KeyPress` says what was pressed and a
  `Keymap` says what it means.
- **The decoder is a state machine.** `ego_shell` chained `str::replace`
  over each input chunk, substituting private-use codepoints. That
  mis-decoded any sequence split across two `onData` callbacks, could not
  tell a substitution from a private-use character the user typed, and
  needed a new `replace` per key. One rule now covers the whole control
  range, so Ctrl+W, Ctrl+U and Ctrl+K cost nothing.
- **The editor is wired up.** `ego_shell`'s `LineEditor` was never reached
  by its CLI, which read cooked lines; the editing existed only in its unit
  tests. Here it is what `read_line` runs.
- **It draws.** There was no renderer, so there was nothing to make an
  edited line appear. There is one now, wrapping and cursor placement
  included.
- **`aloeplatform` is [ego-platform], from git.** Time, spawn, IO, and the
  blob store history persists through.
- **`ego2_proto` is not a dependency.** Its `Ego2ShellStatus` and
  `Ego2ShellMode` described an application's lifecycle and which UI to
  start, which is the host's business, not a line editor's. A host that
  wants the mapping writes it where the vocabulary lives.

## License

Apache-2.0.

[ego-platform]: https://github.com/aloecraft-org/ego-platform
[wasmtime]: https://wasmtime.dev
[trunk]: https://trunkrs.dev
