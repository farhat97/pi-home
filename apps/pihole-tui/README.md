# pihole-tui

Fullscreen Pi-hole v6 dashboard for the pi-home 3.5" display (60x20
terminal). It shows queries per minute over the last 24 h, today's totals and
block rate, the top blocked domains, and service status. It reads the Pi-hole
v6 REST API; the v5 `api.php` is not supported.

```
┌ pi-hole ──────────────────────────────────── [* o o] 42s ┐
│ queries/min · 24h  █ permitted █ blocked                 │
│160│                         ▄▄▄██████▄▄                  │
│   │                    ▄███████████████████▄▄            │
│  0│██████████████████████████████████████████████████████│
├ today ─────────────────┬ top blocked ────────────────────┤
│ queries         48,213 │ doubleclick.net           2,104 │
│ blocked         11,902 │ app-measurement.com       1,388 │
│ ████░░░░░░░░░░░░ 24.7% │ graph.facebook.com          907 │
│ clients             17 │ ads.roku.com                611 │
├ status ────────────────┴─────────────────────────────────┤
│ FTL ok  blocking on  gravity 1.2M (3h ago)  v6.1         │
└──────────────────────────────────────────────────────────┘
```

## What it shows when things go wrong

| Situation | Screen |
|---|---|
| API never answered since start | "waiting for Pi-hole API", retrying every `refresh_s` |
| Wrong password | "authentication failed" with Pi-hole's message, retrying |
| API stops answering | last values stay up; status pane shows `stale · updated 1m ago` and `FTL down` in red |
| Unreachable for more than 30 s | also raises an urgent signal so the rotator holds this screen (header shows `hold`) |
| Blocking disabled or failed | `blocking off` / `blocking failed` in red; no signal |
| API answers with an error (HTTP 500, bad JSON) | stale marker only; FTL is up |

In Pi-hole v6 the API is served by `pihole-FTL` itself, so "API unreachable"
is how the dashboard knows FTL is down.

## Setup on the Pi

1. **Create an app password.** In the Pi-hole web interface go to
   *Settings > Web interface / API*, switch to *Expert* mode, and use
   *Configure app password*. Copy the password shown (it's displayed once),
   then click *Enable new app password*. An app password can be revoked
   without changing your admin password. Note that creating one logs out
   existing sessions.
2. **Write the config.** `deploy/install.sh` installs
   `deploy/pihole-tui.example.toml` as `/etc/pihome/pihole-tui.toml`. Put the
   app password in `password`.
3. **Keep it private.** The file must be readable only by the service user:

   ```sh
   sudo chown pihome:pihome /etc/pihome/pihole-tui.toml
   sudo chmod 600 /etc/pihome/pihole-tui.toml
   ```

   If other users can read it, the app logs a warning naming the file.

Logs go to `<state_dir>/pihole-tui.log` (default `/run/pihome/pihole-tui.log`),
since the terminal belongs to the UI. Set `PIHOLE_TUI_LOG` to put them
elsewhere and `RUST_LOG=debug` for more detail.

## Running standalone for development

No Pi needed: a mock Pi-hole API ships as a cargo example.

```sh
# Terminal 1: fake Pi-hole API on http://127.0.0.1:8053, password "dev"
cargo run -p pihole-tui --example mock_pihole

# Terminal 2: a dev config, then the dashboard
cat > /tmp/pihole-tui.toml <<'TOML'
api_url = "http://127.0.0.1:8053"
password = "dev"
state_dir = "/tmp/pihome"
TOML
chmod 600 /tmp/pihole-tui.toml
cargo run -p pihole-tui -- --config /tmp/pihole-tui.toml
```

Size terminal 2 to 60x20 to see the device layout (`printf '\e[8;20;60t'`
works in most terminals; or `tmux new -x 60 -y 20`). Anything smaller shows
"terminal too small". Press `q` to quit.

Things to try:

- **Outage:** stop the mock with Ctrl-C. The status pane turns stale and shows
  `FTL down`; after 30 s, `/tmp/pihome/signals/pihole-tui.json` appears.
  Restart the mock and both clear.
- **Wrong password:** set `password = "nope"`.
- **Console colors:** `PIHOME_COLOR=16` forces the 16-color, block-character
  theme used on the Linux console; `PIHOME_COLOR=truecolor` forces braille.
- **Rotation indicator:** with no rotator running, the header shows only the
  app name. Write a state file to see it:

  ```sh
  printf '{"active":0,"count":3,"next_in_s":42,"pinned":false,"updated_ms":%s}' \
    "$(date +%s%3N)" > /tmp/pihome/state.json
  ```

  It disappears again 5 s later, as it would if the rotator stopped.

## Tests

```sh
cargo test -p pihole-tui
```

The view tests are `insta` snapshots of the 60x20 screen. After an
intentional layout change, review them with `cargo insta review` (or rerun
with `INSTA_UPDATE=always` and check the diff).
