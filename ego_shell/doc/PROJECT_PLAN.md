# EgoShell: Project Plan v1.0

**Date**: 2026-02-03  
**Goal**: Universal cross-platform shell interface with ruthless platform parity  
**Philosophy**: Depth-first implementation - get it compiling and deployed everywhere, then fill in functionality

---

## 1. Project Scope

### 1.1 In Scope
- ✅ Three test commands: `echo`, `decho` (delayed echo), `status`
- ✅ Two interface modes: CLI-lite (REPL) and TUI (crossterm/ratatui)
- ✅ Three platforms: Native, WASI, Browser (WASM)
- ✅ Crossterm TUI rendering in xterm.js via virtual terminal
- ✅ Separate command stream for programmatic control (concurrent with user interaction)
- ✅ Separate logging stream
- ✅ REST/Socket API for external commands on Native/WASI
- ✅ JavaScript integration examples (buttons + text input)
- ✅ Containerized headless operation

### 1.2 Out of Scope
- ❌ ego2 P2P client integration (happens later)
- ❌ Authentication/authorization
- ❌ Command history persistence
- ❌ Configuration hot-reload
- ❌ More than 3 test commands
- ❌ Plugin architecture
- ❌ Multi-user sessions
- ❌ Production-grade error handling (permissive during development)

### 1.3 Success Criteria
| Criterion | Measurement |
|-----------|-------------|
| Cross-platform parity | Same binary features on Native/WASI/Browser |
| CLI-lite works | `cargo run --bin cli_lite` on all 3 platforms |
| TUI works | Same crossterm UI in native terminal AND xterm.js |
| Command stream works | JavaScript can send commands while TUI runs |
| Logging works | Tailable logs on native/WASI, console.log on browser |
| Build succeeds | All platform targets compile without warnings |

---

## 2. Architecture

### 2.1 Core Data Flow

```
┌──────────────────────────────────────────────────────┐
│                  User Interfaces                     │
│  ┌─────────┐  ┌─────────┐  ┌──────────────────┐    │
│  │CLI-lite │  │   TUI   │  │ xterm.js Browser │    │
│  └────┬────┘  └────┬────┘  └────────┬─────────┘    │
│       │            │                 │               │
│       └────────────┴─────────────────┘               │
│                    │                                 │
└────────────────────┼─────────────────────────────────┘
                     │
         ┌───────────▼───────────┐
         │   Command Channel     │  ◄─── JavaScript / REST API
         │  (mpsc unbounded)     │
         └───────────┬───────────┘
                     │
         ┌───────────▼───────────┐
         │  Command Processor    │  ◄─── Single source of truth
         │  (async task)         │
         └───────────┬───────────┘
                     │
         ┌───────────▼───────────┐
         │  Response Channel     │
         │  (broadcast)          │
         └───────────┬───────────┘
                     │
         ┌───────────┴───────────┐
         │                       │
    ┌────▼────┐           ┌─────▼─────┐
    │ Display │           │ Log Sink  │
    │ Stream  │           │           │
    └─────────┘           └───────────┘
```

### 2.2 Stream Types

```rust
// Command input: Many → One
mpsc::UnboundedReceiver<Command>

// Response output: One → Many
broadcast::Sender<Response>

// Log output: One → Many
broadcast::Sender<LogEvent>
```

### 2.3 Platform Feature Matrix

| Feature | Native | WASI | Browser |
|---------|--------|------|---------|
| CLI-lite | ✅ stdin/stdout | ✅ stdin/stdout | ✅ xterm.js |
| TUI (crossterm) | ✅ Terminal | ✅ Terminal | ✅ xterm.js |
| Command API | ✅ REST (axum) | ✅ REST (axum) | ✅ JS binding |
| Log output | ✅ File + stdout | ✅ File + stdout | ✅ console.log |
| Headless | ✅ Docker | ✅ Docker | N/A |

---

## 3. Core Types

### 3.1 Commands
```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum Command {
    Echo { text: String },
    DelayedEcho { text: String, delay_secs: u64 },
    Status,
    Exit,
}
```

### 3.2 Responses
```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum Response {
    Output(String),
    Status(StatusInfo),
    Error(String),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StatusInfo {
    uptime_secs: u64,
    commands_processed: u64,
    platform: String,
    timestamp: u64,
}
```

### 3.3 Logs
```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LogEvent {
    timestamp: u64,
    level: LogLevel,
    message: String,
}

pub enum LogLevel {
    Info,
    Warn,
    Error,
}
```

---

## 4. Project Structure

```
egoshell/
├── Cargo.toml
├── Dockerfile
├── README.md
│
├── src/
│   ├── lib.rs              # Core types & processor
│   ├── command.rs          # Command/Response enums
│   ├── processor.rs        # Command processing logic
│   ├── vterm.rs            # Virtual terminal (crossterm → buffer)
│   ├── logging.rs          # Log stream setup
│   │
│   ├── bin/
│   │   ├── cli_lite.rs     # REPL interface
│   │   ├── tui.rs          # Crossterm TUI
│   │   └── headless.rs     # Daemon mode
│   │
│   └── platform/
│       ├── native.rs       # Native-specific (REST API, file logs)
│       ├── wasi.rs         # WASI-specific
│       └── wasm.rs         # Browser bindings
│
└── www/
    ├── index.html          # Demo page
    ├── package.json        # wasm-pack config
    └── README.md
```

---

## 5. Implementation Plan

### Phase 1: Core Foundation (Days 1-2)
**Goal**: Command processor compiles and runs unit tests

**Tasks**:
1. Create `command.rs` with Command/Response/LogEvent enums
2. Create `processor.rs` with async command handler
3. Implement echo (instant response)
4. Implement decho (tokio::time::sleep + response)
5. Implement status (return runtime stats)
6. Write unit tests for all commands

**Deliverables**:
- [ ] `cargo test --lib` passes
- [ ] All three commands tested with mock streams

**Exit Criteria**:
```rust
#[tokio::test]
async fn test_echo() {
    let (tx, rx) = mpsc::unbounded_channel();
    let (resp_tx, mut resp_rx) = broadcast::channel(10);
    
    tokio::spawn(command_processor(rx, resp_tx));
    
    tx.send(Command::Echo { text: "test".into() }).unwrap();
    
    match resp_rx.recv().await.unwrap() {
        Response::Output(s) => assert_eq!(s, "test"),
        _ => panic!("Wrong response type"),
    }
}
```

**Files**: 2 (command.rs, processor.rs)  
**LOC**: ~250

---

### Phase 2: Cross-Platform CLI-Lite (Days 2-4)
**Goal**: REPL works on Native, WASI, and Browser (xterm.js)

**Tasks**:
1. Create `bin/cli_lite.rs`
2. Platform-specific stdio wrappers:
   - Native: tokio::io::stdin/stdout
   - WASI: tokio::io::stdin/stdout (same)
   - Browser: Virtual buffer → xterm.js
3. Command parsing from string input
4. Response rendering
5. Create `www/index.html` with xterm.js
6. WASM bindings for browser CLI

**Deliverables**:
- [ ] `cargo run --bin cli_lite` works on native
- [ ] `cargo build --target wasm32-wasi` compiles
- [ ] `wasm-pack build --target web` succeeds
- [ ] Browser demo at `www/index.html` shows working REPL

**Exit Criteria**:

**Native/WASI**:
```bash
$ cargo run --bin cli_lite
EgoShell v0.1.0
> echo Hello
Hello
> decho Test 3
[3s delay]
Test
> status
Status { uptime_secs: 10, commands_processed: 3, ... }
> exit
```

**Browser**:
```
www/index.html loads
xterm.js terminal shows prompt
User types "echo Test" → sees "Test"
```

**Files**: 3 (cli_lite.rs, wasm.rs, index.html)  
**LOC**: ~400

---

### Phase 3: Cross-Platform TUI (Days 4-6)
**Goal**: Same crossterm TUI shows in native terminal AND xterm.js

**Tasks**:
1. Create `vterm.rs` - virtual terminal buffer
2. Create `bin/tui.rs` with ratatui layout:
   - Top pane: Output history
   - Bottom pane: Input line
3. Crossterm backend writes to VirtualTerminal
4. Native: VirtualTerminal → real terminal
5. Browser: VirtualTerminal → drain buffer → xterm.js
6. Update `www/index.html` to support TUI mode toggle

**Deliverables**:
- [ ] `cargo run --bin tui` works on native
- [ ] Same TUI layout shows in browser via xterm.js
- [ ] Keyboard input works on both platforms
- [ ] ANSI codes render identically

**Exit Criteria**:

**Native**:
```bash
$ cargo run --bin tui
┌─ Output ──────────────────┐
│ > echo Test               │
│ Test                      │
│ > status                  │
│ Status { uptime: 5s, ... }│
└───────────────────────────┘
┌─ Input ───────────────────┐
│ _                         │
└───────────────────────────┘
```

**Browser**:
Same visual layout in xterm.js

**Files**: 2 (vterm.rs, tui.rs)  
**LOC**: ~350

---

### Phase 4: Command Stream + Logging (Days 6-8)
**Goal**: External commands work concurrently with user input; logs are tailable

**Tasks**:
1. Create `logging.rs` - log event stream
2. Integrate log macros → broadcast channel
3. Native/WASI: REST API (axum) for commands
   - POST /command → send Command
   - GET /status → get StatusInfo
4. Browser: JavaScript command API
   - `shell.sendCommand({ Echo: { text: "..." } })`
5. Create log sinks:
   - Native/WASI: File (append mode, tailable)
   - Browser: console.log
6. Update `www/index.html`:
   - Add text input + "Echo" button
   - Add "Delayed Echo (5s)" button
   - Add status display div
   - Show logs in separate div

**Deliverables**:
- [ ] REST API on native/WASI (port 3000)
- [ ] JavaScript API in browser
- [ ] Logs written to file (native/WASI) and console (browser)
- [ ] User can operate TUI while external commands execute
- [ ] Demo page has working buttons

**Exit Criteria**:

**Native/WASI**:
```bash
# Terminal 1
$ cargo run --bin tui

# Terminal 2
$ curl -X POST http://localhost:3000/command \
  -H "Content-Type: application/json" \
  -d '{"Echo":{"text":"From curl"}}'

# Terminal 1 shows "From curl" in TUI

# Terminal 3
$ tail -f /tmp/egoshell.log
[INFO] Command processed: Echo
[INFO] Response sent: Output("From curl")
```

**Browser**:
```html
<input id="echo-text" value="Hello" />
<button onclick="sendEcho()">Echo</button>
<!-- Click button → "Hello" appears in TUI output pane -->
```

**Files**: 4 (logging.rs, platform/native.rs, platform/wasm.rs updates)  
**LOC**: ~300

---

### Phase 5: Headless + Docker (Days 8-9)
**Goal**: Containerized headless daemon with REST API and file logs

**Tasks**:
1. Create `bin/headless.rs` - daemon mode (no stdio)
2. Create Dockerfile
3. Mount points for /config and /logs
4. Environment variables:
   - `EGOSHELL_API_PORT` (default 3000)
   - `EGOSHELL_LOG_DIR` (default /logs)
5. Periodic status logging (every 30s)
6. Graceful shutdown on SIGTERM

**Deliverables**:
- [ ] `cargo run --bin headless` starts daemon
- [ ] `docker build -t egoshell .` succeeds
- [ ] Container runs and accepts commands
- [ ] Logs written to mounted volume

**Exit Criteria**:
```bash
$ docker build -t egoshell .
$ docker run -d \
  -v $(pwd)/logs:/logs \
  -p 3000:3000 \
  -e EGOSHELL_API_PORT=3000 \
  egoshell

$ curl -X POST http://localhost:3000/command \
  -d '{"Echo":{"text":"Docker works"}}'

$ cat logs/egoshell.log
[2026-02-03 15:04:01] [INFO] EgoShell started
[2026-02-03 15:04:01] [INFO] API server listening on 3000
[2026-02-03 15:04:31] [INFO] Status: uptime=30s, commands=0
[2026-02-03 15:05:15] [INFO] Command: Echo
```

**Files**: 2 (headless.rs, Dockerfile)  
**LOC**: ~200

---

### Phase 6: Polish + Documentation (Day 10)
**Goal**: Clean up, document, prepare for ego2 integration

**Tasks**:
1. Add comprehensive README.md
2. Document all platform build commands
3. Create www/README.md for browser setup
4. Add inline code comments
5. Fix any compiler warnings
6. Create example scripts:
   - `examples/test_cli.sh`
   - `examples/test_api.sh`
   - `examples/docker_run.sh`

**Deliverables**:
- [ ] README explains how to build/run on all platforms
- [ ] Zero compiler warnings
- [ ] Example scripts work

**Exit Criteria**:
- New developer can clone repo and run all modes successfully
- Documentation explains command/response/log streams clearly

**Files**: 3 (README.md, www/README.md, examples/*.sh)  
**LOC**: ~500 (mostly docs)

---

## 6. Build Matrix

### 6.1 Native (Linux/Mac/Windows)
```bash
# CLI-lite
cargo build --bin cli_lite --release

# TUI
cargo build --bin tui --release --features tui

# Headless
cargo build --bin headless --release --features api
```

### 6.2 WASI
```bash
cargo build --target wasm32-wasi --bin cli_lite --release
cargo build --target wasm32-wasi --bin tui --release --features tui
cargo build --target wasm32-wasi --bin headless --release --features api
```

### 6.3 Browser (WASM)
```bash
wasm-pack build --target web --out-dir www/pkg
# Serves www/index.html
```

---

## 7. Dependencies

```toml
[dependencies]
tokio = { version = "1", features = ["sync", "time", "macros"] }
serde = { version = "1", features = ["derive"] }
serde_json = "1"
log = "0.4"

# TUI
ratatui = { version = "0.26", optional = true }
crossterm = { version = "0.27", optional = true }

# Native API
axum = { version = "0.7", optional = true }

# Browser
wasm-bindgen = { version = "0.2", optional = true }
wasm-bindgen-futures = { version = "0.4", optional = true }
console_error_panic_hook = { version = "0.1", optional = true }
console_log = { version = "1", optional = true }
serde-wasm-bindgen = { version = "0.6", optional = true }
js-sys = { version = "0.3", optional = true }

[target.'cfg(not(target_arch = "wasm32"))'.dependencies]
tokio = { version = "1", features = ["rt-multi-thread", "io-util", "net"] }

[features]
tui = ["ratatui", "crossterm"]
api = ["axum"]
wasm = ["wasm-bindgen", "wasm-bindgen-futures", "console_error_panic_hook", "console_log", "serde-wasm-bindgen", "js-sys"]
```

---

## 8. Risk Mitigation

| Risk | Impact | Mitigation |
|------|--------|------------|
| crossterm ANSI codes differ browser/native | High | Phase 3 tests both early |
| WASI networking support incomplete | Medium | Use feature flags, fallback to stdio |
| Virtual terminal buffer performance | Low | Start simple, optimize if needed |
| Scope creep | High | **Reference this document** - if it's not listed, it's Phase 7+ |

---

## 9. Timeline

| Phase | Days | Cumulative |
|-------|------|------------|
| 1. Core | 2 | Day 2 |
| 2. CLI-lite | 2 | Day 4 |
| 3. TUI | 2 | Day 6 |
| 4. Command/Log streams | 2 | Day 8 |
| 5. Headless/Docker | 1 | Day 9 |
| 6. Polish | 1 | Day 10 |

**Total: 10 days**

---

## 10. Definition of Done

EgoShell is **complete** when:

1. ✅ All 6 phases have passing exit criteria
2. ✅ `cargo build` succeeds for all targets (native, wasm32-wasi, wasm32-unknown-unknown)
3. ✅ Zero compiler warnings
4. ✅ Unit tests pass (`cargo test`)
5. ✅ Documentation complete (README + inline comments)
6. ✅ Demo page works in Chrome and Firefox
7. ✅ Docker container runs successfully

At completion, EgoShell is **ready to receive ego2 runtime integration** (Phase 7+, out of scope for this document).

---

**End of Plan**