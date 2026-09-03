//! The runtime-free native backend, and the property that makes it work:
//! nothing in a `Session` ever returns `Pending`, so a plain executor with
//! no runtime behind it drives one to completion.

#![cfg(not(target_arch = "wasm32"))]

mod common;
use common::test;

use futures_executor::block_on;

use ego_cli::term::mem::MemTerminal;
use ego_cli::{ReadOutcome, Session, Size, Terminal};

/// The claim `BlockingNative` rests on. `MemTerminal` stands in for it here
/// because it needs no tty, but the shape is the same: every future the
/// session awaits is ready when first polled.
#[test]
fn a_session_completes_under_a_plain_executor() {
    let mut term = MemTerminal::raw(Size::new(80, 24));
    term.push_input("hello\r");
    let mut session = Session::new(term);

    let outcome = block_on(session.read_line()).unwrap();
    assert_eq!(outcome, ReadOutcome::Line("hello".into()));
}

#[test]
fn several_lines_in_a_row_under_a_plain_executor() {
    let mut term = MemTerminal::raw(Size::new(80, 24));
    term.push_input("one\rtwo\r");
    let mut session = Session::new(term);

    assert_eq!(
        block_on(session.read_line()).unwrap(),
        ReadOutcome::Line("one".into())
    );
    assert_eq!(
        block_on(session.read_line()).unwrap(),
        ReadOutcome::Line("two".into())
    );
    assert_eq!(block_on(session.read_line()).unwrap(), ReadOutcome::Eof);
}

/// The write path is the one that actually breaks without a runtime: tokio's
/// stdout dispatches to a blocking pool and panics with no reactor, while
/// this backend's `std::io::Stdout` does not.
#[test]
fn the_blocking_backend_writes_without_a_runtime() {
    use ego_cli::term::blocking::BlockingNative;

    let mut term = BlockingNative::new().expect("open terminal");
    block_on(term.write("")).expect("write under a plain executor");
    block_on(term.flush()).expect("flush under a plain executor");
}

#[test]
fn the_blocking_backend_reports_consistent_capabilities() {
    use ego_cli::term::blocking::BlockingNative;

    let term = BlockingNative::new().expect("open terminal");
    let capabilities = term.capabilities();
    // Raw mode and ANSI both follow from having a tty, so they agree.
    assert_eq!(capabilities.raw_mode, term.is_raw_capable());
    assert_eq!(capabilities.ansi, term.is_raw_capable());
    assert!(capabilities.line_discipline);
    assert!(term.size().cols > 0);
}
