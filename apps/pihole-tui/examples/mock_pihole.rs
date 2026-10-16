//! A fake Pi-hole v6 API for developing the dashboard without a Pi.
//!
//!     cargo run -p pihole-tui --example mock_pihole            # http://127.0.0.1:8053
//!     cargo run -p pihole-tui --example mock_pihole -- 9000    # other port
//!
//! Password is `dev`. Stop it with Ctrl-C to see the stale and FTL-down
//! screens; start it again to watch the dashboard recover.

use std::time::{SystemTime, UNIX_EPOCH};

use serde_json::json;
use wiremock::matchers::{body_json, header, method, path};
use wiremock::{Mock, MockServer, Request, Respond, ResponseTemplate};

const SID: &str = "dev-session";

/// Counters that grow on every request so the screen visibly updates.
struct Summary;

impl Respond for Summary {
    fn respond(&self, _: &Request) -> ResponseTemplate {
        let t = now();
        let total = 40_000 + (t as u64 % 86_400) / 2;
        let blocked = total / 4 + (t as u64 % 7);
        ResponseTemplate::new(200).set_body_json(json!({
            "queries": {"total": total, "blocked": blocked,
                        "percent_blocked": blocked as f64 * 100.0 / total as f64},
            "clients": {"active": 17, "total": 22},
            "gravity": {"domains_being_blocked": 1_204_311, "last_update": t as i64 - 3 * 3600},
            "took": 0.001
        }))
    }
}

struct History;

impl Respond for History {
    fn respond(&self, _: &Request) -> ResponseTemplate {
        let end = (now() / 600.0).floor() * 600.0;
        let history: Vec<_> = (0..144)
            .map(|i| {
                let ts = end - (143 - i) as f64 * 600.0;
                let hour = (ts / 3600.0) % 24.0;
                let x = (hour - 6.0) / 24.0 * std::f64::consts::TAU;
                let total = (900.0 + 700.0 * x.sin()).max(50.0) as u64;
                json!({"timestamp": ts, "total": total, "cached": total / 3,
                       "blocked": total / 4, "forwarded": total / 2})
            })
            .collect();
        ResponseTemplate::new(200).set_body_json(json!({"history": history, "took": 0.002}))
    }
}

fn now() -> f64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_secs_f64()
}

#[tokio::main]
async fn main() {
    let port: u16 = std::env::args()
        .nth(1)
        .and_then(|p| p.parse().ok())
        .unwrap_or(8053);
    let listener = std::net::TcpListener::bind(("127.0.0.1", port)).expect("bind");
    let server = MockServer::builder().listener(listener).start().await;

    Mock::given(method("POST"))
        .and(path("/api/auth"))
        .and(body_json(json!({"password": "dev"})))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "session": {"valid": true, "totp": false, "sid": SID, "csrf": "x",
                        "validity": 1800, "message": "app-password correct"}})))
        .mount(&server)
        .await;
    Mock::given(method("POST"))
        .and(path("/api/auth"))
        .respond_with(ResponseTemplate::new(401).set_body_json(json!({
            "session": {"valid": false, "totp": false, "sid": null, "csrf": null,
                        "validity": -1, "message": "password incorrect"}})))
        .mount(&server)
        .await;
    Mock::given(method("DELETE"))
        .and(path("/api/auth"))
        .respond_with(ResponseTemplate::new(204))
        .mount(&server)
        .await;

    let authed = |p: &'static str| {
        Mock::given(method("GET"))
            .and(path(p))
            .and(header("X-FTL-SID", SID))
    };
    authed("/api/stats/summary")
        .respond_with(Summary)
        .mount(&server)
        .await;
    authed("/api/history")
        .respond_with(History)
        .mount(&server)
        .await;
    authed("/api/stats/top_domains")
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({"domains": [
            {"domain": "doubleclick.net", "count": 2104},
            {"domain": "app-measurement.com", "count": 1388},
            {"domain": "graph.facebook.com", "count": 907},
            {"domain": "ads.roku.com", "count": 611},
            {"domain": "a-very-long-tracking-subdomain.ads.example-network.com", "count": 402},
            {"domain": "telemetry.example.org", "count": 98}
        ]})))
        .mount(&server)
        .await;
    authed("/api/dns/blocking")
        .respond_with(
            ResponseTemplate::new(200).set_body_json(json!({"blocking": "enabled", "timer": null})),
        )
        .mount(&server)
        .await;
    authed("/api/info/version")
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({"version": {
            "core": {"local": {"version": "v6.1"}}}})))
        .mount(&server)
        .await;
    // Anything else without a valid session: like Pi-hole, 401.
    Mock::given(method("GET"))
        .respond_with(ResponseTemplate::new(401).set_body_json(json!({
            "error": {"key": "unauthorized", "message": "Unauthorized", "hint": null}})))
        .mount(&server)
        .await;

    eprintln!(
        "mock Pi-hole API on {} (password: dev); Ctrl-C to stop",
        server.uri()
    );
    tokio::signal::ctrl_c().await.ok();
}
