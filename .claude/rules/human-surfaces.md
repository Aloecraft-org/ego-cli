# Human surfaces

This file is identical in every repo. Everything project-specific lives in
`.claude/this_project.md`. Read that file before writing code; it declares
which paths a human reads and at what grade. If it is missing, treat the
whole repo as maintainer-grade and say so.

Code has two readers: the agent maintaining it, and a human reading a
small number of declared surfaces. Write for the maintainer by default.
Write for the human only where `this_project.md` says a human reads.

## Grades

`this_project.md` assigns each declared path one grade:

- `maintainer` (the default for anything undeclared): correctness and
  completeness win over brevity. None of the costly rules below apply.
- `example`: code that exists to be read and retyped. Its job is
  transferring understanding; it may be incomplete if it says so.
- `tool`: code a human reads, runs, and edits directly. It must be
  complete and correct, but only as complete as its spec requires.

A repo may declare a grade for `*`. A small tool repo where the human
reads every line is typically `*: tool`.

## Rules for everything (cheap, no correctness cost)

Any file with dense logic opens with a surface block. The surface block
contains, in order, and nothing else:

1. Entry points: public functions, routes, commands, handlers.
2. Configurable values: constants a human might change.
3. Fan-out points: dispatch tables, state transitions, the list of
   implementations behind a polymorphic base.

Nothing that belongs in the surface block may be declared elsewhere in
the file. No constants at first use, no route registered next to its
handler, no dispatch case added inside the dispatching function.

If a file has no natural surface block (logic multiplexed across
branches, cases spread across subclasses), make one: an explicit dispatch
map, a registration block, or a stub listing the implementations and what
distinguishes them. If you cannot write a surface block, say so rather
than skipping it; the file is probably doing too much.

Below the surface block, mark dense sections so a reader knows they may
skip them: `# depth: retry and backoff`. Surface comments explain why;
depth comments may explain what.

## Example-grade rules

- Fits on one screen. If it does not, split it or cut it.
- Every line is something the reader would plausibly type themselves.
  Boilerplate they would not type goes in a helper or is elided.
- Error handling, retries, validation, and annotations may be omitted.
  Each omission is stated in one comment: `# example: omits <thing>`,
  optionally pointing at where the real version lives. This marker is
  permitted only in example-grade paths; elsewhere it is a bug to fix,
  not a comment to delete.
- The example must still run unchanged against the real API. Never make
  it readable by making it wrong or by stubbing the interface.
- The reader must be able to retype it from memory after one read.

## Tool-grade rules

- Build only what the spec calls for. If it is not in `this_project.md`
  or the documents it points to, do not build it: no extra flags, help
  text, logging, colors, retries, wrappers, or config the spec does not
  name. Propose additions in conversation instead.
- Error handling is what the spec states, exactly, and nothing more.
  The omission marker is not used; a tool is complete by definition.
- When the spec embeds a snippet, that snippet is the target shape and
  size, not a starting point to elaborate on.
- The human edits these files by hand. Prefer flat, obvious code over
  abstraction; three similar lines beat a helper the spec didn't ask for.

## Do not

- Apply example- or tool-grade rules to undeclared paths, even if they
  look like they would benefit. Propose a declaration instead.
- Make code "readable" by shortening names, removing branches, or
  dropping cases. Readability is a property of shape, not size.
- Interleave declarations that belong in the surface block.
- Paper over a missing surface block with a comment describing one.