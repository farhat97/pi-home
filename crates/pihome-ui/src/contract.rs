//! File contract between the rotator and the apps.
//!
//! The rotator writes `<dir>/state.json`; each app may write
//! `<dir>/signals/<app>.json` to ask for the screen. Writes are atomic
//! (temp file + rename) and readers treat missing or unreadable files as
//! absent, so either side keeps working when the other is down.
//!
//! Timestamps are wall-clock milliseconds since the Unix epoch. Every
//! freshness check takes `now` as a parameter so callers and tests control
//! the clock.

use std::fs;
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};

/// Default location of the contract files (tmpfs on the device).
pub const DEFAULT_STATE_DIR: &str = "/run/pihome";

/// Rotation state older than this is treated as absent.
pub const STATE_STALE_AFTER: Duration = Duration::from_secs(5);

/// Published by the rotator at least once per second.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RotationState {
    /// Zero-based position of the app currently on screen.
    pub active: usize,
    /// Number of apps in the rotation.
    pub count: usize,
    /// Seconds until the next scheduled switch (0 while pinned).
    pub next_in_s: u64,
    /// True while an urgent signal holds the screen.
    pub pinned: bool,
    /// When this state was written.
    pub updated_ms: u64,
}

impl RotationState {
    /// Fresh when written no more than [`STATE_STALE_AFTER`] before `now`.
    pub fn is_fresh(&self, now: SystemTime) -> bool {
        unix_ms(now).saturating_sub(self.updated_ms) <= STATE_STALE_AFTER.as_millis() as u64
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Level {
    Urgent,
}

/// An app's request for attention.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Signal {
    pub level: Level,
    /// Short human-readable cause, e.g. "FTL down".
    pub reason: String,
    /// Lifetime after the last renewal; the signal expires if not renewed.
    pub ttl_s: u64,
    /// When the condition was first raised. Kept across renewals so the
    /// rotator can order competing signals by who asked first.
    pub raised_ms: u64,
    /// When the signal was last written.
    pub renewed_ms: u64,
}

impl Signal {
    /// A new urgent signal raised at `now`.
    pub fn urgent(reason: impl Into<String>, ttl_s: u64, now: SystemTime) -> Self {
        let ms = unix_ms(now);
        Self {
            level: Level::Urgent,
            reason: reason.into(),
            ttl_s,
            raised_ms: ms,
            renewed_ms: ms,
        }
    }

    /// Expired once `ttl_s` has passed since the last renewal.
    pub fn is_expired(&self, now: SystemTime) -> bool {
        unix_ms(now).saturating_sub(self.renewed_ms) > self.ttl_s * 1000
    }
}

/// Milliseconds since the Unix epoch (0 for times before it).
pub fn unix_ms(t: SystemTime) -> u64 {
    t.duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

/// Inverse of [`unix_ms`].
pub fn from_unix_ms(ms: u64) -> SystemTime {
    UNIX_EPOCH + Duration::from_millis(ms)
}

/// Root of the contract files.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StateDir {
    base: PathBuf,
}

impl Default for StateDir {
    fn default() -> Self {
        Self::new(DEFAULT_STATE_DIR)
    }
}

impl StateDir {
    pub fn new(base: impl Into<PathBuf>) -> Self {
        Self { base: base.into() }
    }

    pub fn base(&self) -> &Path {
        &self.base
    }

    pub fn state_path(&self) -> PathBuf {
        self.base.join("state.json")
    }

    pub fn signals_dir(&self) -> PathBuf {
        self.base.join("signals")
    }

    pub fn signal_path(&self, app: &str) -> PathBuf {
        self.signals_dir().join(format!("{app}.json"))
    }

    pub fn write_state(&self, state: &RotationState) -> io::Result<()> {
        write_json_atomic(&self.state_path(), state)
    }

    /// The current state, or `None` if missing or unreadable.
    pub fn read_state(&self) -> Option<RotationState> {
        read_json(&self.state_path())
    }

    /// The state if present and fresh at `now`.
    pub fn read_fresh_state(&self, now: SystemTime) -> Option<RotationState> {
        self.read_state().filter(|s| s.is_fresh(now))
    }

    pub fn write_signal(&self, app: &str, signal: &Signal) -> io::Result<()> {
        write_json_atomic(&self.signal_path(app), signal)
    }

    /// Removes the app's signal; succeeds if there was none.
    pub fn clear_signal(&self, app: &str) -> io::Result<()> {
        match fs::remove_file(self.signal_path(app)) {
            Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(()),
            r => r,
        }
    }

    /// The app's signal, or `None` if missing or unreadable.
    pub fn read_signal(&self, app: &str) -> Option<Signal> {
        read_json(&self.signal_path(app))
    }

    /// All readable signals as `(app, signal)`, sorted by app name.
    /// Expired signals are included; filter with [`Signal::is_expired`].
    pub fn read_signals(&self) -> Vec<(String, Signal)> {
        let Ok(entries) = fs::read_dir(self.signals_dir()) else {
            return Vec::new();
        };
        let mut out: Vec<_> = entries
            .filter_map(Result::ok)
            .filter_map(|e| {
                let path = e.path();
                if path.extension()? != "json" {
                    return None;
                }
                let app = path.file_stem()?.to_str()?.to_owned();
                Some((app, read_json(&path)?))
            })
            .collect();
        out.sort_by(|a, b| a.0.cmp(&b.0));
        out
    }
}

fn read_json<T: for<'de> Deserialize<'de>>(path: &Path) -> Option<T> {
    let bytes = fs::read(path).ok()?;
    serde_json::from_slice(&bytes).ok()
}

/// Writes `value` as JSON to a temp file in the same directory, then renames
/// it over `path`, so readers see either the old or the new file, never a mix.
pub fn write_json_atomic<T: Serialize>(path: &Path, value: &T) -> io::Result<()> {
    static SEQ: AtomicU64 = AtomicU64::new(0);

    let dir = path.parent().unwrap_or(Path::new("."));
    fs::create_dir_all(dir)?;
    let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("file");
    let tmp = dir.join(format!(
        ".{name}.{}.{}.tmp",
        std::process::id(),
        SEQ.fetch_add(1, Ordering::Relaxed)
    ));

    let result = (|| {
        let mut f = fs::File::create(&tmp)?;
        f.write_all(&serde_json::to_vec(value).map_err(io::Error::other)?)?;
        f.flush()?;
        fs::rename(&tmp, path)
    })();
    if result.is_err() {
        let _ = fs::remove_file(&tmp);
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;
    use std::sync::atomic::AtomicBool;
    use std::thread;

    fn at(ms: u64) -> SystemTime {
        from_unix_ms(ms)
    }

    fn state(updated_ms: u64) -> RotationState {
        RotationState {
            active: 1,
            count: 3,
            next_in_s: 42,
            pinned: false,
            updated_ms,
        }
    }

    #[test]
    fn state_round_trips() {
        let s = RotationState {
            pinned: true,
            ..state(1_700_000_000_123)
        };
        let json = serde_json::to_string(&s).unwrap();
        assert_eq!(serde_json::from_str::<RotationState>(&json).unwrap(), s);
    }

    #[test]
    fn state_json_shape() {
        let v: serde_json::Value = serde_json::to_value(state(5)).unwrap();
        assert_eq!(
            v,
            serde_json::json!({"active":1,"count":3,"next_in_s":42,"pinned":false,"updated_ms":5})
        );
    }

    #[test]
    fn signal_round_trips() {
        let s = Signal::urgent("FTL down", 60, at(1_700_000_000_000));
        let json = serde_json::to_string(&s).unwrap();
        assert!(json.contains(r#""level":"urgent""#));
        assert_eq!(serde_json::from_str::<Signal>(&json).unwrap(), s);
    }

    #[test]
    fn write_then_read() {
        let tmp = tempfile::tempdir().unwrap();
        let dir = StateDir::new(tmp.path().join("pihome"));
        assert_eq!(dir.read_state(), None);
        dir.write_state(&state(10)).unwrap();
        assert_eq!(dir.read_state(), Some(state(10)));

        let sig = Signal::urgent("x", 30, at(1000));
        dir.write_signal("pihole-tui", &sig).unwrap();
        assert_eq!(dir.read_signal("pihole-tui"), Some(sig.clone()));
        assert_eq!(dir.read_signals(), vec![("pihole-tui".to_owned(), sig)]);

        dir.clear_signal("pihole-tui").unwrap();
        dir.clear_signal("pihole-tui").unwrap();
        assert_eq!(dir.read_signal("pihole-tui"), None);
        assert!(dir.read_signals().is_empty());
        // No temp files left behind.
        assert_eq!(fs::read_dir(dir.signals_dir()).unwrap().count(), 0);
    }

    #[test]
    fn missing_or_corrupt_reads_as_absent() {
        let tmp = tempfile::tempdir().unwrap();
        let dir = StateDir::new(tmp.path());
        assert_eq!(dir.read_state(), None);
        assert!(dir.read_signals().is_empty());

        fs::write(dir.state_path(), b"{\"active\": 1, \"cou").unwrap();
        assert_eq!(dir.read_state(), None);

        fs::create_dir_all(dir.signals_dir()).unwrap();
        fs::write(dir.signal_path("bad"), b"not json").unwrap();
        fs::write(dir.signals_dir().join("notes.txt"), b"ignored").unwrap();
        let good = Signal::urgent("ok", 30, at(1));
        dir.write_signal("good", &good).unwrap();
        assert_eq!(dir.read_signal("bad"), None);
        assert_eq!(dir.read_signals(), vec![("good".to_owned(), good)]);
    }

    #[test]
    fn concurrent_reader_never_sees_partial_file() {
        let tmp = tempfile::tempdir().unwrap();
        let dir = StateDir::new(tmp.path());
        // Large enough that a non-atomic write would be observable mid-way.
        let big = |c: char| Signal::urgent(c.to_string().repeat(256 * 1024), 60, at(1));
        dir.write_signal("app", &big('a')).unwrap();

        let done = Arc::new(AtomicBool::new(false));
        let writer = {
            let (dir, done) = (dir.clone(), done.clone());
            thread::spawn(move || {
                for i in 0..200 {
                    let c = if i % 2 == 0 { 'b' } else { 'a' };
                    dir.write_signal("app", &big(c)).unwrap();
                }
                done.store(true, Ordering::SeqCst);
            })
        };

        let mut reads = 0;
        while !done.load(Ordering::SeqCst) || reads < 200 {
            let s = dir
                .read_signal("app")
                .expect("reader saw a missing or partial file");
            let first = s.reason.chars().next().unwrap();
            assert!(s.reason.chars().all(|c| c == first), "mixed contents");
            assert_eq!(s.reason.len(), 256 * 1024);
            reads += 1;
        }
        writer.join().unwrap();
    }

    #[test]
    fn state_freshness() {
        let s = state(10_000);
        assert!(s.is_fresh(at(10_000)));
        assert!(s.is_fresh(at(15_000)));
        assert!(!s.is_fresh(at(15_001)));
        // Clock behind the writer: still fresh rather than wrapping.
        assert!(s.is_fresh(at(9_000)));
    }

    #[test]
    fn read_fresh_state_filters_stale() {
        let tmp = tempfile::tempdir().unwrap();
        let dir = StateDir::new(tmp.path());
        dir.write_state(&state(10_000)).unwrap();
        assert!(dir.read_fresh_state(at(12_000)).is_some());
        assert!(dir.read_fresh_state(at(20_000)).is_none());
    }

    #[test]
    fn signal_expiry_counts_from_renewal() {
        let mut s = Signal::urgent("x", 60, at(0));
        assert!(!s.is_expired(at(60_000)));
        assert!(s.is_expired(at(60_001)));
        s.renewed_ms = 50_000;
        assert!(!s.is_expired(at(110_000)));
        assert!(s.is_expired(at(110_001)));
    }
}
