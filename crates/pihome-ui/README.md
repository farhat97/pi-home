# pihome-ui

Shared look and rotation contract for pi-home apps. Every app on the device
uses this crate so independently built screens read as one gadget.

| Module     | What it gives you |
|------------|-------------------|
| `theme`    | `Theme` with a truecolor palette (rounded borders, braille graphs) and a 16-color Linux-console fallback (plain borders, block characters). `Theme::detect()` picks one from `PIHOME_COLOR`, `COLORTERM` and `TERM`. |
| `chrome`   | `chrome()`: the outer pane with the app name top-left and the rotation indicator (`[* o o] 42s`, or `hold`) top-right. `Indicator::read()` returns `None` when the rotator is absent or its state is older than 5 s, which hides the indicator. |
| `guard`    | `size_guard()`: returns the drawing area at 60x20 or larger, otherwise draws "terminal too small" with the required and current size. |
| `widgets`  | `pane()` (titled pane; overlapping borders merge), `Meter` (gradient meter with a label), `HistoryGraph` (main series with a highlighted sub-series and optional peak/zero axis), `fit()` (truncate with a marker). |
| `contract` | `StateDir` for `/run/pihome/`: read `state.json`, write or clear `signals/<app>.json` (atomic writes, tolerant reads), and `Signal::urgent()`. |
| `testing`  | `render()`, `buffer_text()` and `assert_console_safe()` for `TestBackend` snapshot tests. |

## Using it in an app

Draw every frame through the size guard, then wrap the layout in the chrome:

```rust
use std::time::SystemTime;

use pihome_ui::widgets::{HistoryGraph, Meter, pane};
use pihome_ui::{Indicator, StateDir, Theme, chrome, size_guard};
use ratatui::Frame;
use ratatui::layout::{Constraint, Layout};

struct App {
    theme: Theme,
    state_dir: StateDir,
    history: Vec<(f64, f64)>,
    block_ratio: f64,
}

fn draw(frame: &mut Frame, app: &App) {
    // Below 60x20 this draws the "terminal too small" message instead.
    let Some(area) = size_guard(frame, &app.theme) else {
        return;
    };

    // App name on the left; rotation indicator on the right while the
    // rotator is running.
    let indicator = Indicator::read(&app.state_dir, SystemTime::now());
    let outer = chrome(&app.theme, "my-app", indicator);
    let inner = outer.inner(area);
    frame.render_widget(outer, area);

    let [graph, meter] = Layout::vertical([Constraint::Fill(1), Constraint::Length(3)]).areas(inner);
    frame.render_widget(HistoryGraph::new(&app.theme, &app.history).axis(true), graph);

    let block = pane(&app.theme, "rate");
    frame.render_widget(Meter::new(&app.theme, app.block_ratio), block.inner(meter));
    frame.render_widget(block, meter);
}

# let app = App {
#     theme: Theme::detect(),
#     state_dir: StateDir::new("/nonexistent"),
#     history: vec![(10.0, 2.0); 144],
#     block_ratio: 0.25,
# };
# for (w, h) in [(60, 20), (59, 20)] {
#     pihome_ui::testing::render(w, h, |f| draw(f, &app));
# }
```

To ask the rotator for the screen, write a signal and keep renewing it while
the problem lasts; clear it once things are healthy:

```rust,no_run
use std::time::SystemTime;
use pihome_ui::{Signal, StateDir};

let dir = StateDir::default(); // /run/pihome
dir.write_signal("my-app", &Signal::urgent("backend down", 60, SystemTime::now()))?;
// ...later
dir.clear_signal("my-app")?;
# Ok::<(), std::io::Error>(())
```

The signal name must match the app's `name` in the rotator config. When
renewing, keep the original `raised_ms` and update `renewed_ms` so the rotator
can order competing signals by who asked first.

## Rules for new apps

- Lay out for exactly 60x20; extra space is a bonus, not a target.
- Take every color and glyph from `Theme`. Never hard-code braille or rounded
  corners: the Linux console font doesn't have them.
- Test both `Theme::truecolor()` and `Theme::basic16()` with
  `testing::render`, and call `assert_console_safe` on the 16-color buffer.
