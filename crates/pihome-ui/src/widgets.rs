//! btop-style building blocks: titled panes, gradient meters, history graphs.

use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::{Color, Style};
use ratatui::symbols::merge::MergeStrategy;
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Widget};

use crate::theme::{GraphGlyphs, Theme};

/// A bordered pane with its title set into the top border. Adjacent panes
/// whose borders overlap by one cell merge into shared junctions.
pub fn pane<'a>(theme: &Theme, title: &'a str) -> Block<'a> {
    let block = Block::new()
        .borders(Borders::ALL)
        .border_set(theme.border_set)
        .border_style(theme.border_style())
        .merge_borders(MergeStrategy::Fuzzy);
    if title.is_empty() {
        block
    } else {
        block.title_top(Line::from(Span::styled(
            format!(" {title} "),
            theme.title_style(),
        )))
    }
}

/// Shortens `s` to at most `width` characters, ending with `ellipsis` when cut.
pub fn fit(s: &str, width: usize, ellipsis: char) -> String {
    if s.chars().count() <= width {
        return s.to_owned();
    }
    if width == 0 {
        return String::new();
    }
    let mut out: String = s.chars().take(width - 1).collect();
    out.push(ellipsis);
    out
}

/// A one-row horizontal meter with a right-aligned label, colored along the
/// theme's gradient.
pub struct Meter<'a> {
    theme: &'a Theme,
    ratio: f64,
    label: String,
}

impl<'a> Meter<'a> {
    /// `ratio` is clamped to `0.0..=1.0`; the label defaults to a percentage.
    pub fn new(theme: &'a Theme, ratio: f64) -> Self {
        let ratio = if ratio.is_finite() {
            ratio.clamp(0.0, 1.0)
        } else {
            0.0
        };
        Self {
            theme,
            ratio,
            label: format!("{:.1}%", ratio * 100.0),
        }
    }

    pub fn label(mut self, label: impl Into<String>) -> Self {
        self.label = label.into();
        self
    }
}

impl Widget for Meter<'_> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        if area.is_empty() {
            return;
        }
        let label_w = self.label.chars().count() as u16 + 1;
        let bar_w = area.width.saturating_sub(label_w);
        let filled = (self.ratio * bar_w as f64).round() as u16;
        for i in 0..bar_w {
            let (ch, style) = if i < filled {
                let t = (i as f64 + 0.5) / bar_w as f64;
                (
                    self.theme.meter_full,
                    Style::new().fg(self.theme.meter_color(t)),
                )
            } else {
                (self.theme.meter_empty, self.theme.dim_style())
            };
            if let Some(cell) = buf.cell_mut((area.x + i, area.y)) {
                cell.set_char(ch).set_style(style);
            }
        }
        let label_x = area.x + area.width.saturating_sub(label_w - 1);
        buf.set_stringn(
            label_x,
            area.y,
            &self.label,
            (area.right() - label_x) as usize,
            self.theme.text_style(),
        );
    }
}

/// A history graph of a main series with a sub-series drawn inside it (for
/// example all queries with the blocked part highlighted). Braille in
/// truecolor mode, block characters in the console fallback.
pub struct HistoryGraph<'a> {
    theme: &'a Theme,
    /// `(main, alt)` per sample, oldest first; `alt` is a part of `main`.
    data: &'a [(f64, f64)],
    axis: bool,
}

impl<'a> HistoryGraph<'a> {
    pub fn new(theme: &'a Theme, data: &'a [(f64, f64)]) -> Self {
        Self {
            theme,
            data,
            axis: false,
        }
    }

    /// Adds a left gutter labelled with the peak (top) and zero (bottom).
    pub fn axis(mut self, on: bool) -> Self {
        self.axis = on;
        self
    }

    /// Average of each bucket when squeezing `data` into `columns`.
    fn resample(&self, columns: usize) -> Vec<(f64, f64)> {
        let n = self.data.len();
        if n == 0 || columns == 0 {
            return Vec::new();
        }
        if n <= columns {
            // Right-align so the newest sample is at the right edge.
            let mut out = vec![(0.0, 0.0); columns - n];
            out.extend_from_slice(self.data);
            return out;
        }
        (0..columns)
            .map(|i| {
                let (a, b) = (
                    i * n / columns,
                    ((i + 1) * n / columns).max(i * n / columns + 1),
                );
                let len = (b - a) as f64;
                let (m, s) = self.data[a..b]
                    .iter()
                    .fold((0.0, 0.0), |acc, &(m, s)| (acc.0 + m, acc.1 + s));
                (m / len, s / len)
            })
            .collect()
    }
}

/// Bit for the braille dot at `(col, row)` within a cell, row 0 at the top.
const BRAILLE_BITS: [[u8; 4]; 2] = [[0x01, 0x02, 0x04, 0x40], [0x08, 0x10, 0x20, 0x80]];

/// Rounds a value to a height in steps, keeping nonzero values visible.
fn steps(v: f64, peak: f64, total: u16) -> u16 {
    if v <= 0.0 || peak <= 0.0 {
        return 0;
    }
    ((v / peak * total as f64).round() as u16).clamp(1, total)
}

/// Compact number for axis labels: 950, 1.2k, 48k, 3.1M.
pub fn short_count(v: f64) -> String {
    let v = v.max(0.0);
    match v {
        v if v >= 10_000_000.0 => format!("{:.0}M", v / 1_000_000.0),
        v if v >= 1_000_000.0 => format!("{:.1}M", v / 1_000_000.0),
        v if v >= 10_000.0 => format!("{:.0}k", v / 1_000.0),
        v if v >= 1_000.0 => format!("{:.1}k", v / 1_000.0),
        v => format!("{v:.0}"),
    }
}

impl Widget for HistoryGraph<'_> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        if area.is_empty() {
            return;
        }
        let mut plot = area;
        let per_cell = match self.theme.graph {
            GraphGlyphs::Braille => 2,
            GraphGlyphs::Blocks => 1,
        };

        // The gutter width depends on the peak label, and the peak on the
        // plot width, so size the gutter from the raw peak first.
        let raw_peak = self.data.iter().map(|d| d.0).fold(0.0, f64::max);
        let gutter = if self.axis {
            short_count(raw_peak).len().max(1) as u16 + 1
        } else {
            0
        };
        plot.x += gutter;
        plot.width = plot.width.saturating_sub(gutter);

        let cols = self.resample(plot.width as usize * per_cell);
        let peak = cols.iter().map(|d| d.0).fold(0.0, f64::max);

        if self.axis && area.width > gutter {
            let label_w = gutter as usize - 1;
            let dim = self.theme.dim_style();
            let top = format!("{:>label_w$}", short_count(peak));
            buf.set_stringn(area.x, area.y, top, label_w, dim);
            let zero = format!("{:>label_w$}", "0");
            buf.set_stringn(area.x, area.bottom() - 1, zero, label_w, dim);
            for y in area.top()..area.bottom() {
                if let Some(c) = buf.cell_mut((area.x + gutter - 1, y)) {
                    c.set_symbol(self.theme.border_set.vertical_left)
                        .set_style(dim);
                }
            }
        }

        let (main, alt) = (self.theme.graph_main, self.theme.graph_alt);
        let h = plot.height;
        match self.theme.graph {
            GraphGlyphs::Braille => {
                let dots = h * 4;
                let hs: Vec<_> = cols
                    .iter()
                    .map(|&(m, s)| (steps(m, peak, dots), steps(s.min(m), peak, dots)))
                    .collect();
                for cx in 0..plot.width {
                    for cy in 0..h {
                        let base = (h - 1 - cy) * 4;
                        let (mut bits, mut n_main, mut n_alt) = (0u8, 0, 0);
                        for (c, col_bits) in BRAILLE_BITS.iter().enumerate() {
                            let Some(&(hm, hs)) = hs.get(cx as usize * 2 + c) else {
                                continue;
                            };
                            for r in 0..4u16 {
                                let d = base + (3 - r);
                                if d < hm {
                                    bits |= col_bits[r as usize];
                                    if d < hs { n_alt += 1 } else { n_main += 1 }
                                }
                            }
                        }
                        if bits != 0 {
                            let ch = char::from_u32(0x2800 + bits as u32).unwrap_or(' ');
                            let fg: Color = if n_alt >= n_main { alt } else { main };
                            if let Some(cell) = buf.cell_mut((plot.x + cx, plot.y + cy)) {
                                cell.set_char(ch).set_style(Style::new().fg(fg));
                            }
                        }
                    }
                }
            }
            GraphGlyphs::Blocks => {
                let halves = h * 2;
                for (cx, &(m, s)) in cols.iter().enumerate() {
                    let hm = steps(m, peak, halves);
                    let hs = steps(s.min(m), peak, halves);
                    for cy in 0..h {
                        let base = (h - 1 - cy) * 2;
                        let ch = if hm >= base + 2 {
                            '█'
                        } else if hm == base + 1 {
                            '▄'
                        } else {
                            continue;
                        };
                        let fg = if hs > base { alt } else { main };
                        if let Some(cell) = buf.cell_mut((plot.x + cx as u16, plot.y + cy)) {
                            cell.set_char(ch).set_style(Style::new().fg(fg));
                        }
                    }
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testing::{assert_console_safe, buffer_text, render};

    fn day() -> Vec<(f64, f64)> {
        (0..144)
            .map(|i| {
                let x = i as f64 / 143.0 * std::f64::consts::TAU;
                let total = 100.0 + 80.0 * (x - 1.0).sin().max(-0.9);
                (total, total * 0.25)
            })
            .collect()
    }

    fn graph(theme: &Theme) -> Buffer {
        let data = day();
        render(30, 7, |f| {
            let area = f.area();
            let inner = pane(theme, "queries").inner(area);
            f.render_widget(pane(theme, "queries"), area);
            f.render_widget(HistoryGraph::new(theme, &data).axis(true), inner);
        })
    }

    #[test]
    fn graph_truecolor() {
        let buf = graph(&Theme::truecolor());
        insta::assert_snapshot!(buffer_text(&buf));
    }

    #[test]
    fn graph_basic16() {
        let buf = graph(&Theme::basic16());
        assert_console_safe(&buf);
        insta::assert_snapshot!(buffer_text(&buf));
    }

    #[test]
    fn graph_colors_blocked_part() {
        let t = Theme::basic16();
        let data = vec![(10.0, 5.0); 4];
        let buf = render(4, 2, |f| {
            f.render_widget(HistoryGraph::new(&t, &data), f.area())
        });
        assert_eq!(buf[(0, 1)].fg, t.graph_alt);
        assert_eq!(buf[(0, 0)].fg, t.graph_main);
    }

    #[test]
    fn graph_empty_data_draws_nothing() {
        let t = Theme::truecolor();
        let buf = render(10, 3, |f| {
            f.render_widget(HistoryGraph::new(&t, &[]), f.area())
        });
        assert_eq!(buffer_text(&buf).trim(), "");
    }

    fn meter(theme: &Theme) -> Buffer {
        render(24, 1, |f| {
            f.render_widget(Meter::new(theme, 0.247), f.area())
        })
    }

    #[test]
    fn meter_truecolor() {
        let t = Theme::truecolor();
        let buf = meter(&t);
        insta::assert_snapshot!(buffer_text(&buf));
        assert_eq!(buf[(0, 0)].fg, t.meter_color(0.5 / 18.0));
    }

    #[test]
    fn meter_basic16() {
        let t = Theme::basic16();
        let buf = meter(&t);
        assert_console_safe(&buf);
        insta::assert_snapshot!(buffer_text(&buf));
        assert_eq!(buf[(0, 0)].fg, Color::Green);
    }

    #[test]
    fn meter_clamps() {
        let t = Theme::basic16();
        for r in [-1.0, 2.0, f64::NAN] {
            render(10, 1, |f| f.render_widget(Meter::new(&t, r), f.area()));
        }
    }

    #[test]
    fn panes_share_borders() {
        let t = Theme::basic16();
        let buf = render(12, 3, |f| {
            f.render_widget(pane(&t, "a"), Rect::new(0, 0, 7, 3));
            f.render_widget(pane(&t, "b"), Rect::new(6, 0, 6, 3));
        });
        assert_console_safe(&buf);
        insta::assert_snapshot!(buffer_text(&buf));
    }

    #[test]
    fn fit_truncates_with_marker() {
        assert_eq!(fit("doubleclick.net", 20, '…'), "doubleclick.net");
        assert_eq!(fit("doubleclick.net", 8, '~'), "doublec~");
        assert_eq!(fit("abc", 0, '~'), "");
    }

    #[test]
    fn short_counts() {
        assert_eq!(short_count(180.0), "180");
        assert_eq!(short_count(1_234.0), "1.2k");
        assert_eq!(short_count(48_213.0), "48k");
        assert_eq!(short_count(1_200_000.0), "1.2M");
    }
}
