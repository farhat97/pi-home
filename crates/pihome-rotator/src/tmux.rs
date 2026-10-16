//! Switching tmux windows.

use std::process::Command;

use anyhow::{Context, bail};

/// Shows a window on the display.
pub trait Switcher {
    fn select(&mut self, window: &str) -> anyhow::Result<()>;
}

/// `tmux select-window` on one session.
#[derive(Debug, Clone)]
pub struct Tmux {
    session: String,
    /// `tmux -L <socket>`; `None` uses the default server.
    socket: Option<String>,
}

impl Tmux {
    pub fn new(session: impl Into<String>) -> Self {
        Self {
            session: session.into(),
            socket: None,
        }
    }

    pub fn with_socket(mut self, socket: impl Into<String>) -> Self {
        self.socket = Some(socket.into());
        self
    }

    fn command(&self) -> Command {
        let mut cmd = Command::new("tmux");
        if let Some(s) = &self.socket {
            cmd.args(["-L", s]);
        }
        cmd
    }
}

/// An exact `session:window` target. tmux otherwise falls back to prefix
/// matching names, so `pihome:pihole` could select `pihole-tui`. Numeric
/// windows are indexes and stay as they are.
fn target(session: &str, window: &str) -> String {
    if !window.is_empty() && window.bytes().all(|b| b.is_ascii_digit()) {
        format!("={session}:{window}")
    } else {
        format!("={session}:={window}")
    }
}

impl Switcher for Tmux {
    fn select(&mut self, window: &str) -> anyhow::Result<()> {
        let target = target(&self.session, window);
        let out = self
            .command()
            .args(["select-window", "-t", &target])
            .output()
            .context("running tmux")?;
        if !out.status.success() {
            bail!(
                "tmux select-window -t {target}: {}",
                String::from_utf8_lossy(&out.stderr).trim()
            );
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn targets_are_exact() {
        assert_eq!(target("pihome", "pihole-tui"), "=pihome:=pihole-tui");
        assert_eq!(target("pihome", "0"), "=pihome:0");
    }
}
