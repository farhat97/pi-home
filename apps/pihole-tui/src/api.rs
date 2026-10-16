//! Pi-hole v6 REST API client.
//!
//! Auth is session based: `POST /api/auth` with the (app) password returns a
//! session ID, sent as `X-FTL-SID` on later requests. A 401 means the session
//! expired; the client logs in again and retries once. Pi-hole v6 serves the
//! API from `pihole-FTL` itself, so a connection failure means FTL is down.

use std::time::Duration;

use reqwest::StatusCode;
use serde::Deserialize;
use serde::de::DeserializeOwned;

const TIMEOUT: Duration = Duration::from_secs(3);

#[derive(Debug, thiserror::Error)]
pub enum ApiError {
    /// No connection or no answer in time: FTL is down or unreachable.
    #[error("Pi-hole API unreachable: {0}")]
    Unreachable(String),
    /// Login rejected, e.g. a wrong password.
    #[error("authentication failed: {0}")]
    Auth(String),
    /// Any other unexpected HTTP status or body.
    #[error("Pi-hole API error: {0}")]
    Other(String),
}

impl ApiError {
    fn from_reqwest(e: reqwest::Error) -> Self {
        if e.is_connect() || e.is_timeout() {
            Self::Unreachable(root_cause(&e))
        } else {
            Self::Other(root_cause(&e))
        }
    }
}

fn root_cause(e: &(dyn std::error::Error + 'static)) -> String {
    let mut cur = e;
    while let Some(next) = cur.source() {
        cur = next;
    }
    cur.to_string()
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct Summary {
    pub queries: SummaryQueries,
    pub clients: SummaryClients,
    pub gravity: SummaryGravity,
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct SummaryQueries {
    pub total: u64,
    pub blocked: u64,
    pub percent_blocked: f64,
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct SummaryClients {
    pub active: u64,
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct SummaryGravity {
    pub domains_being_blocked: i64,
    /// Unix seconds; 0 when unknown.
    pub last_update: i64,
}

/// One 10-minute bucket of the 24h activity graph.
#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct HistoryPoint {
    pub timestamp: f64,
    pub total: u64,
    pub blocked: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct TopDomain {
    pub domain: String,
    pub count: u64,
}

/// `GET /api/dns/blocking`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Blocking {
    Enabled,
    Disabled,
    Failed,
    #[serde(other)]
    Unknown,
}

#[derive(Deserialize)]
struct AuthResponse {
    session: Session,
}

#[derive(Deserialize)]
struct Session {
    valid: bool,
    sid: Option<String>,
    message: Option<String>,
}

#[derive(Deserialize)]
struct ErrorResponse {
    error: ErrorBody,
}

#[derive(Deserialize)]
struct ErrorBody {
    message: String,
}

#[derive(Deserialize)]
struct HistoryResponse {
    history: Vec<HistoryPoint>,
}

#[derive(Deserialize)]
struct TopDomainsResponse {
    domains: Vec<TopDomain>,
}

#[derive(Deserialize)]
struct BlockingResponse {
    blocking: Blocking,
}

#[derive(Deserialize)]
struct VersionResponse {
    version: VersionInfo,
}

#[derive(Deserialize)]
struct VersionInfo {
    core: VersionComponent,
}

#[derive(Deserialize)]
struct VersionComponent {
    local: VersionLocal,
}

#[derive(Deserialize)]
struct VersionLocal {
    version: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum Auth {
    /// Not logged in yet, or the session expired.
    None,
    /// Logged in with this session ID.
    Sid(String),
    /// The Pi-hole has no password; requests need no session.
    Open,
}

pub struct Client {
    http: reqwest::Client,
    api: String,
    password: String,
    auth: Auth,
}

impl Client {
    /// `base_url` is the Pi-hole web server, e.g. `http://127.0.0.1`.
    pub fn new(base_url: &str, password: impl Into<String>) -> Self {
        let http = reqwest::Client::builder()
            .timeout(TIMEOUT)
            .connect_timeout(TIMEOUT)
            .build()
            .expect("HTTP client");
        Self {
            http,
            api: format!("{}/api", base_url.trim_end_matches('/')),
            password: password.into(),
            auth: Auth::None,
        }
    }

    pub async fn summary(&mut self) -> Result<Summary, ApiError> {
        self.get("/stats/summary").await
    }

    pub async fn history(&mut self) -> Result<Vec<HistoryPoint>, ApiError> {
        Ok(self.get::<HistoryResponse>("/history").await?.history)
    }

    pub async fn top_blocked(&mut self, count: usize) -> Result<Vec<TopDomain>, ApiError> {
        let path = format!("/stats/top_domains?blocked=true&count={count}");
        Ok(self.get::<TopDomainsResponse>(&path).await?.domains)
    }

    pub async fn blocking(&mut self) -> Result<Blocking, ApiError> {
        Ok(self
            .get::<BlockingResponse>("/dns/blocking")
            .await?
            .blocking)
    }

    /// Pi-hole core version, e.g. `v6.1`.
    pub async fn version(&mut self) -> Result<Option<String>, ApiError> {
        Ok(self
            .get::<VersionResponse>("/info/version")
            .await?
            .version
            .core
            .local
            .version)
    }

    /// Ends the session so it doesn't hold one of Pi-hole's session slots.
    pub async fn logout(&mut self) {
        if let Auth::Sid(sid) = std::mem::replace(&mut self.auth, Auth::None) {
            let _ = self
                .http
                .delete(format!("{}/auth", self.api))
                .header("X-FTL-SID", sid)
                .send()
                .await;
        }
    }

    async fn login(&mut self) -> Result<(), ApiError> {
        let resp = self
            .http
            .post(format!("{}/auth", self.api))
            .json(&serde_json::json!({ "password": self.password }))
            .send()
            .await
            .map_err(ApiError::from_reqwest)?;
        let status = resp.status();
        let body = resp.bytes().await.map_err(ApiError::from_reqwest)?;

        if let Ok(AuthResponse { session }) = serde_json::from_slice(&body) {
            if session.valid {
                self.auth = session.sid.map_or(Auth::Open, Auth::Sid);
                return Ok(());
            }
            let msg = session.message.unwrap_or_else(|| "login rejected".into());
            return Err(ApiError::Auth(msg));
        }
        let msg = error_message(&body).unwrap_or_else(|| format!("HTTP {status}"));
        Err(match status {
            StatusCode::BAD_REQUEST | StatusCode::UNAUTHORIZED | StatusCode::FORBIDDEN => {
                ApiError::Auth(msg)
            }
            _ => ApiError::Other(format!("login: {msg}")),
        })
    }

    async fn get<T: DeserializeOwned>(&mut self, path: &str) -> Result<T, ApiError> {
        let mut retried = false;
        loop {
            if self.auth == Auth::None {
                self.login().await?;
            }
            let mut req = self.http.get(format!("{}{path}", self.api));
            if let Auth::Sid(sid) = &self.auth {
                req = req.header("X-FTL-SID", sid);
            }
            let resp = req.send().await.map_err(ApiError::from_reqwest)?;
            let status = resp.status();

            if status == StatusCode::UNAUTHORIZED && !retried {
                // Session expired (or a password was set since): log in again.
                self.auth = Auth::None;
                retried = true;
                continue;
            }
            let body = resp.bytes().await.map_err(ApiError::from_reqwest)?;
            if status == StatusCode::UNAUTHORIZED {
                self.auth = Auth::None;
                let msg = error_message(&body).unwrap_or_else(|| "unauthorized".into());
                return Err(ApiError::Auth(msg));
            }
            if !status.is_success() {
                let msg = error_message(&body).unwrap_or_default();
                return Err(ApiError::Other(format!("GET {path}: HTTP {status} {msg}")));
            }
            return serde_json::from_slice(&body)
                .map_err(|e| ApiError::Other(format!("GET {path}: {e}")));
        }
    }
}

fn error_message(body: &[u8]) -> Option<String> {
    serde_json::from_slice::<ErrorResponse>(body)
        .ok()
        .map(|e| e.error.message)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use wiremock::matchers::{body_json, header, method, path, query_param};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    async fn server_with_login(sid: &str) -> MockServer {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/api/auth"))
            .and(body_json(json!({"password": "pw"})))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "session": {"valid": true, "totp": false, "sid": sid, "csrf": "c",
                            "validity": 1800, "message": "app-password correct"},
                "took": 0.01
            })))
            .mount(&server)
            .await;
        server
    }

    async fn ok(server: &MockServer, p: &str, sid: &str, body: serde_json::Value) {
        Mock::given(method("GET"))
            .and(path(p))
            .and(header("X-FTL-SID", sid))
            .respond_with(ResponseTemplate::new(200).set_body_json(body))
            .mount(server)
            .await;
    }

    #[tokio::test]
    async fn summary() {
        let server = server_with_login("S1").await;
        ok(
            &server,
            "/api/stats/summary",
            "S1",
            json!({
                "queries": {"total": 48213, "blocked": 11902, "percent_blocked": 24.686,
                            "unique_domains": 1, "forwarded": 1, "cached": 1},
                "clients": {"active": 17, "total": 22},
                "gravity": {"domains_being_blocked": 1200000, "last_update": 1725194639},
                "took": 0.003
            }),
        )
        .await;
        let s = Client::new(&server.uri(), "pw").summary().await.unwrap();
        assert_eq!(
            (s.queries.total, s.queries.blocked, s.clients.active),
            (48213, 11902, 17)
        );
        assert_eq!(s.gravity.domains_being_blocked, 1_200_000);
        assert_eq!(s.gravity.last_update, 1725194639);
    }

    #[tokio::test]
    async fn history() {
        let server = server_with_login("S1").await;
        ok(&server, "/api/history", "S1", json!({"history": [
            {"timestamp": 1511819900.5, "total": 2134, "cached": 525, "blocked": 413, "forwarded": 1196},
            {"timestamp": 1511820500.5, "total": 2014, "cached": 52, "blocked": 43, "forwarded": 1910}
        ], "took": 0.01}))
        .await;
        let h = Client::new(&server.uri(), "pw").history().await.unwrap();
        assert_eq!(h.len(), 2);
        assert_eq!((h[0].total, h[0].blocked), (2134, 413));
    }

    #[tokio::test]
    async fn top_blocked() {
        let server = server_with_login("S1").await;
        Mock::given(method("GET"))
            .and(path("/api/stats/top_domains"))
            .and(query_param("blocked", "true"))
            .and(query_param("count", "5"))
            .and(header("X-FTL-SID", "S1"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "domains": [{"domain": "doubleclick.net", "count": 2104},
                            {"domain": "ads.roku.com", "count": 611}],
                "total_queries": 48213, "blocked_queries": 11902, "took": 0.002
            })))
            .mount(&server)
            .await;
        let d = Client::new(&server.uri(), "pw")
            .top_blocked(5)
            .await
            .unwrap();
        assert_eq!(
            d[0],
            TopDomain {
                domain: "doubleclick.net".into(),
                count: 2104
            }
        );
        assert_eq!(d.len(), 2);
    }

    #[tokio::test]
    async fn blocking_status() {
        let server = server_with_login("S1").await;
        ok(
            &server,
            "/api/dns/blocking",
            "S1",
            json!({"blocking": "failed", "timer": null, "took": 0.0}),
        )
        .await;
        let mut c = Client::new(&server.uri(), "pw");
        assert_eq!(c.blocking().await.unwrap(), Blocking::Failed);
    }

    #[test]
    fn unknown_blocking_values_parse() {
        let r: BlockingResponse = serde_json::from_str(r#"{"blocking":"something-new"}"#).unwrap();
        assert_eq!(r.blocking, Blocking::Unknown);
    }

    #[tokio::test]
    async fn version() {
        let server = server_with_login("S1").await;
        ok(
            &server,
            "/api/info/version",
            "S1",
            json!({"version": {
            "core": {"local": {"branch": "master", "version": "v6.1", "hash": "abc"},
                     "remote": {"version": "v6.1", "hash": "abc"}},
            "web": {"local": {"version": "v6.1"}},
            "ftl": {"local": {"version": "v6.1"}}
        }, "took": 0.0}),
        )
        .await;
        let v = Client::new(&server.uri(), "pw").version().await.unwrap();
        assert_eq!(v.as_deref(), Some("v6.1"));
    }

    #[tokio::test]
    async fn logs_in_once_and_reuses_session() {
        let server = server_with_login("S1").await;
        ok(
            &server,
            "/api/dns/blocking",
            "S1",
            json!({"blocking": "enabled"}),
        )
        .await;
        let mut c = Client::new(&server.uri(), "pw");
        for _ in 0..3 {
            c.blocking().await.unwrap();
        }
        let logins = server
            .received_requests()
            .await
            .unwrap()
            .iter()
            .filter(|r| r.url.path() == "/api/auth")
            .count();
        assert_eq!(logins, 1);
    }

    #[tokio::test]
    async fn reauthenticates_when_session_expires() {
        let server = MockServer::start().await;
        // First login hands out S1, later logins S2.
        Mock::given(method("POST"))
            .and(path("/api/auth"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "session": {"valid": true, "sid": "S1", "validity": 1800, "message": null}})))
            .up_to_n_times(1)
            .mount(&server)
            .await;
        Mock::given(method("POST"))
            .and(path("/api/auth"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "session": {"valid": true, "sid": "S2", "validity": 1800, "message": null}})))
            .mount(&server)
            .await;
        // S1 works once, then the session has expired.
        ok(
            &server,
            "/api/dns/blocking",
            "S2",
            json!({"blocking": "enabled"}),
        )
        .await;
        Mock::given(method("GET"))
            .and(path("/api/dns/blocking"))
            .and(header("X-FTL-SID", "S1"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({"blocking": "enabled"})))
            .up_to_n_times(1)
            .mount(&server)
            .await;
        Mock::given(method("GET"))
            .and(path("/api/dns/blocking"))
            .and(header("X-FTL-SID", "S1"))
            .respond_with(ResponseTemplate::new(401).set_body_json(json!({
                "error": {"key": "unauthorized", "message": "Unauthorized", "hint": null}})))
            .mount(&server)
            .await;

        let mut c = Client::new(&server.uri(), "pw");
        assert_eq!(c.blocking().await.unwrap(), Blocking::Enabled);
        assert_eq!(c.blocking().await.unwrap(), Blocking::Enabled);
        assert_eq!(c.auth, Auth::Sid("S2".into()));
    }

    #[tokio::test]
    async fn wrong_password() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/api/auth"))
            .respond_with(ResponseTemplate::new(401).set_body_json(json!({
                "session": {"valid": false, "totp": false, "sid": null, "csrf": null,
                            "validity": -1, "message": "password incorrect"},
                "took": 0.003})))
            .mount(&server)
            .await;
        let err = Client::new(&server.uri(), "nope")
            .summary()
            .await
            .unwrap_err();
        assert!(
            matches!(&err, ApiError::Auth(m) if m == "password incorrect"),
            "{err:?}"
        );
    }

    #[tokio::test]
    async fn no_password_pihole_needs_no_session() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/api/auth"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "session": {"valid": true, "sid": null, "validity": -1, "message": "no password set"}})))
            .mount(&server)
            .await;
        Mock::given(method("GET"))
            .and(path("/api/dns/blocking"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({"blocking": "disabled"})))
            .mount(&server)
            .await;
        let mut c = Client::new(&server.uri(), "");
        assert_eq!(c.blocking().await.unwrap(), Blocking::Disabled);
        let reqs = server.received_requests().await.unwrap();
        assert!(reqs.iter().all(|r| !r.headers.contains_key("X-FTL-SID")));
    }

    #[tokio::test]
    async fn connection_refused_is_unreachable() {
        // Bind then drop to get a port nothing listens on.
        let port = std::net::TcpListener::bind("127.0.0.1:0")
            .unwrap()
            .local_addr()
            .unwrap()
            .port();
        let err = Client::new(&format!("http://127.0.0.1:{port}"), "pw")
            .summary()
            .await
            .unwrap_err();
        assert!(matches!(err, ApiError::Unreachable(_)), "{err:?}");
    }

    #[tokio::test]
    async fn server_error_is_not_unreachable() {
        let server = server_with_login("S1").await;
        Mock::given(method("GET"))
            .and(path("/api/stats/summary"))
            .respond_with(ResponseTemplate::new(500))
            .mount(&server)
            .await;
        let err = Client::new(&server.uri(), "pw")
            .summary()
            .await
            .unwrap_err();
        assert!(matches!(err, ApiError::Other(_)), "{err:?}");
    }

    #[tokio::test]
    async fn logout_deletes_session() {
        let server = server_with_login("S1").await;
        ok(
            &server,
            "/api/dns/blocking",
            "S1",
            json!({"blocking": "enabled"}),
        )
        .await;
        Mock::given(method("DELETE"))
            .and(path("/api/auth"))
            .and(header("X-FTL-SID", "S1"))
            .respond_with(ResponseTemplate::new(204))
            .expect(1)
            .mount(&server)
            .await;
        let mut c = Client::new(&server.uri(), "pw");
        c.blocking().await.unwrap();
        c.logout().await;
        assert_eq!(c.auth, Auth::None);
    }
}
