# pi-home

Raspberry Pi home gadget: btop-style terminal dashboards on the Pi's 3.5"
GPIO display, one fullscreen app at a time, rotating on a schedule. Planned
and tracked with [OpenSpec](openspec/).

```
systemd
 ├─ pihome-session.service   tmux session "pihome" on tty1, status bar off
 │    └─ window pihole-tui    (each app in a respawn loop)
 └─ pihome-rotator.service   tmux select-window on a schedule,
                             writes /run/pihome/state.json, reads signals/
```

| Path | What |
|---|---|
| `crates/pihome-ui` | Shared theme, panes, meters, graphs, header chrome, size guard, rotator file contract. See its [README](crates/pihome-ui/README.md). |
| `crates/pihome-rotator` | Switches tmux windows, publishes rotation state, honours urgent signals. |
| `apps/pihole-tui` | Pi-hole v6 dashboard. See its [README](apps/pihole-tui/README.md). |
| `deploy/` | systemd units, `tmux.conf`, respawn wrapper, example configs, `install.sh`. |

## Prerequisites

- Raspberry Pi 4B with **64-bit** Raspberry Pi OS (Lite, or set to boot to
  console: `sudo raspi-config` → *System Options* → *Boot* → *Console*).
- The 3.5" display working as the **console** on tty1 (the text login prompt
  shows up on it). Driver setup depends on the panel and isn't covered here.
- **Pi-hole v6**, installed with its official installer
  (`curl -sSL https://install.pi-hole.net | bash`). The v5 API is not supported.
- `tmux` on the Pi: `sudo apt install tmux`.
- On the build machine: Rust (via rustup) and, for cross-compiling,
  [`cross`](https://github.com/cross-rs/cross) plus Docker or Podman.

### Console font

The layouts target a **60x20** grid: 480x320 px with an 8x16 font. Terminus
8x16 is the font used. Set it in `/etc/default/console-setup`:

```sh
FONTFACE="Terminus"
FONTSIZE="8x16"
```

then `sudo setupcon` (or reboot). With a 6x12 font the grid is 80x26, which
also works but is hard to read at arm's length. Below 60x20 every app shows
"terminal too small" instead of its layout.

## Build

Cross-compile on a laptop (needs Docker running; `Cross.toml` sets the target):

```sh
cargo install cross --locked
cross build --release --target aarch64-unknown-linux-gnu --workspace
ls target/aarch64-unknown-linux-gnu/release/{pihome-rotator,pihole-tui}
```

Or build on the Pi itself (slower): install Rust there and run
`cargo build --release --workspace`; the binaries land in `target/release/`.

Tests run anywhere: `cargo test --workspace`. The tmux integration test needs
`tmux` installed.

## Install on the Pi

Copy the deploy directory and the two binaries to the Pi, side by side:

```sh
rel=target/aarch64-unknown-linux-gnu/release
ssh pi@pi.local mkdir -p pihome
scp -r deploy/* $rel/pihome-rotator $rel/pihole-tui pi@pi.local:pihome/
```

Then on the Pi:

```sh
cd ~/pihome
sudo ./install.sh
```

The script, which is safe to rerun for upgrades:

- creates a `pihome` system user,
- installs the binaries to `/usr/local/bin` and the helper scripts to
  `/usr/local/lib/pihome`,
- installs configs to `/etc/pihome/` (`rotator.toml`, `apps`,
  `pihole-tui.toml` with mode 0600, `tmux.conf`); existing configs are kept,
- installs and enables `pihome-session.service` and `pihome-rotator.service`,
  disables `getty@tty1` (the login prompt), and (re)starts both services.

Finally, give the dashboard its Pi-hole password:

1. Create an app password in the Pi-hole web UI (*Settings → Web interface /
   API*, Expert mode, *Configure app password*). It's shown once.
2. `sudo nano /etc/pihome/pihole-tui.toml` and set `password = "..."`.
   The file stays `0600`, owned by `pihome`.
3. The app re-reads its config when it restarts: `sudo systemctl restart pihome-session`.

The display should now show the Pi-hole dashboard, and it comes back on every
boot with no login prompt.

### Configuration

| File | Purpose |
|---|---|
| `/etc/pihome/apps` | One line per tmux window: `NAME COMMAND...` |
| `/etc/pihome/rotator.toml` | Rotation order, dwell time per app (default 60 s), session name, state dir |
| `/etc/pihome/pihole-tui.toml` | API URL, password, refresh intervals |
| `/etc/pihome/tmux.conf` | Display session look; overwritten on reinstall |

After editing `apps` or `tmux.conf`: `sudo systemctl restart pihome-session`.
After editing `rotator.toml`: `sudo systemctl restart pihome-rotator`.

## Watching it over SSH

The display is a tmux session owned by the `pihome` user. Attach read-only to
see exactly what the screen shows, while rotation carries on:

```sh
ssh pi@pi.local
sudo -u pihome tmux attach -r -t pihome
```

Detach with `Ctrl-b d`. Windows are sized to the smallest attached client,
so a bigger SSH terminal won't change the physical layout. A terminal smaller
than 60x20 *will* shrink it, and the apps show "terminal too small" until you
detach.

Useful checks:

```sh
cat /run/pihome/state.json                 # what the rotator is doing
ls /run/pihome/signals/                    # apps asking for the screen
journalctl -u pihome-rotator -f            # rotator log
cat /run/pihome/pihole-tui.log             # dashboard log
systemctl status pihome-session pihome-rotator
```

## Roll back

```sh
cd ~/pihome
sudo ./install.sh uninstall          # stop and remove services and binaries,
                                     # re-enable and start the tty1 login
sudo ./install.sh uninstall --purge  # also delete /etc/pihome and the pihome user
```

Without `--purge` the configs (including the app password) stay in
`/etc/pihome` for a later reinstall. To revoke the app password itself,
create a new one in the Pi-hole web UI.
