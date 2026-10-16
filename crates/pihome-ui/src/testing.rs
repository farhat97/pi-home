//! Helpers for rendering tests against ratatui's `TestBackend`.

use ratatui::Frame;
use ratatui::Terminal;
use ratatui::backend::TestBackend;
use ratatui::buffer::Buffer;

use crate::theme::is_console_safe;

/// Renders one frame at `width` x `height` and returns the buffer.
pub fn render(width: u16, height: u16, draw: impl FnOnce(&mut Frame)) -> Buffer {
    let mut terminal = Terminal::new(TestBackend::new(width, height)).expect("test backend");
    terminal.draw(draw).expect("draw");
    terminal.backend().buffer().clone()
}

/// The buffer's symbols as lines of text, trailing spaces trimmed.
pub fn buffer_text(buf: &Buffer) -> String {
    let area = buf.area;
    (area.top()..area.bottom())
        .map(|y| {
            let line: String = (area.left()..area.right())
                .map(|x| buf[(x, y)].symbol())
                .collect();
            line.trim_end().to_owned()
        })
        .collect::<Vec<_>>()
        .join("\n")
}

/// Panics if any cell uses a glyph outside the console-safe set.
pub fn assert_console_safe(buf: &Buffer) {
    for (i, cell) in buf.content().iter().enumerate() {
        assert!(
            is_console_safe(cell.symbol()),
            "glyph {:?} at cell {i} is not console-safe",
            cell.symbol()
        );
    }
}
