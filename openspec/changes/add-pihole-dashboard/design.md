# Design

## Context

See proposal.md for motivation. The hardware sets the constraints:

- Raspberry Pi 4B running Pi-hole v6 locally.
- 3.5" GPIO display, 480x320 px. The terminal grid is about **60 cols x 20 rows** with an 8x16 font (80x26 with 6x12, which is hard to read at arm's length). Every layout targets 60x20.
- No keyboard attached in normal use. The display is passive.
- More apps (CTA train tracker, home network info) will follow as separate changes and must fit in without reworking this foundation.

The visual reference is **btop**: boxed panes with titles and labels set into the border, braille history graphs, gradient meters, dense right-aligned numbers, and one theme driving all colors.

## Goals / Non-Goals

**Goals:**
- A foundation (workspace, UI kit, rotator, deploy) that later apps reuse unchanged.
- Apps that run standalone in any 60x20 terminal (laptop, SSH) for development.
- A consistent btop-like look across independently built apps.

**Non-Goals:**
- Installing or configuring Pi-hole itself. It is a prerequisite, installed with its official installer.
- Touch input, time-of-day weighted schedules, and the CTA/network apps. Those come in later changes. The rotator's design leaves room for them.
- Display driver and console font setup beyond documenting what was used.

## Decisions

### D1. One fullscreen app per screen instead of panels in one screen

At 60x20, three side-by-side panels would leave each one about 20 columns wide, which is too narrow to be useful. Each app owns the whole screen.

### D2. tmux session + separate rotator process

```
 systemd
  +-- pihome-session.service   tmux session "pihome" on tty1, status bar off
  |      +-- win 0: pihole-tui
  |      +-- win 1: net-tui      (later change)
  |      +-- win 2: cta-tui      (later change)
  +-- pihome-rotator.service   tmux select-window on a schedule
```

- **Chosen because:** `tmux attach` over SSH shows exactly what's on the screen, each app crashes and restarts on its own, no root is needed to switch, and rotating is a plain tmux command.
- **Alternative, Linux VTs + `chvt`:** native, but switching needs root and you can't see the live screen over SSH.
- **Alternative, one app with multiple screens:** allows smoother transitions, but couples every app's failures and grows poorly into independent apps.

The session runs as a systemd service bound to `tty1` (`TTYPath=/dev/tty1`, conflicts with `getty@tty1`). Each tmux window runs its app in a respawn loop so a crashed app comes back.

### D3. Rotator is its own process and owns all rotation logic

The apps don't know about each other. The rotator does the switching and talks to the apps only through files under `/run/pihome/`:

```
 rotator --writes--> /run/pihome/state.json
    |                  { "active": 0, "count": 3, "next_in_s": 42, "pinned": false }
    |                        ^
    | tmux select-window     | apps read it to draw the header chrome
    v                        |
 [pihole-tui] [net-tui] [cta-tui] --write--> /run/pihome/signals/<app>.json
                                             { "level": "urgent", "reason": "FTL down", "ttl_s": 60 }
```

- **Schedule:** a list of windows with a dwell time per window, set in the rotator config. With only `pihole-tui` installed, the rotator still runs and publishes state. Switching is then a no-op.
- **Interrupts:** a live urgent signal makes the rotator jump to that app's window and hold it while the signal is live (until its `ttl_s` expires or the app clears it).
- **Writes are atomic** (write to a temp file, then rename), so readers never see half a file.
- **Why files instead of a socket:** easy to inspect with `cat`, no server lifecycle to manage, and an app keeps working if the rotator is down. In that case the header just hides the indicator.
- Time-of-day weighting (e.g. more CTA during commutes) is left for the CTA change. It fits into the schedule config without changing this contract.

### D4. Rust + ratatui, cargo workspace

```
pi-home/
+-- Cargo.toml            (workspace)
+-- crates/
|   +-- pihome-ui/        theme, panes, meters, graphs, header chrome, state reader
|   +-- pihome-rotator/   schedule, tmux control, signals, state writer
+-- apps/
|   +-- pihole-tui/
+-- deploy/               systemd units, tmux.conf, install script, example configs
```

- **ratatui** is the closest to btop: it has braille `Canvas`/`Chart`, `Sparkline`, `Gauge`, and titles in block borders built in. It uses `crossterm` as the backend.
- `tokio` + `reqwest` for API polling, and `serde` for config and state.
- Builds are cross-compiled to `aarch64-unknown-linux-gnu` (e.g. with `cross`) and copied to the Pi as single binaries. Building on the Pi also works, just more slowly.
- **Alternatives:** Go + Bubble Tea (charts need third-party libraries) and Python + Textual (heavier on the Pi and less btop-like).

### D5. Shared look lives in `pihome-ui`

Visual consistency comes from one crate, not from each app copying styles:
- **One theme struct**, with a 16-color fallback chosen when the terminal doesn't advertise truecolor. This follows btop's own TTY mode: braille and gradients where available, block characters and flat colors otherwise.
- **Header chrome:** the app name in the top border on the left, and the rotation indicator `[* o o]` plus seconds until the next switch on the right, read from `state.json`.
- Shared widgets for gradient meters, titled panes, and graph helpers.

### D6. Pi-hole data from the v6 REST API

- Pi-hole v6 serves its API at `/api` on the local web server. Auth is session-based: a password or **app password** is exchanged for a session ID. The app uses an app password from its config file. The older v5 `api.php` is not supported.
- **Polling cadence:** summary and status every few seconds, and the 24h history at its natural granularity (on the order of minutes). The render tick is 1 s so the countdown stays smooth.
- If the API is unreachable, the screen stays up and shows the last known values marked stale. If FTL is down, or the API has been unreachable for more than 30 s, the app raises an urgent signal and renews it while the failure lasts.

### Mockups

These are kept as the reference for this and later changes. They are drawn in ASCII. The real UI uses rounded borders and braille graphs.

**Pi-hole (this change)**
```
+- pi-hole ------------------------------ [* o o] 42s -+
| queries/min                                            |
| 180|        .:|:.                 .                    |
|    |    .:|:|||||:.        ..:|:|||:.                  |
|   0|..:||||||||||||||:....:||||||||||||:...            |
|    +-- 24h ------------------------------------------- |
+- today ----------------+- top blocked -----------------+
| queries     48,213     | doubleclick.net        2,104  |
| blocked     11,902     | app-measurement.com    1,388  |
| [|||||||||.......] 24.7%| graph.facebook.com       907  |
| clients         17     | ads.roku.com             611  |
+- status -------------------------------------------- --+
| FTL ok   gravity 1.2M domains   updated 3h ago   v6.1  |
+--------------------------------------------------------+
```

**CTA trains (future change, kept for reference)**
```
+- cta --------------------------------- [o o *] 18s -+
| BELMONT (Red/Brown/Purple)                            |
+-------------------------------------------------------+
| RED  > Howard          2 min   [||||||||||||......]   |
| RED  > Howard         11 min                          |
| RED  < 95th/Dan Ryan   4 min   [||||||||..........]   |
| BRN  > Kimball         7 min                          |
| BRN  < Loop           13 min                          |
+- alerts ----------------------------------------------+
| ! Red Line: minor delays near Jackson                 |
+-------------------------------------------------------+
```

**Context-aware rotation (future direction, kept for reference)**
```
              +---------------- rotator ----------------+
              |  base schedule:                         |
              |    07:00-09:00, 16:30-18:30 -> CTA x3   |
              |    otherwise -> equal weight            |
              |                                         |
 signals ---> |  interrupts:                            |
              |    Pi-hole down        -> pin pihole    |
              |    train due < 6 min   -> jump to CTA   |
              |    new device on LAN   -> jump to net   |
              +-----------------------------------------+
```

## Risks / Trade-offs

- **[Linux console limits braille and colors]** → The theme falls back to 16 colors and block characters (D5). A richer console (`kmscon`) or kiosk terminal can be added later without changing the apps.
- **[A 60x20 layout breaks if the font or size changes]** → Apps render against the actual terminal size, and each one shows a clear "terminal too small" message below 60x20 instead of a garbled layout.
- **[Pi-hole v6 API changes between minor versions]** → Keep the API client in its own module and show the Pi-hole version in the status pane.
- **[The rotator crashes or stops]** → Apps keep running and hide the rotation indicator when `state.json` is stale. systemd restarts the rotator.
- **[App password stored on disk]** → The config file is readable only by the service user (mode 0600). An app password can be revoked without touching the admin password.

## Migration Plan

This is a fresh install, so there's nothing to migrate. Deploy: install Pi-hole v6 → run the install script (copies binaries, configs, systemd units, `tmux.conf`) → enable `pihome-session` and `pihome-rotator`. Rollback: disable both units and re-enable `getty@tty1`.

## Open Questions

- Exact dwell time default (around 60 s assumed) and polling intervals. These get tuned on the device.
- Console font choice for the SPI display (Terminus 8x16 is the likely default).
