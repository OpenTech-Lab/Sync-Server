/// Integration tests for /auth endpoints.
///
/// These tests require a running server and real database.
/// Set `TEST_BASE_URL` (e.g. `http://localhost:8080`) to run them.
/// If the env var is absent the tests are skipped.
use reqwest::Client;
use serde_json::{json, Value};
use uuid::Uuid;

fn base_url() -> Option<String> {
    std::env::var("TEST_BASE_URL").ok()
}

fn unique_email() -> String {
    format!("test-{}@example.com", Uuid::new_v4())
}

/// POST /auth/register with a fresh email → 201 + UserPublic body
#[tokio::test]
async fn register_returns_201() {
    let Some(base) = base_url() else { return };
    let client = Client::new();
    let email = unique_email();

    let res = client
        .post(format!("{base}/auth/register"))
        .json(&json!({ "email": email, "password": "hunter2_secure!" }))
        .send()
        .await
        .expect("request failed");

    assert_eq!(res.status(), 201, "expected 201 Created");
    let body: Value = res.json().await.expect("invalid JSON");
    assert_eq!(body["email"], email);
    assert!(body["id"].as_str().is_some(), "missing id field");
    assert!(
        body.get("password_hash").is_none(),
        "hash must not be exposed"
    );
}

/// Registering the same email twice → 409
#[tokio::test]
async fn duplicate_register_returns_409() {
    let Some(base) = base_url() else { return };
    let client = Client::new();
    let email = unique_email();
    let payload = json!({ "email": email, "password": "hunter2_secure!" });

    client
        .post(format!("{base}/auth/register"))
        .json(&payload)
        .send()
        .await
        .expect("first request failed");

    let res = client
        .post(format!("{base}/auth/register"))
        .json(&payload)
        .send()
        .await
        .expect("second request failed");

    assert_eq!(
        res.status(),
        409,
        "expected 409 Conflict on duplicate email"
    );
}

/// POST /auth/login with correct credentials → 200 + TokenResponse
#[tokio::test]
async fn login_returns_tokens() {
    let Some(base) = base_url() else { return };
    let client = Client::new();
    let email = unique_email();
    let password = "hunter2_secure!";

    // Register first
    client
        .post(format!("{base}/auth/register"))
        .json(&json!({ "email": email, "password": password }))
        .send()
        .await
        .expect("register failed");

    let res = client
        .post(format!("{base}/auth/login"))
        .json(&json!({ "email": email, "password": password }))
        .send()
        .await
        .expect("login failed");

    assert_eq!(res.status(), 200);
    let body: Value = res.json().await.expect("invalid JSON");
    assert!(
        body["access_token"].as_str().is_some(),
        "missing access_token"
    );
    assert!(
        body["refresh_token"].as_str().is_some(),
        "missing refresh_token"
    );
    assert!(body["expires_in"].as_i64().is_some(), "missing expires_in");
}

/// POST /auth/login with wrong password → 401
#[tokio::test]
async fn login_wrong_password_returns_401() {
    let Some(base) = base_url() else { return };
    let client = Client::new();
    let email = unique_email();

    client
        .post(format!("{base}/auth/register"))
        .json(&json!({ "email": email, "password": "correct-pass" }))
        .send()
        .await
        .expect("register failed");

    let res = client
        .post(format!("{base}/auth/login"))
        .json(&json!({ "email": email, "password": "wrong-pass" }))
        .send()
        .await
        .expect("login failed");

    assert_eq!(res.status(), 401);
}

/// POST /auth/refresh with a valid refresh token → 200 + new tokens
#[tokio::test]
async fn refresh_returns_new_tokens() {
    let Some(base) = base_url() else { return };
    let client = Client::new();
    let email = unique_email();

    client
        .post(format!("{base}/auth/register"))
        .json(&json!({ "email": email, "password": "hunter2_secure!" }))
        .send()
        .await
        .unwrap();

    let login: Value = client
        .post(format!("{base}/auth/login"))
        .json(&json!({ "email": email, "password": "hunter2_secure!" }))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();

    let old_refresh = login["refresh_token"].as_str().unwrap().to_owned();

    let res = client
        .post(format!("{base}/auth/refresh"))
        .json(&json!({ "refresh_token": old_refresh }))
        .send()
        .await
        .expect("refresh failed");

    assert_eq!(res.status(), 200);
    let body: Value = res.json().await.unwrap();
    let new_refresh = body["refresh_token"].as_str().unwrap();
    // Rotation: new token must differ from the old one
    assert_ne!(new_refresh, old_refresh, "refresh token must be rotated");
    assert!(body["access_token"].as_str().is_some());
}

/// Replay detection: using a consumed refresh token → 401 (family revoked)
#[tokio::test]
async fn refresh_replay_revokes_family() {
    let Some(base) = base_url() else { return };
    let client = Client::new();
    let email = unique_email();

    client
        .post(format!("{base}/auth/register"))
        .json(&json!({ "email": email, "password": "hunter2_secure!" }))
        .send()
        .await
        .unwrap();

    let login: Value = client
        .post(format!("{base}/auth/login"))
        .json(&json!({ "email": email, "password": "hunter2_secure!" }))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();

    let old_refresh = login["refresh_token"].as_str().unwrap().to_owned();

    // First use — consumes the token
    client
        .post(format!("{base}/auth/refresh"))
        .json(&json!({ "refresh_token": old_refresh }))
        .send()
        .await
        .unwrap();

    // Second use of the same token — should be rejected
    let res = client
        .post(format!("{base}/auth/refresh"))
        .json(&json!({ "refresh_token": old_refresh }))
        .send()
        .await
        .unwrap();

    assert_eq!(res.status(), 401, "replayed token must be rejected");
}

/// POST /auth/logout → 204; subsequent refresh fails
#[tokio::test]
async fn logout_invalidates_session() {
    let Some(base) = base_url() else { return };
    let client = Client::new();
    let email = unique_email();

    client
        .post(format!("{base}/auth/register"))
        .json(&json!({ "email": email, "password": "hunter2_secure!" }))
        .send()
        .await
        .unwrap();

    let login: Value = client
        .post(format!("{base}/auth/login"))
        .json(&json!({ "email": email, "password": "hunter2_secure!" }))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();

    let access_token = login["access_token"].as_str().unwrap().to_owned();
    let refresh_token = login["refresh_token"].as_str().unwrap().to_owned();

    let res = client
        .post(format!("{base}/auth/logout"))
        .bearer_auth(&access_token)
        .json(&json!({ "refresh_token": refresh_token }))
        .send()
        .await
        .unwrap();

    assert_eq!(res.status(), 204, "logout must return 204");

    // Trying to refresh after logout should fail
    let res2 = client
        .post(format!("{base}/auth/refresh"))
        .json(&json!({ "refresh_token": refresh_token }))
        .send()
        .await
        .unwrap();

    assert_eq!(res2.status(), 401, "refresh after logout must fail");
}
