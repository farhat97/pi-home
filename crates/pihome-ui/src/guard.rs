//! Minimum terminal size check shared by all apps.

use ratatui::Frame;
use ratatui::buffer::Buffer;
use ratatui::layout::{Alignment, Rect};
use ratatui::text::Line;
use ratatui::widgets::{Paragraph, Widget};

use crate::theme::Theme;

/// Every layout is designed for the 3.5" display's 60x20 grid.
pub const MIN_WIDTH: u16 = 60;
pub const MIN_HEIGHT: u16 = 20;

/// Returns the frame area when it is at least 60x20. Otherwise draws the
/// "terminal too small" message and returns `None`.
pub fn size_guard(frame: &mut Frame, theme: &Theme) -> Option<Rect> {
    let area = frame.area();
    if area.width >= MIN_WIDTH && area.height >= MIN_HEIGHT {
        Some(area)
    } else {
        frame.render_widget(TooSmall::new(theme, area.width, area.height), area);
        None
    }
}

/// Centered message naming the required and current size.
pub struct TooSmall<'a> {
    theme: &'a Theme,
    width: u16,
    height: u16,
}

impl<'a> TooSmall<'a> {
    pub fn new(theme: &'a Theme, width: u16, height: u16) -> Self {
        Self {
            theme,
            width,
            height,
        }
    }
}

impl Widget for TooSmall<'_> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        let lines = vec![
            Line::styled("terminal too small", self.theme.alert_style()),
            Line::styled(
                format!("need {MIN_WIDTH}x{MIN_HEIGHT}"),
                self.theme.text_style(),
            ),
            Line::styled(
                format!("have {}x{}", self.width, self.height),
                self.theme.dim_style(),
            ),
        ];
        let top = area.y + area.height.saturating_sub(lines.len() as u16) / 2;
        let rect = Rect::new(area.x, top, area.width, area.bottom() - top);
        Paragraph::new(lines)
            .alignment(Alignment::Center)
            .render(rect, buf);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testing::{buffer_text, render};

    fn guarded(w: u16, h: u16) -> (Option<Rect>, String) {
        let theme = Theme::basic16();
        let mut got = None;
        let buf = render(w, h, |f| {
            got = size_guard(f, &theme);
            if let Some(a) = got {
                f.render_widget(Paragraph::new("layout"), a);
            }
        });
        (got, buffer_text(&buf))
    }

    #[test]
    fn exact_minimum_fits() {
        let (area, text) = guarded(60, 20);
        assert_eq!(area, Some(Rect::new(0, 0, 60, 20)));
        assert!(text.starts_with("layout"));
        assert!(!text.contains("too small"));
    }

    #[test]
    fn one_column_short_shows_message() {
        let (area, text) = guarded(59, 20);
        assert_eq!(area, None);
        assert!(text.contains("terminal too small"));
        assert!(text.contains("need 60x20"));
        assert!(text.contains("have 59x20"));
        insta::assert_snapshot!(text);
    }

    #[test]
    fn one_row_short_shows_message() {
        let (area, text) = guarded(60, 19);
        assert_eq!(area, None);
        assert!(text.contains("have 60x19"));
    }

    #[test]
    fn restores_layout_when_resized_back() {
        assert!(guarded(40, 10).0.is_none());
        assert!(guarded(80, 24).0.is_some());
    }
}
