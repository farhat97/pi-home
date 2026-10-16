//! The rotation state machine. Pure: time and signals are passed in.

use std::time::{Duration, SystemTime};

use pihome_ui::contract::{Level, RotationState, Signal, unix_ms};

use crate::config::Config;

/// Which app is on screen and for how long.
///
/// Dwell times run on a monotonic clock (`mono`, time since start) so a wall
/// clock step, e.g. NTP sync after boot on a Pi without an RTC, can't freeze
/// or skip the rotation. Signal expiry uses the wall clock (`wall`) because
/// signals are written by other processes.
#[derive(Debug)]
pub struct Schedule {
    names: Vec<String>,
    dwells: Vec<Duration>,
    active: usize,
    deadline: Option<Duration>,
    held: Option<usize>,
}

impl Schedule {
    /// `apps` in rotation order as `(signal name, dwell)`. Must not be empty.
    pub fn new(apps: Vec<(String, Duration)>) -> Self {
        assert!(!apps.is_empty(), "schedule needs at least one app");
        let (names, dwells) = apps.into_iter().unzip();
        Self {
            names,
            dwells,
            active: 0,
            deadline: None,
            held: None,
        }
    }

    pub fn from_config(cfg: &Config) -> Self {
        Self::new(
            cfg.apps
                .iter()
                .enumerate()
                .map(|(i, a)| (a.name.clone(), cfg.dwell(i)))
                .collect(),
        )
    }

    pub fn active(&self) -> usize {
        self.active
    }

    /// Advances the schedule to `mono`/`wall` given the current signals and
    /// returns the state to publish.
    pub fn tick(
        &mut self,
        mono: Duration,
        wall: SystemTime,
        signals: &[(String, Signal)],
    ) -> RotationState {
        // Live urgent signals from configured apps, earliest raised first.
        let mut live: Vec<(u64, usize)> = signals
            .iter()
            .filter(|(_, s)| s.level == Level::Urgent && !s.is_expired(wall))
            .filter_map(|(name, s)| Some((s.raised_ms, self.index_of(name)?)))
            .collect();
        live.sort();

        let was_held = self.held.is_some();
        if let Some(h) = self.held
            && !live.iter().any(|&(_, i)| i == h)
        {
            self.held = None;
        }
        if self.held.is_none() {
            if let Some(&(_, i)) = live.first() {
                self.held = Some(i);
                self.active = i;
            } else if was_held {
                // Released: the held app gets a full dwell before moving on.
                self.deadline = Some(mono + self.dwells[self.active]);
            }
        }

        let next_in_s = if self.held.is_some() {
            0
        } else {
            let deadline = *self.deadline.get_or_insert(mono + self.dwells[self.active]);
            let deadline = if mono >= deadline {
                if self.names.len() > 1 {
                    self.active = (self.active + 1) % self.names.len();
                }
                let d = mono + self.dwells[self.active];
                self.deadline = Some(d);
                d
            } else {
                deadline
            };
            (deadline - mono).as_secs_f64().ceil() as u64
        };

        RotationState {
            active: self.active,
            count: self.names.len(),
            next_in_s,
            pinned: self.held.is_some(),
            updated_ms: unix_ms(wall),
        }
    }

    fn index_of(&self, name: &str) -> Option<usize> {
        self.names.iter().position(|n| n == name)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use pihome_ui::contract::from_unix_ms;

    const WALL0: u64 = 1_700_000_000_000;

    /// Drives a schedule with a fake clock that moves both clocks together.
    struct Clock {
        sched: Schedule,
        ms: u64,
    }

    impl Clock {
        fn new(apps: &[(&str, u64)]) -> Self {
            let apps = apps
                .iter()
                .map(|&(n, s)| (n.to_owned(), Duration::from_secs(s)))
                .collect();
            Self {
                sched: Schedule::new(apps),
                ms: 0,
            }
        }

        fn wall(&self) -> SystemTime {
            from_unix_ms(WALL0 + self.ms)
        }

        fn at(&mut self, secs: f64, signals: &[(String, Signal)]) -> RotationState {
            self.ms = (secs * 1000.0) as u64;
            self.sched
                .tick(Duration::from_millis(self.ms), self.wall(), signals)
        }

        /// Ticks every 500 ms up to `secs`, returning the active app at each.
        fn run(&mut self, from: f64, to: f64, signals: &[(String, Signal)]) -> Vec<usize> {
            let mut out = Vec::new();
            let mut t = from;
            while t <= to {
                out.push(self.at(t, signals).active);
                t += 0.5;
            }
            out
        }

        fn signal(&self, app: &str, raised_s: f64, ttl_s: u64) -> (String, Signal) {
            let mut s = Signal::urgent("down", ttl_s, from_unix_ms(WALL0));
            s.raised_ms = WALL0 + (raised_s * 1000.0) as u64;
            s.renewed_ms = s.raised_ms;
            (app.to_owned(), s)
        }
    }

    fn three() -> Clock {
        Clock::new(&[("a", 60), ("b", 60), ("c", 60)])
    }

    #[test]
    fn cycles_in_order_and_wraps() {
        let mut c = three();
        let s = c.at(0.0, &[]);
        assert_eq!(
            (s.active, s.count, s.next_in_s, s.pinned),
            (0, 3, 60, false)
        );
        assert_eq!(c.at(59.5, &[]).active, 0);
        assert_eq!(c.at(59.5, &[]).next_in_s, 1);
        assert_eq!(c.at(60.0, &[]).active, 1);
        assert_eq!(c.at(60.0, &[]).next_in_s, 60);
        assert_eq!(c.at(120.0, &[]).active, 2);
        assert_eq!(c.at(180.0, &[]).active, 0);
        assert_eq!(c.at(240.0, &[]).active, 1);
    }

    #[test]
    fn countdown_matches_header_example() {
        let mut c = three();
        c.at(0.0, &[]);
        let s = c.at(18.0, &[]);
        assert_eq!((s.active, s.next_in_s), (0, 42));
    }

    #[test]
    fn per_app_dwell() {
        let mut c = Clock::new(&[("a", 10), ("b", 30)]);
        c.at(0.0, &[]);
        assert_eq!(c.at(10.0, &[]).active, 1);
        assert_eq!(c.at(39.5, &[]).active, 1);
        assert_eq!(c.at(40.0, &[]).active, 0);
    }

    #[test]
    fn single_app_never_switches() {
        let mut c = Clock::new(&[("only", 60)]);
        let actives = c.run(0.0, 300.0, &[]);
        assert!(actives.iter().all(|&a| a == 0));
        assert!(!c.at(300.5, &[]).pinned);
    }

    #[test]
    fn state_carries_wall_time() {
        let mut c = three();
        assert_eq!(c.at(2.0, &[]).updated_ms, WALL0 + 2000);
    }

    #[test]
    fn urgent_signal_takes_over_and_holds() {
        let mut c = three();
        c.at(0.0, &[]);
        let sig = [c.signal("c", 10.0, 600)];
        let s = c.at(10.0, &sig);
        assert_eq!((s.active, s.pinned, s.next_in_s), (2, true, 0));
        // Held well past any dwell time.
        assert!(c.run(10.5, 400.0, &sig).iter().all(|&a| a == 2));
    }

    #[test]
    fn cleared_signal_resumes_with_fresh_dwell_from_held_app() {
        let mut c = three();
        c.at(0.0, &[]);
        let sig = [c.signal("b", 5.0, 600)];
        c.run(5.0, 100.0, &sig);
        let s = c.at(100.5, &[]);
        assert_eq!((s.active, s.pinned, s.next_in_s), (1, false, 60));
        assert_eq!(c.at(160.0, &[]).active, 1);
        assert_eq!(c.at(160.5, &[]).active, 2);
    }

    #[test]
    fn expired_signal_counts_as_cleared() {
        let mut c = three();
        c.at(0.0, &[]);
        // Raised at 5 s, never renewed, 30 s TTL: live through 35 s.
        let sig = [c.signal("c", 5.0, 30)];
        assert!(c.at(35.0, &sig).pinned);
        let s = c.at(35.5, &sig);
        assert_eq!((s.active, s.pinned, s.next_in_s), (2, false, 60));
    }

    #[test]
    fn renewed_signal_stays_live() {
        let mut c = three();
        c.at(0.0, &[]);
        let mut sig = c.signal("b", 0.0, 10);
        for t in 0..60 {
            sig.1.renewed_ms = WALL0 + t * 1000;
            let s = c.at(t as f64, std::slice::from_ref(&sig));
            assert!(s.pinned && s.active == 1);
        }
    }

    #[test]
    fn earliest_raised_wins_then_next() {
        let mut c = three();
        c.at(0.0, &[]);
        let a = c.signal("a", 3.0, 600);
        let b = c.signal("b", 4.0, 600);
        let both = [b.clone(), a.clone()];
        assert_eq!(c.at(5.0, &both).active, 0);
        // a clears: move straight to b, still held.
        let s = c.at(6.0, std::slice::from_ref(&b));
        assert_eq!((s.active, s.pinned), (1, true));
        // b clears: rotation resumes from b.
        let s = c.at(7.0, &[]);
        assert_eq!((s.active, s.pinned), (1, false));
    }

    #[test]
    fn current_hold_is_kept_when_a_later_signal_arrives() {
        let mut c = three();
        c.at(0.0, &[]);
        let a = c.signal("a", 1.0, 600);
        c.at(1.0, std::slice::from_ref(&a));
        let c_sig = c.signal("c", 2.0, 600);
        assert_eq!(c.at(2.0, &[a, c_sig]).active, 0);
    }

    #[test]
    fn ignores_unknown_and_non_urgent_signals() {
        let mut c = three();
        c.at(0.0, &[]);
        let sig = [c.signal("nope", 1.0, 600)];
        let s = c.at(1.0, &sig);
        assert_eq!((s.active, s.pinned), (0, false));
    }

    #[test]
    fn switches_within_two_seconds_of_signal() {
        let mut c = three();
        c.run(0.0, 9.5, &[]);
        let sig = [c.signal("c", 10.0, 60)];
        // Signal appears at 10.0; the next 500 ms tick picks it up.
        assert_eq!(c.at(10.5, &sig).active, 2);
    }
}
