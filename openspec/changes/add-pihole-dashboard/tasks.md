# Tasks

## 1. Workspace setup

- [ ] 1.1 Create the cargo workspace (`Cargo.toml`, `crates/pihome-ui`, `crates/pihome-rotator`, `apps/pihole-tui`) with shared dependency versions for ratatui, crossterm, tokio, reqwest, serde; verify `cargo build --workspace` succeeds
- [ ] 1.2 Add cross-compilation setup for `aarch64-unknown-linux-gnu` (e.g. `cross` config) and verify `cross build --release --target aarch64-unknown-linux-gnu --workspace` produces the three binaries

## 2. Rotation state and signal contract

- [ ] 2.1 Define the rotation state (`active`, `count`, `next_in_s`, `pinned`, update timestamp) and urgent signal (`level`, `reason`, `ttl_s`, raised-at timestamp) types with serde, shared by the rotator and apps; verify with round-trip serialization unit tests
- [ ] 2.2 Implement atomic write (temp file + rename) and tolerant readers for state and signal files under a configurable base dir (default `/run/pihome/`); verify with tests that a concurrent reader never sees a partial file and that missing or corrupt files read as "absent"
- [ ] 2.3 Implement freshness checks: state older than 5 s is stale, signals past `ttl_s` are expired; verify with unit tests using an injected clock

## 3. Shared UI kit (`pihome-ui`)

- [ ] 3.1 Implement the theme with truecolor and 16-color palettes, picked from `COLORTERM`/terminal capabilities; verify with unit tests for selection logic and a test asserting the 16-color mode uses only console-safe glyphs (no braille)
- [ ] 3.2 Implement titled pane, gradient meter, and history graph helpers (braille in truecolor mode, block characters in fallback); verify with ratatui `TestBackend` snapshot tests in both modes
- [ ] 3.3 Implement the header chrome: app name left; `[* o o] 42s` right from rotation state; hold marker when pinned; indicator hidden when state is absent or stale; verify with `TestBackend` snapshot tests for each case
- [ ] 3.4 Implement the 60x20 size guard that renders a "terminal too small" message with required and current size; verify with `TestBackend` tests at 60x20 and 59x20
- [ ] 3.5 Document how an app uses the UI kit (theme, chrome, size guard) in `crates/pihome-ui/README.md`; verify the documented snippet compiles as a doc test

## 4. Rotator (`pihome-rotator`)

- [ ] 4.1 Implement rotator config (ordered app list with tmux window, per-app dwell time defaulting to 60 s, state dir, tmux session name) loaded from TOML; verify with parsing tests including defaults
- [ ] 4.2 Implement the schedule state machine (advance on dwell expiry, wrap around, single-app no-op, hold on urgent signal, earliest-raised-first ordering, resume with fresh dwell on clear or expiry); verify with unit tests driven by an injected clock and fake signals covering every screen-rotation scenario
- [ ] 4.3 Implement tmux control behind a trait (`select-window` on session `pihome`); verify the state machine drives a fake implementation with the expected calls, and that a real tmux call runs against a throwaway session in an integration test
- [ ] 4.4 Wire the main loop: tick at least once per second, scan signals, switch windows, write state atomically; verify by running locally against a test tmux session with two dummy windows and observing switches and `state.json` updates
- [ ] 4.5 Add an example config at `deploy/rotator.example.toml` with comments; verify the rotator starts with it unmodified

## 5. Pi-hole dashboard (`pihole-tui`)

- [ ] 5.1 Implement app config (API base URL defaulting to local Pi-hole, app password, refresh interval defaulting to 5 s, state dir) with a warning logged when the file is readable by group/others; verify with tests for parsing and the permission warning
- [ ] 5.2 Implement the Pi-hole v6 API client: auth with password to obtain a session, automatic re-auth on expired session, and fetching summary, history, top blocked domains, FTL status, gravity info, and version; verify with mock-HTTP-server tests for each endpoint, expired session, and wrong password
- [ ] 5.3 Implement the polling model holding last-known values, last-success time, and connection state (waiting, ok, stale, auth error); verify with unit tests for each transition, including startup with API down and recovery
- [ ] 5.4 Implement the 60x20 layout from the design mockup (history graph, today pane with block meter, top blocked list with truncation, status line with FTL down in alert color, stale marker with age, waiting and auth-error screens) using the UI kit; verify with `TestBackend` snapshot tests for healthy, FTL down, stale, waiting, auth error, and long-domain cases
- [ ] 5.5 Implement urgent signal handling: raise and renew while FTL is down or the API is unreachable for more than 30 s, clear on recovery; verify with tests using an injected clock and a temp state dir
- [ ] 5.6 Add an example config at `deploy/pihole-tui.example.toml` and document setup (creating a Pi-hole app password, file permissions, running standalone for development) in `apps/pihole-tui/README.md`; verify by running the app on a laptop against the mock server in a 60x20 terminal following the README

## 6. Device deployment

- [ ] 6.1 Add `deploy/tmux.conf` (status bar off, low escape-time, no visual noise) and a per-window respawn wrapper that relaunches an app after it exits; verify locally that killing the app in a window brings it back within a few seconds
- [ ] 6.2 Add systemd units: `pihome-session.service` (tmux session on `tty1`, conflicts with `getty@tty1`, one window per configured app) and `pihome-rotator.service` (restart on failure); verify with `systemd-analyze verify` on both units
- [ ] 6.3 Add `deploy/install.sh` to copy binaries, configs (mode 0600 for secrets), tmux.conf, and units, then enable the services, with a matching uninstall/rollback path that re-enables `getty@tty1`; verify by running it twice on the Pi (idempotent) and then rolling back
- [ ] 6.4 Document deployment (Pi-hole v6 prerequisite, cross-build, install, rollback, attaching over SSH, console font used) in the root `README.md`; verify a fresh deploy can be done by following it

## 7. On-device integration checks

- [ ] 7.1 Reboot the Pi and verify it boots straight to the Pi-hole dashboard fullscreen on the 3.5" display with no login prompt and readable layout
- [ ] 7.2 Stop `pihole-FTL` on the Pi and verify the dashboard shows FTL down and holds the screen (header shows the hold marker); start FTL again and verify it recovers and rotation resumes
- [ ] 7.3 Stop the rotator service and verify the dashboard keeps updating with the indicator hidden and that systemd restarts the rotator
- [ ] 7.4 Attach to the session over SSH and verify the remote view matches the physical display while rotation continues

## Workflow follow-up

- Tune dwell time, refresh intervals, and console font on the device; record final values in the example configs.
- Archive the change after review with `/opsx:archive`.
