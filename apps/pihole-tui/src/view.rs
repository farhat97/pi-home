//! The 60x20 dashboard layout.
//!
//! ```text
//! ┌ pi-hole ──────────────────────────── [* o o] 42s ┐  rows 0..12: graph
//! │ queries/min · 24h            █ permitted █ blocked│
//! │180│        ▄██▄                                  │
//! │  0│▄▄▄▄███████████▄▄▄▄                           │
//! ├ today ─────────────────┬ top blocked ────────────┤  rows 11..18
//! │ queries        48,213  │ doubleclick.net    2,104│
//! │ blocked        11,902  │ ...                     │
//! │ ████░░░░░░░░░░░ 24.7%  │                         │
//! │ clients            17  │                         │
//! ├ status ────────────────┴─────────────────────────┤  rows 17..20
//! │ FTL ok  blocking on  gravity 1.2M (3h ago)  v6.1 │
//! └──────────────────────────────────────────────────┘
//! ```

use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use pihome_ui::widgets::{HistoryGraph, Meter, fit, pane, short_count};
use pihome_ui::{Indicator, Theme, chrome, size_guard};
use ratatui::Frame;
use ratatui::buffer::Buffer;
use ratatui::layout::{Alignment, Rect};
use ratatui::style::Style;
use ratatui::text::{Line, Span};
use ratatui::widgets::{Paragraph, Widget};

use crate::api::Blocking;
use crate::model::{Connection, Model};

pub const APP_NAME: &str = "pi-hole";

const TODAY_WIDTH: u16 = 26;
const MIDDLE_HEIGHT: u16 = 7;
const STATUS_HEIGHT: u16 = 3;

/// Inputs that don't come from the API.
pub struct Context<'a> {
    pub theme: &'a Theme,
    pub indicator: Option<Indicator>,
    /// Monotonic now, for the stale age.
    pub now: Instant,
    /// Wall-clock now, for the gravity age.
    pub wall: SystemTime,
    /// Shown on the waiting and auth-error screens.
    pub api_url: &'a str,
    pub refresh: Duration,
}

pub fn draw(frame: &mut Frame, model: &Model, cx: &Context) {
    let Some(area) = size_guard(frame, cx.theme) else {
        return;
    };
    let buf = frame.buffer_mut();
    match &model.connection {
        Connection::Waiting => message_screen(buf, area, cx, waiting_lines(model, cx)),
        Connection::AuthError(msg) => message_screen(buf, area, cx, auth_lines(msg, cx)),
        Connection::Ok | Connection::Stale => dashboard(buf, area, model, cx),
    }
}

fn message_screen(buf: &mut Buffer, area: Rect, cx: &Context, lines: Vec<Line<'static>>) {
    let block = chrome(cx.theme, APP_NAME, cx.indicator);
    let inner = block.inner(area);
    block.render(area, buf);
    let top = inner.y + inner.height.saturating_sub(lines.len() as u16) / 2;
    let rect = Rect::new(inner.x, top, inner.width, inner.bottom() - top);
    Paragraph::new(lines)
        .alignment(Alignment::Center)
        .render(rect, buf);
}

fn waiting_lines(model: &Model, cx: &Context) -> Vec<Line<'static>> {
    let t = cx.theme;
    let mut lines = vec![
        Line::styled("waiting for Pi-hole API", t.title_style()),
        Line::styled(
            format!("{}/api", cx.api_url.trim_end_matches('/')),
            t.text_style(),
        ),
        Line::styled(
            format!("retrying every {}s", cx.refresh.as_secs()),
            t.dim_style(),
        ),
    ];
    if model.ftl_down() {
        lines.insert(
            1,
            Line::styled("FTL down: API unreachable", t.alert_style()),
        );
    }
    lines
}

fn auth_lines(msg: &str, cx: &Context) -> Vec<Line<'static>> {
    let t = cx.theme;
    vec![
        Line::styled("authentication failed", t.alert_style()),
        Line::styled(msg.to_owned(), t.text_style()),
        Line::styled("check the password in the config file", t.dim_style()),
        Line::styled(
            format!("retrying every {}s", cx.refresh.as_secs()),
            t.dim_style(),
        ),
    ]
}

fn dashboard(buf: &mut Buffer, area: Rect, model: &Model, cx: &Context) {
    let t = cx.theme;
    // Panes overlap by one row/column so their borders merge.
    let status = Rect::new(
        area.x,
        area.bottom() - STATUS_HEIGHT,
        area.width,
        STATUS_HEIGHT,
    );
    let middle_y = status.y + 1 - MIDDLE_HEIGHT;
    let today = Rect::new(area.x, middle_y, TODAY_WIDTH, MIDDLE_HEIGHT);
    let top = Rect::new(
        area.x + TODAY_WIDTH - 1,
        middle_y,
        area.width - TODAY_WIDTH + 1,
        MIDDLE_HEIGHT,
    );
    let graph = Rect::new(area.x, area.y, area.width, middle_y + 1 - area.y);

    graph_pane(buf, graph, model, cx);
    today_pane(buf, today, model, t);
    top_pane(buf, top, model, t);
    status_pane(buf, status, model, cx);
}

fn graph_pane(buf: &mut Buffer, area: Rect, model: &Model, cx: &Context) {
    let t = cx.theme;
    let block = chrome(t, APP_NAME, cx.indicator);
    let inner = block.inner(area);
    block.render(area, buf);
    if inner.height < 2 {
        return;
    }

    let swatch = t.meter_full.to_string();
    let legend = Line::from(vec![
        Span::styled("queries/min · 24h", t.dim_style()),
        Span::raw("  "),
        Span::styled(swatch.clone(), Style::new().fg(t.graph_main)),
        Span::styled(" permitted ", t.dim_style()),
        Span::styled(swatch, Style::new().fg(t.graph_alt)),
        Span::styled(" blocked", t.dim_style()),
    ]);
    buf.set_line(inner.x + 1, inner.y, &legend, inner.width.saturating_sub(1));

    let plot = Rect::new(inner.x, inner.y + 1, inner.width, inner.height - 1);
    let data = per_minute(model);
    HistoryGraph::new(t, &data).axis(true).render(plot, buf);
}

/// History as `(queries/min, blocked/min)` per bucket.
fn per_minute(model: &Model) -> Vec<(f64, f64)> {
    let h = &model.history;
    let interval_s = match h.len() {
        0 | 1 => 600.0,
        n => ((h[n - 1].timestamp - h[0].timestamp) / (n - 1) as f64).max(1.0),
    };
    let per_min = interval_s / 60.0;
    h.iter()
        .map(|p| (p.total as f64 / per_min, p.blocked as f64 / per_min))
        .collect()
}

/// `48213` -> `48,213`.
pub fn thousands(n: u64) -> String {
    let s = n.to_string();
    let mut out = String::with_capacity(s.len() + s.len() / 3);
    for (i, c) in s.chars().enumerate() {
        if i > 0 && (s.len() - i).is_multiple_of(3) {
            out.push(',');
        }
        out.push(c);
    }
    out
}

/// Label on the left, value right-aligned to `width`.
fn row(label: &str, value: String, width: u16, t: &Theme) -> Line<'static> {
    let pad = (width as usize).saturating_sub(label.len() + value.len());
    Line::from(vec![
        Span::styled(label.to_owned(), t.dim_style()),
        Span::raw(" ".repeat(pad)),
        Span::styled(value, t.text_style()),
    ])
}

fn today_pane(buf: &mut Buffer, area: Rect, model: &Model, t: &Theme) {
    let block = pane(t, "today");
    let inner = block.inner(area);
    block.render(area, buf);
    let Some(s) = &model.summary else {
        return;
    };
    // One column of padding on each side.
    let content = Rect::new(
        inner.x + 1,
        inner.y,
        inner.width.saturating_sub(2),
        inner.height,
    );
    let w = content.width;
    let lines = [
        (0, row("queries", thousands(s.queries.total), w, t)),
        (1, row("blocked", thousands(s.queries.blocked), w, t)),
        (3, row("clients", thousands(s.clients.active), w, t)),
    ];
    for (dy, line) in lines {
        if dy < content.height {
            buf.set_line(content.x, content.y + dy, &line, w);
        }
    }
    if content.height > 2 {
        let meter_area = Rect::new(content.x, content.y + 2, w, 1);
        Meter::new(t, s.queries.percent_blocked / 100.0).render(meter_area, buf);
    }
}

fn top_pane(buf: &mut Buffer, area: Rect, model: &Model, t: &Theme) {
    let block = pane(t, "top blocked");
    let inner = block.inner(area);
    block.render(area, buf);
    let content = Rect::new(
        inner.x + 1,
        inner.y,
        inner.width.saturating_sub(2),
        inner.height,
    );

    let mut domains = model.top_blocked.clone();
    domains.sort_by_key(|d| std::cmp::Reverse(d.count));
    for (i, d) in domains.iter().take(content.height as usize).enumerate() {
        let count = thousands(d.count);
        let name_w = (content.width as usize).saturating_sub(count.len() + 1);
        let name = fit(&d.domain, name_w, t.ellipsis);
        let line = row(&name, count, content.width, t);
        let line = Line::from(
            line.spans
                .into_iter()
                .enumerate()
                .map(|(i, s)| if i == 0 { s.style(t.text_style()) } else { s })
                .collect::<Vec<_>>(),
        );
        buf.set_line(content.x, content.y + i as u16, &line, content.width);
    }
}

/// `45s`, `12m`, `3h`, `2d`.
pub fn ago(d: Duration) -> String {
    let s = d.as_secs();
    match s {
        0..60 => format!("{s}s"),
        60..3600 => format!("{}m", s / 60),
        3600..86400 => format!("{}h", s / 3600),
        _ => format!("{}d", s / 86400),
    }
}

fn status_pane(buf: &mut Buffer, area: Rect, model: &Model, cx: &Context) {
    let t = cx.theme;
    let mut block = pane(t, "status");
    if model.connection == Connection::Stale
        && let Some(age) = model.age(cx.now)
    {
        block = block.title_top(
            Line::from(Span::styled(
                format!(" stale · updated {} ago ", ago(age)),
                Style::new().fg(t.warn),
            ))
            .right_aligned(),
        );
    }
    let inner = block.inner(area);
    block.render(area, buf);

    let gap = || Span::raw("  ");
    let mut spans = vec![Span::raw(" ")];
    if model.ftl_down() {
        spans.push(Span::styled("FTL down", t.alert_style()));
    } else {
        spans.push(Span::styled("FTL ", t.dim_style()));
        spans.push(Span::styled("ok", Style::new().fg(t.ok)));
    }
    if let Some(b) = model.blocking {
        let (text, style) = match b {
            Blocking::Enabled => ("on", Style::new().fg(t.ok)),
            Blocking::Disabled => ("off", t.alert_style()),
            Blocking::Failed => ("failed", t.alert_style()),
            Blocking::Unknown => ("unknown", Style::new().fg(t.warn)),
        };
        spans.extend([
            gap(),
            Span::styled("blocking ", t.dim_style()),
            Span::styled(text, style),
        ]);
    }
    if let Some(s) = &model.summary {
        let g = &s.gravity;
        spans.extend([
            gap(),
            Span::styled("gravity ", t.dim_style()),
            Span::styled(
                short_count(g.domains_being_blocked.max(0) as f64),
                t.text_style(),
            ),
        ]);
        if g.last_update > 0 {
            let updated = UNIX_EPOCH + Duration::from_secs(g.last_update as u64);
            let age = cx.wall.duration_since(updated).unwrap_or_default();
            spans.push(Span::styled(format!(" ({} ago)", ago(age)), t.dim_style()));
        }
    }
    if let Some(v) = &model.version {
        spans.extend([gap(), Span::styled(v.clone(), t.dim_style())]);
    }
    buf.set_line(inner.x, inner.y, &Line::from(spans), inner.width);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::api::{
        ApiError, HistoryPoint, Summary, SummaryClients, SummaryGravity, SummaryQueries, TopDomain,
    };
    use crate::model::Update;
    use pihome_ui::testing::{assert_console_safe, buffer_text, render};

    const WALL: u64 = 1_725_205_439; // 3 h after the gravity update below

    fn healthy(t0: Instant) -> Model {
        let mut m = Model::default();
        let history = (0..144)
            .map(|i| {
                let x = i as f64 / 143.0 * std::f64::consts::TAU;
                let total = (900.0 + 700.0 * (x - 2.0).sin()) as u64;
                HistoryPoint {
                    timestamp: 1_725_120_000.0 + i as f64 * 600.0,
                    total,
                    blocked: total / 4,
                }
            })
            .collect();
        let top = [
            ("doubleclick.net", 2104),
            ("app-measurement.com", 1388),
            ("graph.facebook.com", 907),
            ("ads.roku.com", 611),
            ("telemetry.example.org", 402),
            ("not-shown.example", 1),
        ]
        .into_iter()
        .map(|(d, c)| TopDomain {
            domain: d.into(),
            count: c,
        })
        .collect();
        m.apply(
            Ok(Update {
                summary: Some(Summary {
                    queries: SummaryQueries {
                        total: 48213,
                        blocked: 11902,
                        percent_blocked: 24.686,
                    },
                    clients: SummaryClients { active: 17 },
                    gravity: SummaryGravity {
                        domains_being_blocked: 1_204_311,
                        last_update: 1_725_194_639,
                    },
                }),
                blocking: Some(Blocking::Enabled),
                top_blocked: Some(top),
                history: Some(history),
                version: Some("v6.1".into()),
            }),
            t0,
        );
        m
    }

    fn screen(model: &Model, theme: &Theme, now: Instant, indicator: Option<Indicator>) -> Buffer {
        let cx = Context {
            theme,
            indicator,
            now,
            wall: UNIX_EPOCH + Duration::from_secs(WALL),
            api_url: "http://127.0.0.1",
            refresh: Duration::from_secs(5),
        };
        render(60, 20, |f| draw(f, model, &cx))
    }

    fn rotating() -> Option<Indicator> {
        Some(Indicator {
            active: 0,
            count: 3,
            next_in_s: 42,
            pinned: false,
        })
    }

    fn basic(model: &Model, now: Instant) -> String {
        let buf = screen(model, &Theme::basic16(), now, rotating());
        assert_console_safe(&buf);
        buffer_text(&buf)
    }

    #[test]
    fn healthy_basic16() {
        let t0 = Instant::now();
        let text = basic(&healthy(t0), t0);
        assert!(text.contains("48,213") && text.contains("11,902") && text.contains("24.7%"));
        assert!(text.contains("FTL ok") && text.contains("v6.1") && text.contains("(3h ago)"));
        assert!(text.contains("[* o o] 42s"));
        assert!(!text.contains("not-shown"), "only as many domains as fit");
        insta::assert_snapshot!(text);
    }

    #[test]
    fn healthy_truecolor() {
        let t0 = Instant::now();
        let buf = screen(&healthy(t0), &Theme::truecolor(), t0, rotating());
        insta::assert_snapshot!(buffer_text(&buf));
    }

    #[test]
    fn ftl_down_is_alert_colored_and_stale() {
        let t0 = Instant::now();
        let mut m = healthy(t0);
        m.apply(
            Err(ApiError::Unreachable("refused".into())),
            t0 + Duration::from_secs(5),
        );
        let theme = Theme::basic16();
        let held = Some(Indicator {
            active: 0,
            count: 3,
            next_in_s: 0,
            pinned: true,
        });
        let buf = screen(&m, &theme, t0 + Duration::from_secs(65), held);
        assert_console_safe(&buf);
        let text = buffer_text(&buf);
        assert!(text.contains("FTL down") && text.contains("[* o o] hold"));
        assert!(text.contains("stale · updated 1m ago"));
        assert!(text.contains("48,213"), "last values stay on screen");
        let (x, y) = find(&text, "FTL down");
        assert_eq!(buf[(x, y)].fg, theme.alert);
        insta::assert_snapshot!(text);
    }

    #[test]
    fn stale_after_server_error() {
        let t0 = Instant::now();
        let mut m = healthy(t0);
        m.apply(
            Err(ApiError::Other("HTTP 500".into())),
            t0 + Duration::from_secs(5),
        );
        let text = basic(&m, t0 + Duration::from_secs(45));
        assert!(text.contains("stale · updated 45s ago"));
        assert!(text.contains("FTL ok"));
        insta::assert_snapshot!(text);
    }

    #[test]
    fn blocking_disabled_is_alert_colored() {
        let t0 = Instant::now();
        let mut m = healthy(t0);
        m.apply(
            Ok(Update {
                blocking: Some(Blocking::Failed),
                ..Default::default()
            }),
            t0,
        );
        let theme = Theme::basic16();
        let buf = screen(&m, &theme, t0, None);
        let text = buffer_text(&buf);
        let (x, y) = find(&text, "failed");
        assert_eq!(buf[(x, y)].fg, theme.alert);
    }

    #[test]
    fn waiting() {
        let t0 = Instant::now();
        let mut m = Model::default();
        m.apply(Err(ApiError::Unreachable("refused".into())), t0);
        let text = basic(&m, t0);
        assert!(text.contains("waiting for Pi-hole API") && text.contains("http://127.0.0.1/api"));
        insta::assert_snapshot!(text);
    }

    #[test]
    fn auth_error() {
        let t0 = Instant::now();
        let mut m = Model::default();
        m.apply(Err(ApiError::Auth("password incorrect".into())), t0);
        let text = basic(&m, t0);
        assert!(text.contains("authentication failed") && text.contains("password incorrect"));
        assert!(!text.contains("queries"));
        insta::assert_snapshot!(text);
    }

    #[test]
    fn long_domains_truncate_and_keep_counts() {
        let t0 = Instant::now();
        let mut m = healthy(t0);
        m.top_blocked = vec![
            TopDomain {
                domain: "a-very-long-tracking-subdomain.ads.example-network.com".into(),
                count: 123_456,
            },
            TopDomain {
                domain: "short.io".into(),
                count: 9,
            },
        ];
        let text = basic(&m, t0);
        let line = text
            .lines()
            .find(|l| l.contains("123,456"))
            .expect("count visible");
        assert!(
            line.contains("a-very-long-tracking") && line.contains("~ 123,456"),
            "{line}"
        );
        insta::assert_snapshot!(text);
    }

    #[test]
    fn too_small() {
        let t0 = Instant::now();
        let cx = Context {
            theme: &Theme::basic16(),
            indicator: None,
            now: t0,
            wall: SystemTime::now(),
            api_url: "x",
            refresh: Duration::from_secs(5),
        };
        let buf = render(59, 20, |f| draw(f, &healthy(t0), &cx));
        assert!(buffer_text(&buf).contains("have 59x20"));
    }

    #[test]
    fn helpers() {
        assert_eq!(thousands(0), "0");
        assert_eq!(thousands(999), "999");
        assert_eq!(thousands(48213), "48,213");
        assert_eq!(thousands(1_234_567), "1,234,567");
        assert_eq!(ago(Duration::from_secs(45)), "45s");
        assert_eq!(ago(Duration::from_secs(10_800)), "3h");
        assert_eq!(ago(Duration::from_secs(200_000)), "2d");
    }

    /// Cell position of the first occurrence of `needle` in rendered text.
    fn find(text: &str, needle: &str) -> (u16, u16) {
        for (y, line) in text.lines().enumerate() {
            if let Some(byte) = line.find(needle) {
                return (line[..byte].chars().count() as u16, y as u16);
            }
        }
        panic!("{needle:?} not on screen");
    }
}
