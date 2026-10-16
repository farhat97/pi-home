//! Raises, renews and clears this app's urgent signal for the rotator.

use std::io;
use std::time::{Duration, SystemTime};

use pihome_ui::contract::{Signal, StateDir, unix_ms};

/// Signal lifetime; renewed well before it runs out.
pub const SIGNAL_TTL_S: u64 = 60;
const RENEW_EVERY: Duration = Duration::from_secs(10);

pub struct Alerter {
    dir: StateDir,
    name: String,
    current: Option<Signal>,
}

impl Alerter {
    pub fn new(dir: StateDir, name: impl Into<String>) -> Self {
        Self {
            dir,
            name: name.into(),
            current: None,
        }
    }

    pub fn is_raised(&self) -> bool {
        self.current.is_some()
    }

    /// Call on every tick with the current reason to hold the screen, if any.
    pub fn update(&mut self, reason: Option<&str>, now: SystemTime) -> io::Result<()> {
        match (reason, &mut self.current) {
            (Some(reason), None) => {
                let s = Signal::urgent(reason, SIGNAL_TTL_S, now);
                self.dir.write_signal(&self.name, &s)?;
                tracing::warn!(reason, "raised urgent signal");
                self.current = Some(s);
            }
            (Some(reason), Some(s)) => {
                let due =
                    unix_ms(now).saturating_sub(s.renewed_ms) >= RENEW_EVERY.as_millis() as u64;
                if due || s.reason != reason {
                    let mut next = s.clone();
                    next.renewed_ms = unix_ms(now);
                    next.reason = reason.to_owned();
                    self.dir.write_signal(&self.name, &next)?;
                    *s = next;
                }
            }
            (None, Some(_)) => {
                self.dir.clear_signal(&self.name)?;
                tracing::info!("cleared urgent signal");
                self.current = None;
            }
            (None, None) => {}
        }
        Ok(())
    }

    /// Removes the signal, e.g. on exit.
    pub fn clear(&mut self) -> io::Result<()> {
        self.current = None;
        self.dir.clear_signal(&self.name)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::api::ApiError;
    use crate::model::Model;
    use pihome_ui::contract::from_unix_ms;
    use std::time::Instant;

    const WALL0: u64 = 1_700_000_000_000;

    /// Moves the model's monotonic clock and the wall clock together.
    struct Harness {
        t0: Instant,
        model: Model,
        alerter: Alerter,
        dir: StateDir,
        _tmp: tempfile::TempDir,
    }

    impl Harness {
        fn new() -> Self {
            let tmp = tempfile::tempdir().unwrap();
            let dir = StateDir::new(tmp.path());
            Self {
                t0: Instant::now(),
                model: Model::default(),
                alerter: Alerter::new(dir.clone(), "pihole-tui"),
                dir,
                _tmp: tmp,
            }
        }

        fn poll(&mut self, s: u64, ok: bool) -> Option<Signal> {
            let now = self.t0 + Duration::from_secs(s);
            let result = if ok {
                Ok(Default::default())
            } else {
                Err(ApiError::Unreachable("refused".into()))
            };
            self.model.apply(result, now);
            let reason = self.model.alert(now);
            self.alerter
                .update(reason.as_deref(), from_unix_ms(WALL0 + s * 1000))
                .unwrap();
            self.dir.read_signal("pihole-tui")
        }
    }

    #[test]
    fn raises_after_thirty_seconds_renews_and_clears() {
        let mut h = Harness::new();
        assert!(h.poll(0, true).is_none());
        for s in (5..=35).step_by(5) {
            assert!(h.poll(s, false).is_none(), "raised early at {s}s");
        }
        let raised = h.poll(40, false).expect("signal after 30 s unreachable");
        assert!(raised.reason.contains("FTL down"));
        assert_eq!(raised.raised_ms, WALL0 + 40_000);
        assert!(!raised.is_expired(from_unix_ms(WALL0 + 100_000)));

        // Still down a long time later: same raise time, renewed recently.
        let mut last = raised.clone();
        for s in (45..=300).step_by(5) {
            last = h.poll(s, false).unwrap();
        }
        assert_eq!(last.raised_ms, raised.raised_ms);
        assert!(last.renewed_ms >= WALL0 + 290_000);
        assert!(!last.is_expired(from_unix_ms(WALL0 + 300_000)));

        assert!(h.poll(305, true).is_none());
        assert!(!h.alerter.is_raised());
    }

    #[test]
    fn unreachable_at_startup_also_raises() {
        let mut h = Harness::new();
        for s in (0..=30).step_by(5) {
            assert!(h.poll(s, false).is_none());
        }
        assert!(h.poll(35, false).is_some());
    }

    #[test]
    fn brief_outage_never_raises() {
        let mut h = Harness::new();
        h.poll(0, true);
        h.poll(5, false);
        h.poll(25, false);
        assert!(h.poll(30, true).is_none());
        assert!(h.poll(70, false).is_none());
    }

    #[test]
    fn clear_removes_file() {
        let mut h = Harness::new();
        for s in (0..=40).step_by(5) {
            h.poll(s, false);
        }
        assert!(h.dir.read_signal("pihole-tui").is_some());
        h.alerter.clear().unwrap();
        assert!(h.dir.read_signal("pihole-tui").is_none());
    }
}
