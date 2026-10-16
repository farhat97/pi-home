use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::mpsc;
use std::time::{Duration, Instant, SystemTime};

use anyhow::{Context as _, bail};
use crossterm::event::{self, Event, KeyCode, KeyEventKind, KeyModifiers};
use pihole_tui::alert::Alerter;
use pihole_tui::api::{ApiError, Client};
use pihole_tui::config::{Config, DEFAULT_CONFIG_PATH, permission_warning};
use pihole_tui::model::{Model, Update};
use pihole_tui::view::{self, Context};
use pihome_ui::{Indicator, StateDir, Theme};
use tokio::sync::Notify;

/// Redraw often enough that the rotation countdown ticks smoothly.
const FRAME: Duration = Duration::from_millis(250);
const TOP_BLOCKED: usize = 10;
const VERSION_REFRESH: Duration = Duration::from_secs(3600);

fn main() -> anyhow::Result<()> {
    let config_path = config_path()?;
    let cfg = Config::load(&config_path)?;
    let dir = StateDir::new(&cfg.state_dir);
    init_logging(&dir);

    if let Some(w) = permission_warning(&config_path) {
        // Before the TUI takes the screen, so it's visible when run by hand.
        eprintln!("warning: {w}");
        tracing::warn!("{w}");
    }
    tracing::info!(config = %config_path.display(), api = %cfg.api_url, "starting");

    let (tx, rx) = mpsc::channel();
    let stop = Arc::new(Notify::new());
    let poller = {
        let (cfg, stop) = (cfg.clone(), stop.clone());
        std::thread::spawn(move || {
            tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
                .expect("tokio runtime")
                .block_on(poll_loop(cfg, tx, stop))
        })
    };

    let result = run(&cfg, &dir, &rx);

    stop.notify_one();
    let _ = poller.join();
    result
}

fn run(
    cfg: &Config,
    dir: &StateDir,
    rx: &mpsc::Receiver<Result<Update, ApiError>>,
) -> anyhow::Result<()> {
    let theme = Theme::detect();
    let mut model = Model::default();
    let mut alerter = Alerter::new(dir.clone(), &cfg.signal_name);
    let mut alert_error: Option<String> = None;

    let mut terminal = ratatui::init();
    let result = (|| -> anyhow::Result<()> {
        loop {
            let now = Instant::now();
            while let Ok(r) = rx.try_recv() {
                model.apply(r, now);
            }
            let wall = SystemTime::now();
            match alerter.update(model.alert(now).as_deref(), wall) {
                Ok(()) => alert_error = None,
                Err(e) => {
                    let msg = format!("writing signal: {e}");
                    if alert_error.as_ref() != Some(&msg) {
                        tracing::warn!("{msg}");
                        alert_error = Some(msg);
                    }
                }
            }

            let cx = Context {
                theme: &theme,
                indicator: Indicator::read(dir, wall),
                now,
                wall,
                api_url: &cfg.api_url,
                refresh: cfg.refresh(),
            };
            terminal.draw(|f| view::draw(f, &model, &cx))?;

            if event::poll(FRAME)?
                && let Event::Key(k) = event::read()?
                && k.kind == KeyEventKind::Press
                && (k.code == KeyCode::Char('q')
                    || (k.code == KeyCode::Char('c')
                        && k.modifiers.contains(KeyModifiers::CONTROL)))
            {
                return Ok(());
            }
        }
    })();
    ratatui::restore();
    if let Err(e) = alerter.clear() {
        tracing::warn!("clearing signal: {e}");
    }
    result
}

async fn poll_loop(cfg: Config, tx: mpsc::Sender<Result<Update, ApiError>>, stop: Arc<Notify>) {
    let mut client = Client::new(&cfg.api_url, cfg.password.clone());
    let mut history_due = Instant::now();
    let mut version_due = Instant::now();
    loop {
        let now = Instant::now();
        let (want_history, want_version) = (now >= history_due, now >= version_due);
        let result = poll(&mut client, want_history, want_version).await;
        match &result {
            Ok(u) => {
                if u.history.is_some() {
                    history_due = now + cfg.history_refresh();
                }
                if want_version {
                    version_due = now + VERSION_REFRESH;
                }
            }
            Err(e) => tracing::debug!("poll failed: {e}"),
        }
        if tx.send(result).is_err() {
            break;
        }
        tokio::select! {
            _ = tokio::time::sleep(cfg.refresh()) => {}
            _ = stop.notified() => break,
        }
    }
    client.logout().await;
}

async fn poll(client: &mut Client, history: bool, version: bool) -> Result<Update, ApiError> {
    Ok(Update {
        summary: Some(client.summary().await?),
        blocking: Some(client.blocking().await?),
        top_blocked: Some(client.top_blocked(TOP_BLOCKED).await?),
        history: if history {
            Some(client.history().await?)
        } else {
            None
        },
        version: if version {
            client.version().await?
        } else {
            None
        },
    })
}

/// Logs go to a file because the terminal belongs to the UI.
/// `PIHOLE_TUI_LOG` overrides the default `<state_dir>/pihole-tui.log`.
fn init_logging(dir: &StateDir) {
    let path = std::env::var_os("PIHOLE_TUI_LOG")
        .map(PathBuf::from)
        .unwrap_or_else(|| dir.base().join("pihole-tui.log"));
    let file = std::fs::create_dir_all(path.parent().unwrap_or(Path::new(".")))
        .and_then(|()| std::fs::File::create(&path));
    match file {
        Ok(file) => tracing_subscriber::fmt()
            .with_env_filter(
                tracing_subscriber::EnvFilter::try_from_default_env()
                    .unwrap_or_else(|_| "info".into()),
            )
            .with_ansi(false)
            .with_writer(std::sync::Mutex::new(file))
            .init(),
        Err(e) => eprintln!(
            "warning: logging disabled, cannot open {}: {e}",
            path.display()
        ),
    }
}

fn config_path() -> anyhow::Result<PathBuf> {
    let mut args = std::env::args().skip(1);
    let path = match (args.next().as_deref(), args.next()) {
        (None, _) => PathBuf::from(DEFAULT_CONFIG_PATH),
        (Some("--config" | "-c"), Some(p)) => p.into(),
        (Some("--help" | "-h"), _) => {
            println!("usage: pihole-tui [--config PATH]  (default {DEFAULT_CONFIG_PATH})");
            println!("keys: q or Ctrl-C quits");
            std::process::exit(0);
        }
        _ => bail!("usage: pihole-tui [--config PATH]"),
    };
    std::fs::metadata(&path).with_context(|| format!("config {}", path.display()))?;
    Ok(path)
}
