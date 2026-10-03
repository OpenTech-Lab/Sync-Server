/// Integration tests for the `/ws` handshake (ticket + legacy token).
///
/// These tests require a running server, Redis and a real database.
/// Set `TEST_BASE_URL` (e.g. `http://localhost:8080`) to run them.
/// If the env var is absent the tests are skipped.
///
/// The legacy `?token=` handshake is gated by the server's
/// `WS_ALLOW_LEGACY_TOKEN` flag (default on). Run once per mode: with the flag
/// off, also set `TEST_WS_ALLOW_LEGACY_TOKEN=false` so the expectation matches.
use reqwest::Client;
use serde_json::{json, Value};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;
use uuid::Uuid;

fn base_url() -> Option<String> {
    std::env::var("TEST_BASE_URL").ok()
}

fn legacy_expected_on() -> bool {
    !matches!(
        std::env::var("TEST_WS_ALLOW_LEGACY_TOKEN")
            .unwrap_or_default()
            .trim()
            .to_ascii_lowercase()
            .as_str(),
        "0" | "false" | "no" | "off"
    )
}

/// Helper: register a user and return their access token.
async fn register_and_login(client: &Client, base: &str) -> String {
    let email = format!("test-{}@example.com", Uuid::new_v4());
    let username = format!("test{}", &Uuid::new_v4().simple().to_string()[..12]);
    let password = "test_pass_secure!";

    // Registration is capped per client IP per hour; give each user its own.
    let octets = Uuid::new_v4().into_bytes();
    let client_ip = format!("10.{}.{}.{}", octets[0], octets[1], octets[2]);

    client
        .post(format!("{base}/auth/register"))
        .header("X-Real-IP", client_ip)
        .json(&json!({ "username": username, "email": email, "password": password }))
        .send()
        .await
        .expect("register failed");

    let login: Value = client
        .post(format!("{base}/auth/login"))
        .json(&json!({ "email": email, "password": password }))
        .send()
        .await
        .expect("login failed")
        .json()
        .await
        .expect("login invalid JSON");
    login["access_token"]
        .as_str()
        .expect("missing access_token")
        .to_owned()
}

async fn issue_ticket(client: &Client, base: &str, access_token: &str) -> String {
    let res = client
        .post(format!("{base}/auth/ws-ticket"))
        .bearer_auth(access_token)
        .send()
        .await
        .expect("ws-ticket failed");
    assert_eq!(res.status(), 200, "ws-ticket should succeed");
    let body: Value = res.json().await.unwrap();
    let ticket = body["ticket"].as_str().expect("missing ticket").to_owned();
    assert_eq!(ticket.len(), 64);
    assert!(ticket.bytes().all(|b| b.is_ascii_hexdigit()));
    ticket
}

/// Performs a raw WebSocket upgrade handshake and returns the HTTP status
/// (101 when the upgrade succeeds). Avoids a WS client dependency: only the
/// handshake outcome matters here.
async fn ws_handshake_status(base: &str, path_and_query: &str) -> u16 {
    let authority = base
        .trim_start_matches("http://")
        .trim_start_matches("https://")
        .trim_end_matches('/');
    let mut stream = TcpStream::connect(authority)
        .await
        .expect("connect to test server");
    let request = format!(
        "GET {path_and_query} HTTP/1.1\r\nHost: {authority}\r\nUpgrade: websocket\r\n\
         Connection: Upgrade\r\nSec-WebSocket-Key: dGhlIHNhbXBsZSBub25jZQ==\r\n\
         Sec-WebSocket-Version: 13\r\n\r\n"
    );
    stream.write_all(request.as_bytes()).await.unwrap();
    let mut buf = [0u8; 256];
    let n = stream.read(&mut buf).await.unwrap();
    let head = String::from_utf8_lossy(&buf[..n]);
    head.split_whitespace()
        .nth(1)
        .and_then(|code| code.parse().ok())
        .unwrap_or_else(|| panic!("unparseable handshake response: {head}"))
}

/// A ticket opens the socket once; replaying it is refused.
#[tokio::test]
async fn ws_ticket_connects_once_and_cannot_be_replayed() {
    let Some(base) = base_url() else { return };
    let client = Client::new();
    let token = register_and_login(&client, &base).await;
    let ticket = issue_ticket(&client, &base, &token).await;

    assert_eq!(
        ws_handshake_status(&base, &format!("/ws?ticket={ticket}")).await,
        101
    );
    assert_eq!(
        ws_handshake_status(&base, &format!("/ws?ticket={ticket}")).await,
        401,
        "ticket must be single-use"
    );
}

/// A ticket the server never issued is refused.
#[tokio::test]
async fn ws_unknown_ticket_is_rejected() {
    let Some(base) = base_url() else { return };
    let ticket = "ab".repeat(32);
    assert_eq!(
        ws_handshake_status(&base, &format!("/ws?ticket={ticket}")).await,
        401
    );
}

/// Legacy `?token=<access JWT>` (installed apps <= 1.3.2) works while
/// WS_ALLOW_LEGACY_TOKEN is on and is refused once it is off.
#[tokio::test]
async fn ws_legacy_token_follows_the_transition_flag() {
    let Some(base) = base_url() else { return };
    let client = Client::new();
    let token = register_and_login(&client, &base).await;

    let expected = if legacy_expected_on() { 101 } else { 401 };
    assert_eq!(
        ws_handshake_status(&base, &format!("/ws?token={token}")).await,
        expected
    );
}

/// Legacy `?token=` with a bad JWT is refused in either flag state, and a bad
/// ticket is never retried as a token.
#[tokio::test]
async fn ws_invalid_credentials_are_rejected() {
    let Some(base) = base_url() else { return };
    assert_eq!(ws_handshake_status(&base, "/ws?token=not-a-jwt").await, 401);
    assert_eq!(ws_handshake_status(&base, "/ws").await, 401);

    let client = Client::new();
    let token = register_and_login(&client, &base).await;
    assert_eq!(
        ws_handshake_status(&base, &format!("/ws?ticket=nope&token={token}")).await,
        401
    );
}
