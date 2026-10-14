# Proposal

## Why

The Raspberry Pi 4B runs Pi-hole for network-wide ad blocking, and its 3.5" GPIO screen is the natural place to see what Pi-hole is doing at a glance. We want a btop-style terminal dashboard for it, built on a foundation that later apps (CTA train tracker, home network info) can plug into without redesign. A 480x320 screen is too small for side-by-side panels, so each app runs fullscreen and the screen rotates between them.

## What Changes

- New Rust cargo workspace for all on-device software.
- New shared UI crate (`pihome-ui`): theme, bordered panes with titles in the border, gradient meters, graph helpers, and the common header chrome every app renders.
- New rotator (`pihome-rotator`): drives a tmux session with one fullscreen window per app, switches windows on a schedule, publishes rotation state for apps to display, and reacts to urgent signals from apps.
- New Pi-hole dashboard app (`pihole-tui`): reads the Pi-hole v6 REST API and shows query history, today's totals, block rate, top blocked domains, and service status.
- New deploy assets: systemd units, tmux config, and an install script that starts the session on the Pi's console at boot.

## Capabilities

### New Capabilities
- `pihole-dashboard`: Fullscreen terminal display of Pi-hole metrics and health, sourced from the Pi-hole v6 API.
- `screen-rotation`: Running multiple fullscreen apps on the device display, rotating between them on a schedule, and letting apps request attention with urgent signals.
- `display-chrome`: Shared look and header shown by every app, including which screen is active and the time left before the next rotation.

### Modified Capabilities
<!-- None: no existing specs. -->

## Impact

- **New code:** cargo workspace under the repo root (`crates/`, `apps/`), plus `deploy/`.
- **Dependencies:** `ratatui`, `crossterm`, `tokio`, `reqwest`, `serde`; `tmux` on the device; build target `aarch64-unknown-linux-gnu`.
- **Systems:** Pi-hole v6 must already be installed on the Pi; the dashboard takes over `tty1` at boot.
- **Secrets:** a Pi-hole app password is stored in on-device config.
