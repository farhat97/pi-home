#!/bin/sh
# Installs (or removes) pi-home on the Raspberry Pi. Run as root on the Pi,
# from a copy of the deploy/ directory with the binaries next to it.
#
#   sudo ./install.sh                     install or upgrade (safe to rerun)
#   sudo ./install.sh uninstall           roll back: stop, remove, restore tty1 login
#   sudo ./install.sh uninstall --purge   also delete /etc/pihome and the pihome user
#
# Binaries are taken from $BIN_DIR, default: the directory containing this
# script, then ../target/aarch64-unknown-linux-gnu/release.
set -eu

here=$(cd "$(dirname "$0")" && pwd)
user=pihome
bin=/usr/local/bin
lib=/usr/local/lib/pihome
conf=/etc/pihome
units=/etc/systemd/system
services="pihome-session.service pihome-rotator.service"

say() { printf 'pihome: %s\n' "$*"; }
die() { printf 'pihome: error: %s\n' "$*" >&2; exit 1; }

[ "$(id -u)" -eq 0 ] || die "run as root (sudo $0 $*)"

find_bin() {
    for d in ${BIN_DIR:-} "$here" "$here/../target/aarch64-unknown-linux-gnu/release"; do
        [ -x "$d/$1" ] && { echo "$d/$1"; return; }
    done
    die "binary $1 not found; build it (see README) or set BIN_DIR"
}

# Copies a config into place only if it isn't there yet, so reruns keep edits.
install_config() { # SRC DEST MODE OWNER
    if [ -e "$2" ]; then
        say "keeping existing $2"
    else
        install -m "$3" -o "${4%:*}" -g "${4#*:}" "$1" "$2"
        say "installed $2"
    fi
}

do_install() {
    command -v tmux >/dev/null || die "tmux is not installed (sudo apt install tmux)"
    systemctl list-unit-files pihole-FTL.service >/dev/null 2>&1 \
        || say "warning: pihole-FTL.service not found; the dashboard needs Pi-hole v6"

    rotator=$(find_bin pihome-rotator)
    tui=$(find_bin pihole-tui)

    if ! id "$user" >/dev/null 2>&1; then
        useradd --system --home-dir /var/lib/pihome --create-home \
            --shell /usr/sbin/nologin "$user"
        say "created user $user"
    fi

    install -d -m 0755 "$lib"
    install -m 0755 "$rotator" "$tui" "$bin/"
    install -m 0755 "$here/run-app" "$here/start-session" "$lib/"

    install -d -m 0750 -o root -g "$user" "$conf"
    install -m 0644 "$here/tmux.conf" "$conf/tmux.conf"
    install_config "$here/apps.example" "$conf/apps" 0644 root:root
    install_config "$here/rotator.example.toml" "$conf/rotator.toml" 0644 root:root
    install_config "$here/pihole-tui.example.toml" "$conf/pihole-tui.toml" 0600 "$user:$user"
    # Holds the Pi-hole password: always owner-only, even if it was loosened.
    chown "$user:$user" "$conf/pihole-tui.toml"
    chmod 0600 "$conf/pihole-tui.toml"

    for s in $services; do
        install -m 0644 "$here/systemd/$s" "$units/$s"
    done
    systemctl daemon-reload

    # The session owns tty1 from now on.
    systemctl disable getty@tty1.service >/dev/null 2>&1 || true
    systemctl enable $services
    # Restart so upgraded binaries and units take effect.
    systemctl restart $services
    say "installed; the display should show the dashboard now"
    if ! grep -Eq '^[[:space:]]*password[[:space:]]*=[[:space:]]*"[^"]+"' "$conf/pihole-tui.toml"; then
        say "next: put your Pi-hole app password in $conf/pihole-tui.toml"
    fi
}

do_uninstall() {
    purge=${1:-}
    for s in $services; do
        systemctl disable --now "$s" >/dev/null 2>&1 || true
        rm -f "$units/$s"
    done
    systemctl daemon-reload
    systemctl enable getty@tty1.service
    systemctl start getty@tty1.service
    rm -f "$bin/pihome-rotator" "$bin/pihole-tui"
    rm -rf "$lib" /run/pihome
    if [ "$purge" = "--purge" ]; then
        rm -rf "$conf"
        if id "$user" >/dev/null 2>&1; then
            userdel --remove "$user" 2>/dev/null || userdel "$user"
        fi
        say "removed configs and user $user"
    else
        say "kept $conf and user $user (use --purge to remove)"
    fi
    say "uninstalled; tty1 login restored"
}

case "${1:-install}" in
    install) do_install ;;
    uninstall) do_uninstall "${2:-}" ;;
    *) die "usage: $0 [install | uninstall [--purge]]" ;;
esac
