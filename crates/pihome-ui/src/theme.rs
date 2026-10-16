//! One theme for every app, in a truecolor and a Linux-console variant.

use ratatui::style::{Color, Modifier, Style};
use ratatui::symbols::border;

/// Which palette and glyph set to draw with.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ColorMode {
    /// 24-bit color, rounded borders, braille graphs.
    TrueColor,
    /// 16 ANSI colors and glyphs present in standard console fonts.
    Basic16,
}

impl ColorMode {
    /// Picks the mode from the environment of the current process.
    pub fn detect() -> Self {
        let var = |k| std::env::var(k).ok();
        Self::from_env(
            var("PIHOME_COLOR").as_deref(),
            var("COLORTERM").as_deref(),
            var("TERM").as_deref(),
        )
    }

    /// `PIHOME_COLOR` (`truecolor` or `16`) overrides detection. Otherwise
    /// truecolor needs `COLORTERM=truecolor|24bit` and a terminal other than
    /// the Linux console.
    pub fn from_env(force: Option<&str>, colorterm: Option<&str>, term: Option<&str>) -> Self {
        match force.map(str::to_ascii_lowercase).as_deref() {
            Some("truecolor" | "24bit") => return Self::TrueColor,
            Some("16" | "basic") => return Self::Basic16,
            _ => {}
        }
        let term = term.unwrap_or("");
        if term == "linux" || term == "dumb" || term.is_empty() {
            return Self::Basic16;
        }
        match colorterm.map(str::to_ascii_lowercase).as_deref() {
            Some("truecolor" | "24bit") => Self::TrueColor,
            _ if term.ends_with("-direct") => Self::TrueColor,
            _ => Self::Basic16,
        }
    }
}

/// How history graphs are drawn.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GraphGlyphs {
    /// 2x4 dots per cell.
    Braille,
    /// Full and lower-half blocks: two steps per cell.
    Blocks,
}

/// Colors and glyphs shared by all apps.
#[derive(Debug, Clone)]
pub struct Theme {
    pub mode: ColorMode,
    pub border: Color,
    pub title: Color,
    pub text: Color,
    pub dim: Color,
    /// Highlight for the active rotation marker and similar accents.
    pub accent: Color,
    pub ok: Color,
    pub warn: Color,
    pub alert: Color,
    /// Graph color for permitted queries (or the primary series).
    pub graph_main: Color,
    /// Graph color for blocked queries (or the secondary series).
    pub graph_alt: Color,
    /// Meter gradient from empty to full: low, mid, high.
    pub meter: [Color; 3],
    pub border_set: border::Set<'static>,
    pub graph: GraphGlyphs,
    pub meter_full: char,
    pub meter_empty: char,
    /// Marks text cut short to fit.
    pub ellipsis: char,
}

/// Characters guaranteed by standard Linux console fonts (Lat15/Terminus):
/// printable ASCII, light box drawing, and the CP437 block elements.
pub const CONSOLE_SAFE_EXTRA: &str = "─│┌┐└┘├┤┬┴┼█▄▀░▒▓·";

/// True if `s` only uses characters from [`CONSOLE_SAFE_EXTRA`] or ASCII.
pub fn is_console_safe(s: &str) -> bool {
    s.chars()
        .all(|c| (' '..='~').contains(&c) || CONSOLE_SAFE_EXTRA.contains(c))
}

impl Theme {
    pub fn detect() -> Self {
        Self::for_mode(ColorMode::detect())
    }

    pub fn for_mode(mode: ColorMode) -> Self {
        match mode {
            ColorMode::TrueColor => Self::truecolor(),
            ColorMode::Basic16 => Self::basic16(),
        }
    }

    pub fn truecolor() -> Self {
        Self {
            mode: ColorMode::TrueColor,
            border: Color::Rgb(0x5c, 0x63, 0x70),
            title: Color::Rgb(0xe6, 0xe6, 0xe6),
            text: Color::Rgb(0xcc, 0xcc, 0xcc),
            dim: Color::Rgb(0x7a, 0x7f, 0x88),
            accent: Color::Rgb(0x5f, 0xd7, 0xff),
            ok: Color::Rgb(0x6c, 0xd0, 0x6c),
            warn: Color::Rgb(0xe5, 0xc0, 0x4b),
            alert: Color::Rgb(0xff, 0x55, 0x55),
            graph_main: Color::Rgb(0x4e, 0xa8, 0xde),
            graph_alt: Color::Rgb(0xe0, 0x6c, 0x75),
            meter: [
                Color::Rgb(0x6c, 0xd0, 0x6c),
                Color::Rgb(0xe5, 0xc0, 0x4b),
                Color::Rgb(0xff, 0x55, 0x55),
            ],
            border_set: border::ROUNDED,
            graph: GraphGlyphs::Braille,
            meter_full: '■',
            meter_empty: '·',
            ellipsis: '…',
        }
    }

    pub fn basic16() -> Self {
        Self {
            mode: ColorMode::Basic16,
            border: Color::DarkGray,
            title: Color::White,
            text: Color::Gray,
            dim: Color::DarkGray,
            accent: Color::LightCyan,
            ok: Color::Green,
            warn: Color::Yellow,
            alert: Color::LightRed,
            graph_main: Color::Cyan,
            graph_alt: Color::Red,
            meter: [Color::Green, Color::Yellow, Color::Red],
            border_set: border::PLAIN,
            graph: GraphGlyphs::Blocks,
            meter_full: '█',
            meter_empty: '░',
            ellipsis: '~',
        }
    }

    pub fn border_style(&self) -> Style {
        Style::new().fg(self.border)
    }

    pub fn title_style(&self) -> Style {
        Style::new().fg(self.title).add_modifier(Modifier::BOLD)
    }

    pub fn text_style(&self) -> Style {
        Style::new().fg(self.text)
    }

    pub fn dim_style(&self) -> Style {
        Style::new().fg(self.dim)
    }

    pub fn alert_style(&self) -> Style {
        Style::new().fg(self.alert).add_modifier(Modifier::BOLD)
    }

    /// Meter color at position `t` in `0.0..=1.0` along the gradient.
    pub fn meter_color(&self, t: f64) -> Color {
        let t = t.clamp(0.0, 1.0);
        match self.mode {
            ColorMode::Basic16 => self.meter[((t * 3.0) as usize).min(2)],
            ColorMode::TrueColor => {
                let (a, b, u) = if t < 0.5 {
                    (self.meter[0], self.meter[1], t * 2.0)
                } else {
                    (self.meter[1], self.meter[2], (t - 0.5) * 2.0)
                };
                lerp(a, b, u)
            }
        }
    }
}

fn lerp(a: Color, b: Color, t: f64) -> Color {
    match (a, b) {
        (Color::Rgb(r1, g1, b1), Color::Rgb(r2, g2, b2)) => {
            let m = |x: u8, y: u8| (x as f64 + (y as f64 - x as f64) * t).round() as u8;
            Color::Rgb(m(r1, r2), m(g1, g2), m(b1, b2))
        }
        _ => a,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mode_selection() {
        use ColorMode::*;
        let f = ColorMode::from_env;
        assert_eq!(
            f(None, Some("truecolor"), Some("xterm-256color")),
            TrueColor
        );
        assert_eq!(f(None, Some("24bit"), Some("tmux-256color")), TrueColor);
        assert_eq!(f(None, None, Some("xterm-256color")), Basic16);
        assert_eq!(f(None, None, Some("xterm-direct")), TrueColor);
        // The Linux console never gets truecolor, even if COLORTERM leaks in.
        assert_eq!(f(None, Some("truecolor"), Some("linux")), Basic16);
        assert_eq!(f(None, Some("truecolor"), None), Basic16);
        assert_eq!(f(Some("16"), Some("truecolor"), Some("xterm")), Basic16);
        assert_eq!(f(Some("truecolor"), None, Some("linux")), TrueColor);
        assert_eq!(f(Some("bogus"), None, Some("linux")), Basic16);
    }

    #[test]
    fn basic16_uses_only_console_safe_glyphs_and_ansi_colors() {
        let t = Theme::basic16();
        assert_eq!(t.graph, GraphGlyphs::Blocks);
        let s = t.border_set;
        for g in [
            s.top_left,
            s.top_right,
            s.bottom_left,
            s.bottom_right,
            s.vertical_left,
            s.vertical_right,
            s.horizontal_top,
            s.horizontal_bottom,
        ] {
            assert!(is_console_safe(g), "border glyph {g:?}");
        }
        for c in [t.meter_full, t.meter_empty, t.ellipsis] {
            assert!(is_console_safe(&c.to_string()), "glyph {c:?}");
        }
        for c in [
            t.border, t.title, t.text, t.dim, t.accent, t.ok, t.warn, t.alert,
        ]
        .into_iter()
        .chain(t.meter)
        .chain([t.graph_main, t.graph_alt])
        {
            assert!(!matches!(c, Color::Rgb(..) | Color::Indexed(_)), "{c:?}");
        }
        for t in [0.0, 0.3, 0.5, 0.9, 1.0] {
            assert!(!matches!(Theme::basic16().meter_color(t), Color::Rgb(..)));
        }
    }

    #[test]
    fn braille_is_not_console_safe() {
        assert!(!is_console_safe("⣿"));
        assert!(!is_console_safe("╭"));
        assert!(is_console_safe("┌─┐ █▄ ok"));
    }

    #[test]
    fn gradient_endpoints() {
        let t = Theme::truecolor();
        assert_eq!(t.meter_color(0.0), t.meter[0]);
        assert_eq!(t.meter_color(0.5), t.meter[1]);
        assert_eq!(t.meter_color(1.0), t.meter[2]);
    }
}
