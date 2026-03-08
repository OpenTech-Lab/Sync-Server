/// Integration tests for the trust / gamification system.
///
/// These tests require a running server and real database.
/// Set `TEST_BASE_URL` (e.g. `http://localhost:8080`) to run them.
/// If the env var is absent the tests are skipped.
///
/// Admin-gated tests additionally require `TEST_ADMIN_TOKEN` — a JWT whose
/// `role` claim equals "admin".  If absent, those tests are also skipped.
use reqwest::Client;
use serde_json::{json, Value};
use uuid::Uuid;

fn base_url() -> Option<String> {
    std::env::var("TEST_BASE_URL").ok()
}

fn admin_token() -> Option<String> {
    std::env::var("TEST_ADMIN_TOKEN").ok()
}

fn unique_email() -> String {
    format!("trust-test-{}@example.com", Uuid::new_v4())
}

/// Helper: register a new user and return (access_token, user_id).
async fn register_and_login(client: &Client, base: &str) -> (String, String) {
    let email = unique_email();
    let password = "trust_test_secure!";

    let reg: Value = client
        .post(format!("{base}/auth/register"))
        .json(&json!({ "email": email, "password": password }))
        .send()
        .await
        .expect("register request failed")
        .json()
        .await
        .expect("register response not JSON");

    let user_id = reg["id"].as_str().expect("register: missing id").to_owned();

    let login: Value = client
        .post(format!("{base}/auth/login"))
        .json(&json!({ "email": email, "password": password }))
        .send()
        .await
        .expect("login request failed")
        .json()
        .await
        .expect("login response not JSON");

    let token = login["access_token"]
        .as_str()
        .expect("login: missing access_token")
        .to_owned();

    (token, user_id)
}

/// Helper: fetch the trust snapshot from GET /api/profile/me.
async fn get_trust_snapshot(client: &Client, base: &str, token: &str) -> Value {
    let profile: Value = client
        .get(format!("{base}/api/profile/me"))
        .bearer_auth(token)
        .send()
        .await
        .expect("GET /api/profile/me failed")
        .json()
        .await
        .expect("profile response not JSON");

    profile["trust"].clone()
}

// ── Snapshot shape tests ──────────────────────────────────────────────────────

/// A freshly registered user should have a complete trust snapshot with
/// level 1, rank F, and challenge_state "none".
#[tokio::test]
async fn trust_snapshot_present_on_profile() {
    let Some(base) = base_url() else { return };
    let client = Client::new();

    let (token, _id) = register_and_login(&client, &base).await;
    let trust = get_trust_snapshot(&client, &base, &token).await;

    assert!(
        !trust.is_null(),
        "trust field should be present on /api/profile/me"
    );
    assert_eq!(trust["level"], 1, "new user should start at level 1");
    assert_eq!(trust["rank"], "F", "new user should start at rank F");
    assert_eq!(
        trust["challenge_state"], "none",
        "new user should have no active challenge"
    );
    assert_eq!(
        trust["contribution_score"], 0,
        "new user should start with zero contribution score"
    );
    assert!(
        trust["active_days"].as_i64().is_some(),
        "active_days should be an integer"
    );
    assert!(
        trust["level_progress_percent"].as_i64().is_some(),
        "level_progress_percent should be present"
    );
}

/// The snapshot must include daily limit fields for all enforced action types.
#[tokio::test]
async fn trust_snapshot_includes_daily_limit_fields() {
    let Some(base) = base_url() else { return };
    let client = Client::new();

    let (token, _id) = register_and_login(&client, &base).await;
    let trust = get_trust_snapshot(&client, &base, &token).await;

    // Outbound message fields
    assert!(
        trust.get("daily_outbound_messages_enforced").is_some(),
        "missing daily_outbound_messages_enforced"
    );
    assert!(
        trust.get("daily_outbound_messages_sent").is_some(),
        "missing daily_outbound_messages_sent"
    );

    // Friend add fields
    assert!(
        trust.get("daily_friend_adds_enforced").is_some(),
        "missing daily_friend_adds_enforced"
    );
    assert!(
        trust.get("daily_friend_adds_sent").is_some(),
        "missing daily_friend_adds_sent"
    );

    // Attachment fields
    assert!(
        trust.get("daily_attachment_sends_enforced").is_some(),
        "missing daily_attachment_sends_enforced"
    );
    assert!(
        trust.get("daily_attachment_sends_sent").is_some(),
        "missing daily_attachment_sends_sent"
    );

    // Allowed attachment types list
    assert!(
        trust["allowed_attachment_types"].is_array(),
        "allowed_attachment_types should be an array"
    );
}

// ── Message send decrements remaining counter ─────────────────────────────────

/// Sending a message should increment daily_outbound_messages_sent by 1 and
/// decrement daily_outbound_messages_remaining by 1 (when a limit is enforced).
#[tokio::test]
async fn send_message_decrements_daily_messages_remaining() {
    let Some(base) = base_url() else { return };
    let client = Client::new();

    let (token_a, _id_a) = register_and_login(&client, &base).await;
    let (_token_b, id_b) = register_and_login(&client, &base).await;

    let trust_before = get_trust_snapshot(&client, &base, &token_a).await;
    let sent_before = trust_before["daily_outbound_messages_sent"]
        .as_i64()
        .unwrap_or(0);
    let remaining_before = trust_before["daily_outbound_messages_remaining"].as_i64();

    let res = client
        .post(format!("{base}/api/messages"))
        .bearer_auth(&token_a)
        .json(&json!({ "recipient_id": id_b, "content": "trust test ping" }))
        .send()
        .await
        .expect("POST /api/messages failed");

    // Message should succeed (201) or be limit-blocked (429).
    // Either way the counter should have changed.
    let status = res.status().as_u16();
    assert!(
        status == 201 || status == 429,
        "unexpected status {status}"
    );

    if status == 201 {
        let trust_after = get_trust_snapshot(&client, &base, &token_a).await;
        let sent_after = trust_after["daily_outbound_messages_sent"]
            .as_i64()
            .unwrap_or(0);

        assert_eq!(
            sent_after,
            sent_before + 1,
            "daily_outbound_messages_sent should increase by 1 after a successful send"
        );

        // Only check remaining if the field was present and a limit is active.
        if let (Some(rem_before), Some(rem_after)) = (
            remaining_before,
            trust_after["daily_outbound_messages_remaining"].as_i64(),
        ) {
            assert_eq!(
                rem_after,
                rem_before - 1,
                "daily_outbound_messages_remaining should decrease by 1"
            );
        }
    }
}

// ── Admin trust endpoints ─────────────────────────────────────────────────────

/// Verifying an abuse report should award VERIFIED_ABUSE_REPORT_POINTS (+25)
/// to the reporter's contribution_score.
#[tokio::test]
async fn verify_report_increases_reporter_score() {
    let Some(base) = base_url() else { return };
    let Some(admin_tok) = admin_token() else { return };
    let client = Client::new();

    let (reporter_token, reporter_id) = register_and_login(&client, &base).await;
    let (_subject_token, subject_id) = register_and_login(&client, &base).await;
    let reference_id = format!("report-{}", Uuid::new_v4());

    let trust_before = get_trust_snapshot(&client, &base, &reporter_token).await;
    let score_before = trust_before["contribution_score"].as_i64().unwrap_or(0);

    let res: Value = client
        .post(format!(
            "{base}/api/admin/users/{subject_id}/trust/verify-report"
        ))
        .bearer_auth(&admin_tok)
        .json(&json!({
            "reporter_user_id": reporter_id,
            "report_reference_id": reference_id,
        }))
        .send()
        .await
        .expect("verify-report request failed")
        .json()
        .await
        .expect("verify-report response not JSON");

    // applied_delta should equal VERIFIED_ABUSE_REPORT_POINTS (25).
    let applied_delta = res["applied_delta"].as_i64().unwrap_or(-1);
    assert_eq!(
        applied_delta, 25,
        "verify-report should award +25 points; got {applied_delta}"
    );

    let trust_after = get_trust_snapshot(&client, &base, &reporter_token).await;
    let score_after = trust_after["contribution_score"].as_i64().unwrap_or(0);

    assert_eq!(
        score_after,
        score_before + 25,
        "contribution_score should increase by 25 after verify-report"
    );
}

/// Dismissing an abuse report should penalise the reporter by
/// FALSE_REPORT_PENALTY_POINTS (-10).
#[tokio::test]
async fn dismiss_report_penalises_reporter_score() {
    let Some(base) = base_url() else { return };
    let Some(admin_tok) = admin_token() else { return };
    let client = Client::new();

    let (reporter_token, reporter_id) = register_and_login(&client, &base).await;
    let (_subject_token, subject_id) = register_and_login(&client, &base).await;
    let reference_id = format!("report-{}", Uuid::new_v4());

    let trust_before = get_trust_snapshot(&client, &base, &reporter_token).await;
    let score_before = trust_before["contribution_score"].as_i64().unwrap_or(0);

    let res: Value = client
        .post(format!(
            "{base}/api/admin/users/{subject_id}/trust/dismiss-report"
        ))
        .bearer_auth(&admin_tok)
        .json(&json!({
            "reporter_user_id": reporter_id,
            "report_reference_id": reference_id,
        }))
        .send()
        .await
        .expect("dismiss-report request failed")
        .json()
        .await
        .expect("dismiss-report response not JSON");

    // applied_delta should be FALSE_REPORT_PENALTY_POINTS (-10).
    let applied_delta = res["applied_delta"].as_i64().unwrap_or(0);
    assert_eq!(
        applied_delta, -10,
        "dismiss-report should apply -10 delta; got {applied_delta}"
    );

    let trust_after = get_trust_snapshot(&client, &base, &reporter_token).await;
    let score_after = trust_after["contribution_score"].as_i64().unwrap_or(0);

    assert_eq!(
        score_after,
        score_before - 10,
        "contribution_score should decrease by 10 after dismiss-report"
    );
}

/// Calling verify-report twice with the same reference_id must be idempotent:
/// the second call should return applied_delta = 0 and duplicate = true.
#[tokio::test]
async fn verify_report_is_idempotent_for_same_reference_id() {
    let Some(base) = base_url() else { return };
    let Some(admin_tok) = admin_token() else { return };
    let client = Client::new();

    let (_reporter_token, reporter_id) = register_and_login(&client, &base).await;
    let (_subject_token, subject_id) = register_and_login(&client, &base).await;
    let reference_id = format!("report-{}", Uuid::new_v4());

    let payload = json!({
        "reporter_user_id": reporter_id,
        "report_reference_id": reference_id,
    });

    // First call — should apply +25.
    let first: Value = client
        .post(format!(
            "{base}/api/admin/users/{subject_id}/trust/verify-report"
        ))
        .bearer_auth(&admin_tok)
        .json(&payload)
        .send()
        .await
        .expect("first verify-report failed")
        .json()
        .await
        .expect("first verify-report not JSON");

    assert_eq!(first["applied_delta"], 25, "first call should apply +25");
    assert_eq!(
        first["duplicate"], false,
        "first call should not be a duplicate"
    );

    // Second call — same reference_id, must be idempotent.
    let second: Value = client
        .post(format!(
            "{base}/api/admin/users/{subject_id}/trust/verify-report"
        ))
        .bearer_auth(&admin_tok)
        .json(&payload)
        .send()
        .await
        .expect("second verify-report failed")
        .json()
        .await
        .expect("second verify-report not JSON");

    assert_eq!(
        second["applied_delta"], 0,
        "second call with same reference_id should have applied_delta = 0"
    );
    assert_eq!(
        second["duplicate"], true,
        "second call with same reference_id should be marked duplicate"
    );
}

/// Admin freeze should set the user's challenge_state to "frozen".
#[tokio::test]
async fn freeze_sets_challenge_state_frozen() {
    let Some(base) = base_url() else { return };
    let Some(admin_tok) = admin_token() else { return };
    let client = Client::new();

    let (user_token, user_id) = register_and_login(&client, &base).await;

    // Confirm starting state.
    let trust_before = get_trust_snapshot(&client, &base, &user_token).await;
    assert_eq!(
        trust_before["challenge_state"], "none",
        "user should start with challenge_state 'none'"
    );

    // Admin freeze.
    let freeze_res = client
        .post(format!("{base}/api/admin/users/{user_id}/trust/freeze"))
        .bearer_auth(&admin_tok)
        .json(&json!({ "reason": "integration test freeze" }))
        .send()
        .await
        .expect("freeze request failed");

    assert_eq!(
        freeze_res.status().as_u16(),
        200,
        "freeze should return 200"
    );

    // User snapshot should now show "frozen".
    let trust_after = get_trust_snapshot(&client, &base, &user_token).await;
    assert_eq!(
        trust_after["challenge_state"], "frozen",
        "challenge_state should be 'frozen' after admin freeze"
    );
}

/// Admin unfreeze should clear challenge_state back to "none".
#[tokio::test]
async fn unfreeze_clears_challenge_state() {
    let Some(base) = base_url() else { return };
    let Some(admin_tok) = admin_token() else { return };
    let client = Client::new();

    let (user_token, user_id) = register_and_login(&client, &base).await;

    // Freeze first.
    client
        .post(format!("{base}/api/admin/users/{user_id}/trust/freeze"))
        .bearer_auth(&admin_tok)
        .json(&json!({ "reason": "setup for unfreeze test" }))
        .send()
        .await
        .expect("freeze request failed");

    let trust_frozen = get_trust_snapshot(&client, &base, &user_token).await;
    assert_eq!(
        trust_frozen["challenge_state"], "frozen",
        "setup: expected 'frozen' after freeze"
    );

    // Now unfreeze.
    let unfreeze_res = client
        .post(format!("{base}/api/admin/users/{user_id}/trust/unfreeze"))
        .bearer_auth(&admin_tok)
        .json(&json!({ "reason": "integration test unfreeze" }))
        .send()
        .await
        .expect("unfreeze request failed");

    assert_eq!(
        unfreeze_res.status().as_u16(),
        200,
        "unfreeze should return 200"
    );

    let trust_after = get_trust_snapshot(&client, &base, &user_token).await;
    assert_eq!(
        trust_after["challenge_state"], "none",
        "challenge_state should return to 'none' after admin unfreeze"
    );
}

// ── Non-admin rejection ───────────────────────────────────────────────────────

/// A regular (non-admin) user must receive 403 when calling admin trust endpoints.
#[tokio::test]
async fn trust_admin_endpoints_require_admin_role() {
    let Some(base) = base_url() else { return };
    let client = Client::new();

    let (token, user_id) = register_and_login(&client, &base).await;
    let (_other_token, other_id) = register_and_login(&client, &base).await;

    let endpoints: &[(&str, serde_json::Value)] = &[
        (
            "verify-report",
            json!({ "reporter_user_id": other_id, "report_reference_id": "r1" }),
        ),
        (
            "dismiss-report",
            json!({ "reporter_user_id": other_id, "report_reference_id": "r2" }),
        ),
        ("freeze", json!({ "reason": "test" })),
        ("unfreeze", json!({ "reason": "test" })),
    ];

    for (action, body) in endpoints {
        let status = client
            .post(format!("{base}/api/admin/users/{user_id}/trust/{action}"))
            .bearer_auth(&token)
            .json(body)
            .send()
            .await
            .expect("request failed")
            .status()
            .as_u16();

        assert_eq!(
            status, 403,
            "non-admin calling /trust/{action} should get 403, got {status}"
        );
    }
}
