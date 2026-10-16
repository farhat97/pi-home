//! Rotator configuration, loaded from TOML.

use std::path::{Path, PathBuf};
use std::time::Duration;

use anyhow::{Context, bail};
use serde::Deserialize;

pub const DEFAULT_CONFIG_PATH: &str = "/etc/pihome/rotator.toml";
pub const DEFAULT_DWELL_S: u64 = 60;

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Config {
    /// tmux session holding one window per app.
    #[serde(default = "default_session")]
    pub session: String,
    /// Directory for `state.json` and `signals/`.
    #[serde(default = "default_state_dir")]
    pub state_dir: PathBuf,
    /// Dwell time for apps that don't set their own.
    #[serde(default = "default_dwell")]
    pub dwell_s: u64,
    /// Apps in rotation order.
    pub apps: Vec<AppConfig>,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AppConfig {
    /// Also the signal file name: `signals/<name>.json`.
    pub name: String,
    /// tmux window target (name or index); defaults to `name`.
    pub window: Option<String>,
    pub dwell_s: Option<u64>,
}

fn default_session() -> String {
    "pihome".into()
}

fn default_state_dir() -> PathBuf {
    pihome_ui::contract::DEFAULT_STATE_DIR.into()
}

fn default_dwell() -> u64 {
    DEFAULT_DWELL_S
}

impl AppConfig {
    pub fn window(&self) -> &str {
        self.window.as_deref().unwrap_or(&self.name)
    }
}

impl Config {
    pub fn parse(text: &str) -> anyhow::Result<Self> {
        let cfg: Self = toml::from_str(text)?;
        cfg.validate()?;
        Ok(cfg)
    }

    pub fn load(path: &Path) -> anyhow::Result<Self> {
        let text =
            std::fs::read_to_string(path).with_context(|| format!("reading {}", path.display()))?;
        Self::parse(&text).with_context(|| format!("parsing {}", path.display()))
    }

    /// Effective dwell time for the app at `index`.
    pub fn dwell(&self, index: usize) -> Duration {
        Duration::from_secs(self.apps[index].dwell_s.unwrap_or(self.dwell_s))
    }

    fn validate(&self) -> anyhow::Result<()> {
        if self.apps.is_empty() {
            bail!("at least one [[apps]] entry is required");
        }
        for (i, app) in self.apps.iter().enumerate() {
            if app.name.is_empty() || app.name.contains(['/', '\\']) || app.name.starts_with('.') {
                bail!("apps[{i}]: invalid name {:?}", app.name);
            }
            if self.apps[..i].iter().any(|a| a.name == app.name) {
                bail!("apps[{i}]: duplicate name {:?}", app.name);
            }
            if self.dwell(i).is_zero() {
                bail!("apps[{i}]: dwell_s must be at least 1");
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults() {
        let cfg = Config::parse("[[apps]]\nname = \"pihole-tui\"\n").unwrap();
        assert_eq!(cfg.session, "pihome");
        assert_eq!(cfg.state_dir, PathBuf::from("/run/pihome"));
        assert_eq!(cfg.dwell(0), Duration::from_secs(60));
        assert_eq!(cfg.apps[0].window(), "pihole-tui");
    }

    #[test]
    fn full() {
        let cfg = Config::parse(
            r#"
            session = "test"
            state_dir = "/tmp/x"
            dwell_s = 30

            [[apps]]
            name = "pihole-tui"
            window = "0"

            [[apps]]
            name = "cta-tui"
            dwell_s = 90
            "#,
        )
        .unwrap();
        assert_eq!(cfg.session, "test");
        assert_eq!(cfg.state_dir, PathBuf::from("/tmp/x"));
        assert_eq!(cfg.apps[0].window(), "0");
        assert_eq!(cfg.dwell(0), Duration::from_secs(30));
        assert_eq!(cfg.dwell(1), Duration::from_secs(90));
    }

    #[test]
    fn rejects_bad_configs() {
        for bad in [
            "apps = []",
            "",
            "[[apps]]\nname = \"a\"\n[[apps]]\nname = \"a\"\n",
            "[[apps]]\nname = \"../x\"\n",
            "[[apps]]\nname = \"a\"\ndwell_s = 0\n",
            "[[apps]]\nname = \"a\"\nbogus = 1\n",
        ] {
            assert!(Config::parse(bad).is_err(), "accepted {bad:?}");
        }
    }
}
