/// Integration tests for agent admin tokens.
///
/// These tests require a running server and real database.
/// Set `TEST_BASE_URL` (e.g. `http://localhost:8080`) to run them.
/// Admin-gated tests additionally require `TEST_ADMIN_TOKEN` — a JWT whose
/// `role` claim equals "admin". If either is absent, these tests are skipped.
use reqwest::Client;
use serde_json::{json, Value};

fn base_url() -> Option<String> {
    std::env::var("TEST_BASE_URL").ok()
}

fn admin_token() -> Option<String> {
    std::env::var("TEST_ADMIN_TOKEN").ok()
}

#[tokio::test]
async fn agent_tokens_require_admin_auth() {
    let Some(base) = base_url() else { return };
    let client = Client::new();

    let res = client
        .get(format!("{base}/api/admin/agent-tokens"))
        .send()
        .await
        .expect("request failed");

    assert_eq!(res.status(), 401);
}

#[tokio::test]
async fn agent_token_full_lifecycle() {
    let (Some(base), Some(admin_tok)) = (base_url(), admin_token()) else {
        return;
    };
    let client = Client::new();

    // Create
    let create_res = client
        .post(format!("{base}/api/admin/agent-tokens"))
        .bearer_auth(&admin_tok)
        .json(&json!({ "name": "integration-test-agent" }))
        .send()
        .await
        .expect("create request failed");
    assert_eq!(create_res.status(), 201);
    let created: Value = create_res.json().await.expect("create json");
    let raw_token = created["token"]
        .as_str()
        .expect("missing token")
        .to_string();
    let token_id = created["id"].as_str().expect("missing id").to_string();
    assert!(raw_token.starts_with("agt_"));

    // List shows the prefix, never the raw token
    let list_res = client
        .get(format!("{base}/api/admin/agent-tokens"))
        .bearer_auth(&admin_tok)
        .send()
        .await
        .expect("list request failed");
    assert_eq!(list_res.status(), 200);
    let list_body: Value = list_res.json().await.expect("list json");
    let entry = list_body
        .as_array()
        .unwrap()
        .iter()
        .find(|item| item["id"] == token_id)
        .expect("created token missing from list");
    assert!(entry.get("token").is_none());
    assert!(entry["token_prefix"].as_str().unwrap().starts_with("agt_"));

    // The raw agent token authenticates as admin
    let overview_res = client
        .get(format!("{base}/api/admin/overview"))
        .bearer_auth(&raw_token)
        .send()
        .await
        .expect("overview request failed");
    assert_eq!(overview_res.status(), 200);

    // Revoke
    let revoke_res = client
        .delete(format!("{base}/api/admin/agent-tokens/{token_id}"))
        .bearer_auth(&admin_tok)
        .send()
        .await
        .expect("revoke request failed");
    assert_eq!(revoke_res.status(), 200);

    // The revoked token no longer authenticates
    let after_revoke_res = client
        .get(format!("{base}/api/admin/overview"))
        .bearer_auth(&raw_token)
        .send()
        .await
        .expect("post-revoke overview request failed");
    assert_eq!(after_revoke_res.status(), 401);
}
