# ego-cli

A library: cross-platform line editing for command-line applications.
`Session::read_line` on native, WASI Preview 2 and the browser, with
history, word motions, undo/redo, and hooks for completion and
highlighting.

Downstream, in order: `diluvium` (a C CLI, reached through
`diluvium-sys`) and `diluvium-drt` (already Rust). Neither is in this
repo yet; the shape of `extend::Completer` and `extend::Highlighter` is
what they will plug into.

`ego_shell/` is the transplant source, kept for reference. It does not
build — its path dependencies live outside this repo — and it is
excluded from the workspace. Read it to check what was carried over;
do not extend it.

## Grades

- `*`: maintainer
- `www/index.html`: example

Everything is maintainer-grade except the browser demo page, which
exists to be copied into a host's own page and should stay short enough
to read in one screen.

`src/main.rs` is deliberately **not** example-grade even though it is a
demo. It has to exercise the whole surface — both terminal shapes, a
completer, a highlighter, the capability report — and cutting it to one
screen would cost the thing it is for. Read it as the worked integration,
not as a snippet to retype.

Example-grade integration snippets for `diluvium` and `diluvium-drt`
will be declared here when they land.

## Surface blocks

Every module opens with one, under a `# Surface` heading in the module
docs, so it renders in `cargo doc` as well as reading well in the file.
`crate::term` is the one to check first: it is where the three platforms
diverge, and it names which backend `PlatformTerminal` resolves to.
