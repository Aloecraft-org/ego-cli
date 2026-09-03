//! The renderer's cursor arithmetic — the part that is hardest to see is
//! wrong by looking at a terminal, and easiest to pin down here.

mod common;
use common::test;

use ego_cli::render::Renderer;
use ego_cli::style::display_width;

#[test]
fn the_first_frame_just_draws() {
    let mut renderer = Renderer::new(80);
    assert_eq!(renderer.frame("> ", "hi", 2), "> hi");
}

#[test]
fn later_frames_return_to_the_top_and_erase() {
    let mut renderer = Renderer::new(80);
    renderer.frame("> ", "hi", 2);
    assert_eq!(renderer.frame("> ", "hip", 3), "\r\x1b[J> hip");
}

#[test]
fn a_cursor_inside_the_line_is_moved_back_to() {
    let mut renderer = Renderer::new(80);
    let frame = renderer.frame("> ", "hip", 1);
    // Draw the whole line, then come back to column 3 (prompt is 2 wide).
    assert!(frame.ends_with("> hip\r\x1b[3C"), "{frame:?}");
}

#[test]
fn a_wrapped_line_is_rewound_by_its_rows() {
    let mut renderer = Renderer::new(10);
    // 2 + 15 = 17 cells over 10 columns: the cursor ends on row 1.
    renderer.frame("> ", &"x".repeat(15), 15);
    let frame = renderer.frame("> ", &"x".repeat(15), 15);
    assert!(frame.starts_with("\r\x1b[1A\x1b[J"), "{frame:?}");
}

/// Ending exactly on the margin is the case terminals disagree about; the
/// padding space makes both of them agree.
#[test]
fn a_line_ending_on_the_margin_is_padded() {
    let mut renderer = Renderer::new(10);
    let frame = renderer.frame("> ", &"x".repeat(8), 8);
    assert_eq!(frame, format!("> {} \r", "x".repeat(8)));

    // And the next frame rewinds by the row the padding put us on.
    let next = renderer.frame("> ", &"x".repeat(8), 8);
    assert!(next.starts_with("\r\x1b[1A\x1b[J"), "{next:?}");
}

#[test]
fn finish_erases_the_padding_before_moving_on() {
    let mut renderer = Renderer::new(10);
    renderer.frame("> ", &"x".repeat(8), 8);
    assert_eq!(renderer.finish(), "\r\x1b[K\r\n");

    let mut renderer = Renderer::new(80);
    renderer.frame("> ", "hi", 2);
    assert_eq!(renderer.finish(), "\r\n");
}

#[test]
fn finish_comes_down_from_a_cursor_left_mid_line() {
    let mut renderer = Renderer::new(10);
    // 2 + 15 cells, cursor at the very start: row 0, while the text ends on
    // row 1.
    renderer.frame("> ", &"x".repeat(15), 0);
    assert_eq!(renderer.finish(), "\x1b[1B\r\n");
}

#[test]
fn erase_leaves_nothing_and_forgets() {
    let mut renderer = Renderer::new(80);
    renderer.frame("> ", "hi", 2);
    assert_eq!(renderer.erase(), "\r\x1b[J");
    assert_eq!(renderer.frame("> ", "hi", 2), "> hi", "drawing starts over");
}

/// A highlighter adds escapes; every width the renderer computes has to
/// ignore them, or the cursor lands in the wrong column.
#[test]
fn colours_do_not_shift_the_cursor() {
    let mut plain = Renderer::new(80);
    let mut coloured = Renderer::new(80);
    let plain_frame = plain.frame("> ", "hip", 1);
    let coloured_frame = coloured.frame("> ", "\x1b[32mhip\x1b[39m", 1);

    assert!(plain_frame.ends_with("\r\x1b[3C"));
    assert!(coloured_frame.ends_with("\r\x1b[3C"), "{coloured_frame:?}");
}

#[test]
fn width_ignores_escapes_and_counts_wide_characters() {
    assert_eq!(display_width("hi"), 2);
    assert_eq!(display_width("\x1b[32mhi\x1b[0m"), 2);
    assert_eq!(display_width("日本"), 4, "each CJK character is two cells");
}

#[test]
fn a_zero_width_terminal_does_not_divide_by_zero() {
    let mut renderer = Renderer::new(0);
    assert_eq!(renderer.cols(), 1);
    renderer.frame("> ", "hi", 2);
}
