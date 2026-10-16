//! App configuration, loaded from TOML.

use std::path::{Path, PathBuf};
use std::time::Duration;

use anyhow::{Context, bail};
use serde::Deserialize;

pub const DEFAULT_CONFIG_PATH: &str = "/etc/pihome/pihole-tui.toml";

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Config {
    /// Pi-hole web server; the client appends `/api`.
    #[serde(default = "default_api_url")]
    pub api_url: String,
    /// Pi-hole app password (or admin password). Empty when the Pi-hole has
    /// no password set.
    #[serde(default)]
    pub password: String,
    /// Seconds between summary/status polls.
    #[serde(default = "default_refresh")]
    pub refresh_s: u64,
    /// Seconds between 24h history polls (Pi-hole buckets it by 10 minutes).
    #[serde(default = "default_history_refresh")]
    pub history_refresh_s: u64,
    /// Rotator contract directory.
    #[serde(default = "default_state_dir")]
    pub state_dir: PathBuf,
    /// Signal file name; must match this app's `name` in the rotator config.
    #[serde(default = "default_signal_name")]
    pub signal_name: String,
}

fn default_api_url() -> String {
    "http://127.0.0.1".into()
}

fn default_refresh() -> u64 {
    5
}

fn default_history_refresh() -> u64 {
    60
}

fn default_state_dir() -> PathBuf {
    pihome_ui::contract::DEFAULT_STATE_DIR.into()
}

fn default_signal_name() -> String {
    "pihole-tui".into()
}

impl Default for Config {
    fn default() -> Self {
        Self::parse("").expect("defaults are valid")
    }
}

impl Config {
    pub fn parse(text: &str) -> anyhow::Result<Self> {
        let cfg: Self = toml::from_str(text)?;
        if cfg.refresh_s == 0 || cfg.history_refresh_s == 0 {
            bail!("refresh_s and history_refresh_s must be at least 1");
        }
        if !cfg.api_url.starts_with("http://") && !cfg.api_url.starts_with("https://") {
            bail!("api_url must start with http:// or https://");
        }
        Ok(cfg)
    }

    pub fn load(path: &Path) -> anyhow::Result<Self> {
        let text =
            std::fs::read_to_string(path).with_context(|| format!("reading {}", path.display()))?;
        Self::parse(&text).with_context(|| format!("parsing {}", path.display()))
    }

    pub fn refresh(&self) -> Duration {
        Duration::from_secs(self.refresh_s)
    }

    pub fn history_refresh(&self) -> Duration {
        Duration::from_secs(self.history_refresh_s)
    }
}

/// A warning if `path` (which holds the password) is readable by group or
/// others. `None` when permissions are tight or the file can't be inspected.
pub fn permission_warning(path: &Path) -> Option<String> {
    use std::os::unix::fs::PermissionsExt;
    let mode = std::fs::metadata(path).ok()?.permissions().mode();
    (mode & 0o044 != 0).then(|| {
        format!(
            "{} is readable by other users (mode {:o}); it holds the Pi-hole password, run: chmod 600 {}",
            path.display(),
            mode & 0o777,
            path.display()
        )
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::os::unix::fs::PermissionsExt;

    #[test]
    fn defaults() {
        let cfg = Config::default();
        assert_eq!(cfg.api_url, "http://127.0.0.1");
        assert_eq!(cfg.password, "");
        assert_eq!(cfg.refresh(), Duration::from_secs(5));
        assert_eq!(cfg.history_refresh(), Duration::from_secs(60));
        assert_eq!(cfg.state_dir, PathBuf::from("/run/pihome"));
        assert_eq!(cfg.signal_name, "pihole-tui");
    }

    #[test]
    fn full() {
        let cfg = Config::parse(
            r#"
            api_url = "https://pi.hole:443"
            password = "s3cret"
            refresh_s = 2
            history_refresh_s = 120
            state_dir = "/tmp/pihome"
            signal_name = "pihole"
            "#,
        )
        .unwrap();
        assert_eq!(cfg.api_url, "https://pi.hole:443");
        assert_eq!(cfg.password, "s3cret");
        assert_eq!(cfg.refresh(), Duration::from_secs(2));
        assert_eq!(cfg.signal_name, "pihole");
    }

    #[test]
    fn rejects_bad_values() {
        for bad in [
            "refresh_s = 0",
            "api_url = \"pi.hole\"",
            "pasword = \"typo\"",
        ] {
            assert!(Config::parse(bad).is_err(), "accepted {bad:?}");
        }
    }

    #[test]
    fn warns_when_readable_by_others() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("pihole-tui.toml");
        std::fs::write(&path, "password = \"x\"").unwrap();

        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600)).unwrap();
        assert_eq!(permission_warning(&path), None);

        for mode in [0o644, 0o640, 0o604] {
            std::fs::set_permissions(&path, std::fs::Permissions::from_mode(mode)).unwrap();
            let w = permission_warning(&path).expect("warning");
            assert!(w.contains(&path.display().to_string()), "{w}");
        }
        assert_eq!(permission_warning(&dir.path().join("missing")), None);
    }
}
