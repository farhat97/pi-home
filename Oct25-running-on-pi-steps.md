 ▐▛███▛█   Claude Code v2.1.310
▝▜██████▀  Opus 5.5 · Claude Team
 ▝▝   ▝▝   ~/Documents/personal-and-poc/pi-home


❯ I am already running pi-hole on a raspberry pi with a static IP. How should I 
  go about running this first terminal screen on the pi?                        

  Ran 2 shell commands (ctrl+o to expand)

● The repo already has a deploy path for this: build on your laptop, copy to the
  Pi, and run install.sh. Since your Pi-hole is already running, the steps
  below are adapted to that. I've added a manual test before the install. The
  installer takes over tty1 and turns off the login prompt there, so it's worth
  seeing the dashboard work first.

  0. Check the Pi (over SSH)

  uname -m          # must print aarch64. armv7l means 32-bit OS, and the 
  aarch64 build won't run
  pihole -v         # must be v6.x. The v5 API isn't supported
  sudo ss -tlnp | grep pihole-FTL   # which port the web/API is on (usually 80)
  sudo apt install tmux

  If FTL listens on a port other than 80, you'll need api_url = 
  "http://127.0.0.1:<port>" later. The default http://127.0.0.1 works as-is,
  because the dashboard runs on the same Pi as Pi-hole.

  1. Build on your laptop

  Docker needs to be running for this:

  cargo install cross --locked
  cross build --release --target aarch64-unknown-linux-gnu --workspace

  2. Copy to the Pi (use your static IP)

  PI=pi@192.168.x.y
  rel=target/aarch64-unknown-linux-gnu/release
  ssh $PI mkdir -p pihome
  scp -r deploy/* $rel/pihome-rotator $rel/pihole-tui $PI:pihome/

  3. Create a Pi-hole app password

  In the Pi-hole web UI: Settings → Web interface / API, switch to Expert, then
  Configure app password. Copy it, because it's only shown once, then enable it.
  This logs out your existing web sessions.

  4. Optional: test manually before installing

  On the Pi, over SSH:

  cd ~/pihome
  cat > /tmp/pihole-tui.toml <<'TOML'
  api_url = "http://127.0.0.1"
  password = "PASTE-APP-PASSWORD"
  state_dir = "/tmp/pihome"
  TOML
  chmod 600 /tmp/pihole-tui.toml
  tmux new -x 60 -y 20 './pihole-tui --config /tmp/pihole-tui.toml'

  You should see real numbers. If you see "authentication failed", the password
  is wrong. If it stays on "waiting for Pi-hole API", check api_url and the
  port. Press q to quit.

  5. Install for real

  cd ~/pihome
  sudo ./install.sh
  sudo nano /etc/pihome/pihole-tui.toml     # set password = "..." (and api_url 
  if needed)
  sudo systemctl restart pihome-session

  install.sh creates a pihome user, installs the binaries, configs and systemd
  units, disables getty@tty1, and starts the session. The dashboard then comes
  back on every boot.

  6. Display prerequisites

  The 3.5" panel has to be working as the console on tty1: the text login prompt
  should appear on it before you install. Set the Pi to boot to console, not
  desktop (raspi-config → System Options → Boot → Console). For the intended
  60x20 layout, set the console font in /etc/default/console-setup:

  FONTFACE="Terminus"
  FONTSIZE="8x16"

  Then run sudo setupcon. Anything smaller than 60x20 shows "terminal too
  small".

  SPI panel (ILI9486 / tft35a overlay): put the console on the LCD
  The panel is /dev/fb1 (fb_ili9486), but the console defaults to fb0. Symptom:
  the screen stays on the boot splash (last line "rc-local.service") even
  though pihome-session and pihole-tui are running. Fix by editing the kernel
  command line (one single line, no newlines):

  sudo cp /boot/firmware/cmdline.txt /boot/firmware/cmdline.txt.bak
  sudo sed -i 's/ quiet splash plymouth.ignore-serial-consoles//; s/$/ fbcon=map:10 fbcon=font:VGA8x8/' /boot/firmware/cmdline.txt
  cat /boot/firmware/cmdline.txt
  sudo reboot

  - fbcon=map:10 maps the console to fb1 (the LCD).
  - Removing quiet/splash/plymouth.ignore-serial-consoles stops the Plymouth
    splash from covering the screen.
  - fbcon=font:VGA8x8 gives 60x40 on 480x320. For exactly 60x20 use the
    Terminus 8x16 console-setup above instead.
  - Persists across reboots and apt upgrades. Re-check after a fresh OS image
    or a rewrite of cmdline.txt. Undo with the .bak file.

  Checking on it later

  sudo -u pihome tmux attach -r -t pihome   # see what the screen shows (Ctrl-b 
  d to detach)
  cat /run/pihome/pihole-tui.log
  systemctl status pihome-session pihome-rotator

  To undo everything and get the tty1 login back, run sudo ./install.sh 
  uninstall.

✻ Crunched for 22s · done 1:29 PM