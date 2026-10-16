//! Last-known Pi-hole data and the connection state derived from polls.

use std::time::{Duration, Instant};

use crate::api::{ApiError, Blocking, HistoryPoint, Summary, TopDomain};

/// The API must be unreachable this long before the app asks for the screen.
pub const UNREACHABLE_ALERT_AFTER: Duration = Duration::from_secs(30);

/// Result of one successful poll. History and version are fetched less often
/// and are `None` when not part of this poll.
#[derive(Debug, Clone, Default)]
pub struct Update {
    pub summary: Option<Summary>,
    pub blocking: Option<Blocking>,
    pub top_blocked: Option<Vec<TopDomain>>,
    pub history: Option<Vec<HistoryPoint>>,
    pub version: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Connection {
    /// No successful poll yet.
    Waiting,
    /// The last poll succeeded.
    Ok,
    /// The last poll failed after data was loaded; values are last-known.
    Stale,
    /// Login rejected; the message comes from Pi-hole.
    AuthError(String),
}

#[derive(Debug, Clone)]
pub struct Model {
    pub summary: Option<Summary>,
    pub blocking: Option<Blocking>,
    pub top_blocked: Vec<TopDomain>,
    pub history: Vec<HistoryPoint>,
    pub version: Option<String>,
    pub connection: Connection,
    pub last_success: Option<Instant>,
    /// Message of the most recent failure, if the last poll failed.
    pub last_error: Option<String>,
    /// Since when the API has been unreachable (FTL down), if it is.
    unreachable_since: Option<Instant>,
}

impl Default for Model {
    fn default() -> Self {
        Self {
            summary: None,
            blocking: None,
            top_blocked: Vec::new(),
            history: Vec::new(),
            version: None,
            connection: Connection::Waiting,
            last_success: None,
            last_error: None,
            unreachable_since: None,
        }
    }
}

impl Model {
    pub fn apply(&mut self, result: Result<Update, ApiError>, now: Instant) {
        match result {
            Ok(u) => {
                if let Some(s) = u.summary {
                    self.summary = Some(s);
                }
                if let Some(b) = u.blocking {
                    self.blocking = Some(b);
                }
                if let Some(t) = u.top_blocked {
                    self.top_blocked = t;
                }
                if let Some(h) = u.history {
                    self.history = h;
                }
                if u.version.is_some() {
                    self.version = u.version;
                }
                self.connection = Connection::Ok;
                self.last_success = Some(now);
                self.last_error = None;
                self.unreachable_since = None;
            }
            Err(e) => {
                self.last_error = Some(e.to_string());
                match e {
                    ApiError::Auth(msg) => {
                        self.connection = Connection::AuthError(msg);
                        self.unreachable_since = None;
                    }
                    ApiError::Unreachable(_) => {
                        self.unreachable_since.get_or_insert(now);
                        self.connection = self.no_data_or_stale();
                    }
                    ApiError::Other(_) => {
                        self.unreachable_since = None;
                        self.connection = self.no_data_or_stale();
                    }
                }
            }
        }
    }

    fn no_data_or_stale(&self) -> Connection {
        if self.last_success.is_some() {
            Connection::Stale
        } else {
            Connection::Waiting
        }
    }

    /// The API can't be reached; in Pi-hole v6 that means FTL is down.
    pub fn ftl_down(&self) -> bool {
        self.unreachable_since.is_some()
    }

    /// Time since the last successful poll.
    pub fn age(&self, now: Instant) -> Option<Duration> {
        self.last_success.map(|t| now.saturating_duration_since(t))
    }

    /// Why the app should hold the screen, if it should.
    pub fn alert(&self, now: Instant) -> Option<String> {
        let since = self.unreachable_since?;
        (now.saturating_duration_since(since) > UNREACHABLE_ALERT_AFTER)
            .then(|| "FTL down: Pi-hole API unreachable".to_owned())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::api::{SummaryClients, SummaryGravity, SummaryQueries};

    pub(crate) fn summary(total: u64) -> Summary {
        Summary {
            queries: SummaryQueries {
                total,
                blocked: total / 4,
                percent_blocked: 25.0,
            },
            clients: SummaryClients { active: 3 },
            gravity: SummaryGravity {
                domains_being_blocked: 100,
                last_update: 0,
            },
        }
    }

    fn ok(total: u64) -> Result<Update, ApiError> {
        Ok(Update {
            summary: Some(summary(total)),
            blocking: Some(Blocking::Enabled),
            ..Default::default()
        })
    }

    fn down() -> Result<Update, ApiError> {
        Err(ApiError::Unreachable("connection refused".into()))
    }

    fn secs(t0: Instant, s: u64) -> Instant {
        t0 + Duration::from_secs(s)
    }

    #[test]
    fn starts_waiting() {
        let m = Model::default();
        assert_eq!(m.connection, Connection::Waiting);
        assert!(!m.ftl_down());
    }

    #[test]
    fn startup_with_api_down_waits_then_recovers() {
        let t0 = Instant::now();
        let mut m = Model::default();
        m.apply(down(), t0);
        assert_eq!(m.connection, Connection::Waiting);
        assert!(m.ftl_down());
        assert!(m.last_error.as_deref().unwrap().contains("refused"));
        m.apply(ok(10), secs(t0, 5));
        assert_eq!(m.connection, Connection::Ok);
        assert!(!m.ftl_down());
        assert_eq!(m.summary.as_ref().unwrap().queries.total, 10);
    }

    #[test]
    fn failure_after_data_goes_stale_and_keeps_values() {
        let t0 = Instant::now();
        let mut m = Model::default();
        m.apply(ok(10), t0);
        m.apply(down(), secs(t0, 5));
        assert_eq!(m.connection, Connection::Stale);
        assert_eq!(m.summary.as_ref().unwrap().queries.total, 10);
        assert_eq!(m.age(secs(t0, 12)), Some(Duration::from_secs(12)));
        m.apply(ok(20), secs(t0, 15));
        assert_eq!(m.connection, Connection::Ok);
        assert_eq!(m.age(secs(t0, 15)), Some(Duration::ZERO));
    }

    #[test]
    fn server_errors_are_stale_but_not_ftl_down() {
        let t0 = Instant::now();
        let mut m = Model::default();
        m.apply(Err(ApiError::Other("HTTP 500".into())), t0);
        assert_eq!(m.connection, Connection::Waiting);
        m.apply(ok(1), t0);
        m.apply(Err(ApiError::Other("HTTP 500".into())), t0);
        assert_eq!(m.connection, Connection::Stale);
        assert!(!m.ftl_down());
        assert_eq!(m.alert(secs(t0, 120)), None);
    }

    #[test]
    fn auth_error_and_recovery() {
        let t0 = Instant::now();
        let mut m = Model::default();
        m.apply(Err(ApiError::Auth("password incorrect".into())), t0);
        assert_eq!(
            m.connection,
            Connection::AuthError("password incorrect".into())
        );
        m.apply(ok(1), secs(t0, 5));
        assert_eq!(m.connection, Connection::Ok);
    }

    #[test]
    fn partial_updates_keep_previous_history_and_version() {
        let t0 = Instant::now();
        let mut m = Model::default();
        m.apply(
            Ok(Update {
                history: Some(vec![HistoryPoint {
                    timestamp: 0.0,
                    total: 5,
                    blocked: 1,
                }]),
                version: Some("v6.1".into()),
                ..Default::default()
            }),
            t0,
        );
        m.apply(ok(1), t0);
        assert_eq!(m.history.len(), 1);
        assert_eq!(m.version.as_deref(), Some("v6.1"));
    }

    #[test]
    fn alert_after_thirty_seconds_unreachable() {
        let t0 = Instant::now();
        let mut m = Model::default();
        m.apply(ok(1), t0);
        m.apply(down(), secs(t0, 5));
        m.apply(down(), secs(t0, 30));
        assert_eq!(m.alert(secs(t0, 35)), None);
        assert!(m.alert(secs(t0, 36)).unwrap().contains("FTL down"));
        m.apply(ok(1), secs(t0, 40));
        assert_eq!(m.alert(secs(t0, 40)), None);
    }
}
