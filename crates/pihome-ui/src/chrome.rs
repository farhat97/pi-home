//! Header chrome every app draws: its name on the left of the top border and
//! the rotation indicator on the right.

use std::time::SystemTime;

use ratatui::style::Modifier;
use ratatui::text::{Line, Span};
use ratatui::widgets::Block;

use crate::contract::{RotationState, StateDir};
use crate::theme::Theme;
use crate::widgets::pane;

/// What the header shows about rotation. Only built from fresh, valid state.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Indicator {
    pub active: usize,
    pub count: usize,
    pub next_in_s: u64,
    pub pinned: bool,
}

impl Indicator {
    /// `None` when the state is absent, stale at `now`, or inconsistent.
    pub fn from_state(state: Option<&RotationState>, now: SystemTime) -> Option<Self> {
        let s = state?;
        if !s.is_fresh(now) || s.count == 0 || s.active >= s.count {
            return None;
        }
        Some(Self {
            active: s.active,
            count: s.count,
            next_in_s: s.next_in_s,
            pinned: s.pinned,
        })
    }

    /// Reads the rotator's state from `dir`.
    pub fn read(dir: &StateDir, now: SystemTime) -> Option<Self> {
        Self::from_state(dir.read_state().as_ref(), now)
    }

    /// `[* o o] 42s`, or `[* o o] hold` while pinned.
    pub fn line(&self, theme: &Theme) -> Line<'static> {
        let mut spans = vec![Span::raw(" "), Span::styled("[", theme.dim_style())];
        for i in 0..self.count {
            if i > 0 {
                spans.push(Span::raw(" "));
            }
            spans.push(if i == self.active {
                Span::styled("*", theme.title_style().fg(theme.accent))
            } else {
                Span::styled("o", theme.dim_style())
            });
        }
        spans.push(Span::styled("] ", theme.dim_style()));
        spans.push(if self.pinned {
            Span::styled("hold", theme.alert_style())
        } else {
            Span::styled(format!("{}s", self.next_in_s), theme.text_style())
        });
        spans.push(Span::raw(" "));
        Line::from(spans).right_aligned()
    }
}

/// The outermost pane of an app: a titled pane with the rotation indicator
/// in the top-right of the border when rotation state is available.
pub fn chrome<'a>(theme: &Theme, app_name: &'a str, indicator: Option<Indicator>) -> Block<'a> {
    let block = pane(theme, app_name);
    match indicator {
        Some(ind) => block.title_top(ind.line(theme).patch_style(Modifier::empty())),
        None => block,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::contract::from_unix_ms;
    use crate::testing::{assert_console_safe, buffer_text, render};

    const NOW: u64 = 1_700_000_000_000;

    fn state(active: usize, next_in_s: u64, pinned: bool, age_ms: u64) -> RotationState {
        RotationState {
            active,
            count: 3,
            next_in_s,
            pinned,
            updated_ms: NOW - age_ms,
        }
    }

    fn header(state: Option<RotationState>) -> String {
        let theme = Theme::basic16();
        let ind = Indicator::from_state(state.as_ref(), from_unix_ms(NOW));
        let buf = render(40, 3, |f| {
            f.render_widget(chrome(&theme, "pi-hole", ind), f.area())
        });
        assert_console_safe(&buf);
        buffer_text(&buf)
    }

    #[test]
    fn rotating() {
        let text = header(Some(state(0, 42, false, 300)));
        assert!(text.lines().next().unwrap().ends_with("[* o o] 42s ┐"));
        insta::assert_snapshot!(text);
    }

    #[test]
    fn pinned_shows_hold() {
        let text = header(Some(state(1, 0, true, 0)));
        assert!(text.contains("[o * o] hold"));
        insta::assert_snapshot!(text);
    }

    #[test]
    fn absent_state_hides_indicator() {
        let text = header(None);
        assert!(!text.contains('['));
        insta::assert_snapshot!(text);
    }

    #[test]
    fn stale_state_hides_indicator_and_fresh_restores_it() {
        assert!(!header(Some(state(0, 42, false, 5_001))).contains('['));
        assert!(header(Some(state(0, 42, false, 5_000))).contains("[* o o] 42s"));
    }

    #[test]
    fn inconsistent_state_hides_indicator() {
        let mut s = state(3, 1, false, 0);
        assert!(!header(Some(s.clone())).contains('['));
        s.count = 0;
        s.active = 0;
        assert!(!header(Some(s)).contains('['));
    }

    #[test]
    fn active_marker_uses_accent() {
        let theme = Theme::truecolor();
        let ind = Indicator {
            active: 0,
            count: 1,
            next_in_s: 5,
            pinned: false,
        };
        let line = ind.line(&theme);
        let star = line.spans.iter().find(|s| s.content == "*").unwrap();
        assert_eq!(star.style.fg, Some(theme.accent));
    }
}
