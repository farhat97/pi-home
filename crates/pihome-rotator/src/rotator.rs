//! One rotator step: read signals, advance the schedule, switch, publish.

use std::time::{Duration, SystemTime};

use pihome_ui::contract::{RotationState, StateDir};

use crate::config::Config;
use crate::schedule::Schedule;
use crate::tmux::Switcher;

pub struct Rotator<S> {
    schedule: Schedule,
    windows: Vec<String>,
    switcher: S,
    dir: StateDir,
    /// Window index last selected successfully; retried until it matches.
    shown: Option<usize>,
    switch_error: Option<String>,
    write_error: Option<String>,
}

impl<S: Switcher> Rotator<S> {
    pub fn new(cfg: &Config, switcher: S) -> Self {
        Self {
            schedule: Schedule::from_config(cfg),
            windows: cfg.apps.iter().map(|a| a.window().to_owned()).collect(),
            switcher,
            dir: StateDir::new(&cfg.state_dir),
            shown: None,
            switch_error: None,
            write_error: None,
        }
    }

    pub fn switcher(&self) -> &S {
        &self.switcher
    }

    pub fn step(&mut self, mono: Duration, wall: SystemTime) -> RotationState {
        let signals = self.dir.read_signals();
        let state = self.schedule.tick(mono, wall, &signals);

        if self.shown != Some(state.active) {
            match self.switcher.select(&self.windows[state.active]) {
                Ok(()) => {
                    tracing::info!(
                        window = %self.windows[state.active],
                        pinned = state.pinned,
                        "switched"
                    );
                    self.shown = Some(state.active);
                    self.switch_error = None;
                }
                Err(e) => report(&mut self.switch_error, e),
            }
        }
        match self.dir.write_state(&state) {
            Ok(()) => self.write_error = None,
            Err(e) => report(
                &mut self.write_error,
                anyhow::Error::new(e).context("writing state"),
            ),
        }
        state
    }
}

/// Logs an error once, not on every tick while it persists.
fn report(last: &mut Option<String>, e: anyhow::Error) {
    let msg = format!("{e:#}");
    if last.as_ref() != Some(&msg) {
        tracing::warn!("{msg}");
        *last = Some(msg);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use pihome_ui::contract::{Signal, from_unix_ms};

    #[derive(Default)]
    struct Fake {
        calls: Vec<String>,
        fail: bool,
    }

    impl Switcher for Fake {
        fn select(&mut self, window: &str) -> anyhow::Result<()> {
            self.calls.push(window.to_owned());
            if self.fail {
                anyhow::bail!("no session");
            }
            Ok(())
        }
    }

    const WALL0: u64 = 1_700_000_000_000;

    fn setup(apps: &str) -> (tempfile::TempDir, Config) {
        let tmp = tempfile::tempdir().unwrap();
        let cfg = Config::parse(&format!(
            "state_dir = {:?}\ndwell_s = 10\n{apps}",
            tmp.path()
        ))
        .unwrap();
        (tmp, cfg)
    }

    fn step<S: Switcher>(r: &mut Rotator<S>, secs: u64) -> RotationState {
        r.step(Duration::from_secs(secs), from_unix_ms(WALL0 + secs * 1000))
    }

    const TWO: &str =
        "[[apps]]\nname = \"pihole-tui\"\nwindow = \"pi\"\n[[apps]]\nname = \"cta-tui\"\n";

    #[test]
    fn drives_switcher_on_schedule_and_writes_state() {
        let (tmp, cfg) = setup(TWO);
        let mut r = Rotator::new(&cfg, Fake::default());
        for s in 0..=25 {
            step(&mut r, s);
        }
        assert_eq!(r.switcher().calls, ["pi", "cta-tui", "pi"]);
        let state = StateDir::new(tmp.path()).read_state().unwrap();
        assert_eq!((state.active, state.count, state.next_in_s), (0, 2, 5));
        assert_eq!(state.updated_ms, WALL0 + 25_000);
    }

    #[test]
    fn switches_to_signalling_app() {
        let (tmp, cfg) = setup(TWO);
        let dir = StateDir::new(tmp.path());
        let mut r = Rotator::new(&cfg, Fake::default());
        step(&mut r, 0);
        dir.write_signal(
            "cta-tui",
            &Signal::urgent("x", 60, from_unix_ms(WALL0 + 1000)),
        )
        .unwrap();
        let s = step(&mut r, 1);
        assert!(s.pinned);
        assert_eq!(r.switcher().calls, ["pi", "cta-tui"]);
        assert!(dir.read_state().unwrap().pinned);

        for s in 2..30 {
            step(&mut r, s);
        }
        assert_eq!(r.switcher().calls, ["pi", "cta-tui"]);

        dir.clear_signal("cta-tui").unwrap();
        assert!(!step(&mut r, 30).pinned);
        step(&mut r, 40);
        assert_eq!(r.switcher().calls, ["pi", "cta-tui", "pi"]);
    }

    #[test]
    fn retries_failed_switch_and_still_publishes() {
        let (tmp, cfg) = setup(TWO);
        let mut r = Rotator::new(
            &cfg,
            Fake {
                fail: true,
                ..Default::default()
            },
        );
        step(&mut r, 0);
        step(&mut r, 1);
        assert_eq!(r.switcher().calls, ["pi", "pi"]);
        assert!(StateDir::new(tmp.path()).read_state().is_some());
        r.switcher.fail = false;
        step(&mut r, 2);
        step(&mut r, 3);
        assert_eq!(r.switcher().calls.len(), 3);
    }
}
