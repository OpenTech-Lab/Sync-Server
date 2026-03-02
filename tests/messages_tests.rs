/// Integration tests for /api/messages endpoints.
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

/// Helper: register a user and return their access token + user_id.
async fn register_and_login(client: &Client, base: &str) -> (String, String) {
    let email = unique_email();
    let password = "test_pass_secure!";

    let reg: Value = client
        .post(format!("{base}/auth/register"))
        .json(&json!({ "email": email, "password": password }))
        .send()
        .await
        .expect("register failed")
        .json()
        .await
        .expect("register invalid JSON");

    let user_id = reg["id"].as_str().expect("missing id").to_owned();

    let login: Value = client
        .post(format!("{base}/auth/login"))
        .json(&json!({ "email": email, "password": password }))
        .send()
        .await
        .expect("login failed")
        .json()
        .await
        .expect("login invalid JSON");

    let access_token = login["access_token"]
        .as_str()
        .expect("missing access_token")
        .to_owned();
    (access_token, user_id)
}

/// POST /api/messages → 201 + saved message body
#[tokio::test]
async fn send_message_returns_201() {
    let Some(base) = base_url() else { return };
    let client = Client::new();

    let (token_a, _id_a) = register_and_login(&client, &base).await;
    let (_token_b, id_b) = register_and_login(&client, &base).await;

    let res = client
        .post(format!("{base}/api/messages"))
        .bearer_auth(&token_a)
        .json(&json!({ "recipient_id": id_b, "content": "Hello!" }))
        .send()
        .await
        .expect("send_message failed");

    assert_eq!(res.status(), 201, "expected 201 Created");
    let body: Value = res.json().await.unwrap();
    assert_eq!(body["content"], "Hello!");
    assert!(body["id"].as_str().is_some());
}

/// GET /api/messages/{partner_id} → 200 + array containing sent message
#[tokio::test]
async fn get_conversation_returns_messages() {
    let Some(base) = base_url() else { return };
    let client = Client::new();

    let (token_a, _id_a) = register_and_login(&client, &base).await;
    let (_token_b, id_b) = register_and_login(&client, &base).await;

    // Send a message A → B
    client
        .post(format!("{base}/api/messages"))
        .bearer_auth(&token_a)
        .json(&json!({ "recipient_id": id_b, "content": "Ping" }))
        .send()
        .await
        .unwrap();

    let res = client
        .get(format!("{base}/api/messages/{id_b}"))
        .bearer_auth(&token_a)
        .send()
        .await
        .expect("get_conversation failed");

    assert_eq!(res.status(), 200);
    let body: Value = res.json().await.unwrap();
    let msgs = body.as_array().expect("expected array");
    assert!(
        !msgs.is_empty(),
        "conversation should contain at least one message"
    );
    assert_eq!(msgs[0]["content"], "Ping");
}

/// Keyset pagination: ?before=<id> returns an earlier page
#[tokio::test]
async fn pagination_before_cursor() {
    let Some(base) = base_url() else { return };
    let client = Client::new();

    let (token_a, _id_a) = register_and_login(&client, &base).await;
    let (_token_b, id_b) = register_and_login(&client, &base).await;

    // Send 3 messages
    let mut first_id = String::new();
    for i in 1u8..=3 {
        let sent: Value = client
            .post(format!("{base}/api/messages"))
            .bearer_auth(&token_a)
            .json(&json!({ "recipient_id": id_b, "content": format!("msg {i}") }))
            .send()
            .await
            .unwrap()
            .json()
            .await
            .unwrap();
        if i == 1 {
            first_id = sent["id"].as_str().unwrap().to_owned();
        }
    }

    // Fetch with limit=1 first — get the latest message id
    let page1: Value = client
        .get(format!("{base}/api/messages/{id_b}?limit=1"))
        .bearer_auth(&token_a)
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();

    let cursor = page1[0]["id"].as_str().expect("cursor id").to_owned();

    // Fetch the page before that cursor
    let page2: Value = client
        .get(format!(
            "{base}/api/messages/{id_b}?before={cursor}&limit=10"
        ))
        .bearer_auth(&token_a)
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();

    let ids: Vec<&str> = page2
        .as_array()
        .unwrap()
        .iter()
        .map(|m| m["id"].as_str().unwrap())
        .collect();

    assert!(
        !ids.contains(&cursor.as_str()),
        "cursor message must not appear in previous page"
    );
    assert!(
        ids.contains(&first_id.as_str()),
        "first message must appear in earlier page"
    );
}

/// POST /api/messages/{partner_id}/read → 200 + updates read state
#[tokio::test]
async fn mark_read_returns_ok() {
    let Some(base) = base_url() else { return };
    let client = Client::new();

    let (token_a, id_a) = register_and_login(&client, &base).await;
    let (token_b, id_b) = register_and_login(&client, &base).await;

    // A sends a message to B
    client
        .post(format!("{base}/api/messages"))
        .bearer_auth(&token_a)
        .json(&json!({ "recipient_id": id_b, "content": "read me" }))
        .send()
        .await
        .unwrap();

    // B marks messages from A as read
    let res = client
        .post(format!("{base}/api/messages/{id_a}/read"))
        .bearer_auth(&token_b)
        .send()
        .await
        .expect("mark_read failed");

    assert!(
        res.status().is_success(),
        "mark_read should succeed, got {}",
        res.status()
    );
}

/// GET /api/messages/unread-counts → map of partner_id → count
#[tokio::test]
async fn unread_counts_reflects_new_messages() {
    let Some(base) = base_url() else { return };
    let client = Client::new();

    let (token_a, id_a) = register_and_login(&client, &base).await;
    let (token_b, _id_b) = register_and_login(&client, &base).await;

    // A sends a message to B
    client
        .post(format!("{base}/api/messages"))
        .bearer_auth(&token_a)
        .json(&json!({ "recipient_id": _id_b, "content": "unread msg" }))
        .send()
        .await
        .unwrap();

    // B checks unread counts — should see 1 from A
    let res = client
        .get(format!("{base}/api/messages/unread-counts"))
        .bearer_auth(&token_b)
        .send()
        .await
        .expect("unread_counts failed");

    assert_eq!(res.status(), 200);
    let counts: Value = res.json().await.unwrap();
    let count_from_a = counts[&id_a].as_i64().unwrap_or(0);
    assert_eq!(count_from_a, 1, "expected 1 unread from user A");
}

/// Unauthenticated request → 401
#[tokio::test]
async fn unauthenticated_send_returns_401() {
    let Some(base) = base_url() else { return };
    let client = Client::new();

    let res = client
        .post(format!("{base}/api/messages"))
        .json(&json!({ "recipient_id": Uuid::new_v4(), "content": "sneaky" }))
        .send()
        .await
        .expect("request failed");

    assert_eq!(res.status(), 401);
}
