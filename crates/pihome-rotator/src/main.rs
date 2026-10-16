use std::path::PathBuf;
use std::time::{Duration, Instant, SystemTime};

use anyhow::bail;
use pihome_rotator::config::{Config, DEFAULT_CONFIG_PATH};
use pihome_rotator::rotator::Rotator;
use pihome_rotator::tmux::Tmux;

/// Twice a second: state stays well inside the 1 s update requirement and
/// urgent signals are picked up within the 2 s budget.
const TICK: Duration = Duration::from_millis(500);

fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env().unwrap_or_else(|_| "info".into()),
        )
        .with_writer(std::io::stderr)
        .init();

    let path = config_path()?;
    let cfg = Config::load(&path)?;
    tracing::info!(
        config = %path.display(),
        session = %cfg.session,
        apps = cfg.apps.len(),
        "rotator starting"
    );

    let mut tmux = Tmux::new(&cfg.session);
    if let Ok(socket) = std::env::var("PIHOME_TMUX_SOCKET") {
        tmux = tmux.with_socket(socket);
    }
    let mut rotator = Rotator::new(&cfg, tmux);

    let start = Instant::now();
    let mut next = start;
    loop {
        rotator.step(start.elapsed(), SystemTime::now());
        next += TICK;
        let now = Instant::now();
        if next < now {
            next = now;
        }
        std::thread::sleep(next - now);
    }
}

fn config_path() -> anyhow::Result<PathBuf> {
    let mut args = std::env::args().skip(1);
    match (args.next().as_deref(), args.next()) {
        (None, _) => Ok(DEFAULT_CONFIG_PATH.into()),
        (Some("--config" | "-c"), Some(p)) => Ok(p.into()),
        (Some("--help" | "-h"), _) => {
            println!("usage: pihome-rotator [--config PATH]  (default {DEFAULT_CONFIG_PATH})");
            std::process::exit(0);
        }
        _ => bail!("usage: pihome-rotator [--config PATH]"),
    }
}
