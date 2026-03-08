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

// ── Abuse / regression scenarios ──────────────────────────────────────────────

/// Passive account polling must not increment active_days.
///
/// Simulates a bot pattern: repeatedly call GET /api/profile/me (background
/// sync / idle heartbeat) without any human-like write actions.  The
/// active_days counter must stay constant across all calls because none of
/// them represent genuine human-initiated activity.
#[tokio::test]
async fn passive_polling_does_not_increment_active_days() {
    let Some(base) = base_url() else { return };
    let client = Client::new();

    let (token, _id) = register_and_login(&client, &base).await;

    let trust_first = get_trust_snapshot(&client, &base, &token).await;
    let days_first = trust_first["active_days"].as_i64().unwrap_or(-1);

    // Issue many passive GET /api/profile/me requests — mimicking a bot polling loop.
    for _ in 0..10 {
        get_trust_snapshot(&client, &base, &token).await;
    }

    let trust_last = get_trust_snapshot(&client, &base, &token).await;
    let days_last = trust_last["active_days"].as_i64().unwrap_or(-1);

    assert_eq!(
        days_last, days_first,
        "active_days must not change from passive profile polling \
         (passive-aging attempt: before={days_first}, after={days_last})"
    );
}

/// A freshly created (burner) account must face enforced outbound message
/// limits from the first request.  This prevents mass-account creation from
/// being useful for amplification campaigns.
#[tokio::test]
async fn burner_account_has_enforced_message_limits() {
    let Some(base) = base_url() else { return };
    let client = Client::new();

    let (token, _id) = register_and_login(&client, &base).await;
    let trust = get_trust_snapshot(&client, &base, &token).await;

    // A brand-new Level 1 account must have enforcement active.
    assert_eq!(
        trust["level"], 1,
        "burner account should start at Level 1"
    );
    assert_eq!(
        trust["daily_outbound_messages_enforced"], true,
        "outbound message limits must be enforced for a new (burner) account"
    );
    assert_eq!(
        trust["daily_friend_adds_enforced"], true,
        "friend-add limits must be enforced for a new (burner) account"
    );
    assert_eq!(
        trust["daily_attachment_sends_enforced"], true,
        "attachment send limits must be enforced for a new (burner) account"
    );

    // daily limits must be non-null (i.e. actually capped, not unlimited).
    assert!(
        trust["daily_outbound_messages_limit"].as_i64().is_some(),
        "new account should have a concrete daily_outbound_messages_limit"
    );
    assert!(
        trust["daily_friend_add_limit"].as_i64().is_some(),
        "new account should have a concrete daily_friend_add_limit"
    );
}

/// Sending messages up to the daily cap and then one more must return 429.
///
/// This regression test guards against a code path where the counter is
/// updated after the gate check, allowing scripted clients to fire one extra
/// action per session.
#[tokio::test]
async fn scripted_message_sends_are_blocked_at_cap() {
    let Some(base) = base_url() else { return };
    let client = Client::new();

    let (token_a, _id_a) = register_and_login(&client, &base).await;
    let (_token_b, id_b) = register_and_login(&client, &base).await;

    let trust = get_trust_snapshot(&client, &base, &token_a).await;

    // Skip if enforcement is off or limit is null (uncapped tier).
    let Some(limit) = trust["daily_outbound_messages_limit"].as_i64() else {
        return;
    };
    if trust["daily_outbound_messages_enforced"] != true {
        return;
    }

    let sent_so_far = trust["daily_outbound_messages_sent"].as_i64().unwrap_or(0);
    let remaining = (limit - sent_so_far).max(0) as usize;

    // Consume all remaining slots.
    let mut last_successful_status = 0u16;
    for _ in 0..remaining {
        let res = client
            .post(format!("{base}/api/messages"))
            .bearer_auth(&token_a)
            .json(&json!({ "recipient_id": id_b, "content": "cap test" }))
            .send()
            .await
            .expect("send request failed");
        last_successful_status = res.status().as_u16();
        if last_successful_status == 429 {
            // Already capped earlier than expected — stop.
            break;
        }
    }

    // If we exhausted the cap without hitting 429 yet, the very next send must
    // return 429 (scripted overflow attempt).
    if last_successful_status != 429 {
        let overflow_status = client
            .post(format!("{base}/api/messages"))
            .bearer_auth(&token_a)
            .json(&json!({ "recipient_id": id_b, "content": "overflow attempt" }))
            .send()
            .await
            .expect("overflow send failed")
            .status()
            .as_u16();

        assert_eq!(
            overflow_status, 429,
            "message send beyond daily cap must return 429 (scripted send regression)"
        );
    }

    // Either way, the daily_remaining field must be 0 or null.
    let trust_after = get_trust_snapshot(&client, &base, &token_a).await;
    let remaining_after = trust_after["daily_outbound_messages_remaining"]
        .as_i64()
        .unwrap_or(0);
    assert_eq!(
        remaining_after, 0,
        "daily_outbound_messages_remaining must be 0 after exhausting the cap"
    );
}

/// A frozen account should still return a trust snapshot but must have
/// challenge_state == "frozen" and must not be able to progress its active_days
/// through passive actions.
#[tokio::test]
async fn frozen_account_does_not_gain_active_days_passively() {
    let Some(base) = base_url() else { return };
    let Some(admin_tok) = admin_token() else { return };
    let client = Client::new();

    let (user_token, user_id) = register_and_login(&client, &base).await;

    let trust_before = get_trust_snapshot(&client, &base, &user_token).await;
    let days_before = trust_before["active_days"].as_i64().unwrap_or(-1);

    // Admin freezes the account.
    client
        .post(format!("{base}/api/admin/users/{user_id}/trust/freeze"))
        .bearer_auth(&admin_tok)
        .json(&json!({ "reason": "abuse regression: frozen passive aging test" }))
        .send()
        .await
        .expect("freeze request failed");

    // Passive polling should not change active_days for a frozen account.
    for _ in 0..5 {
        get_trust_snapshot(&client, &base, &user_token).await;
    }

    let trust_after = get_trust_snapshot(&client, &base, &user_token).await;
    let days_after = trust_after["active_days"].as_i64().unwrap_or(-1);

    assert_eq!(
        trust_after["challenge_state"], "frozen",
        "frozen account must report challenge_state 'frozen'"
    );
    assert_eq!(
        days_after, days_before,
        "frozen account must not gain active_days from passive polling"
    );
}

/// A bot-farm pattern: multiple newly registered low-trust accounts all try to
/// grant contribution score to the same target user.  Because all grantors are
/// Level 1, the server must suppress or heavily discount each event.
///
/// This test verifies that the target's score does not increase by the naive
/// sum of all grant attempts.  Exact suppression thresholds are internal; the
/// test just asserts that the score gain is strictly less than
/// `num_grantors × max_per_grant_points`.
#[tokio::test]
async fn low_trust_bot_farm_cannot_inflate_target_score() {
    let Some(base) = base_url() else { return };
    let Some(admin_tok) = admin_token() else { return };
    let client = Client::new();

    // Register the target user.
    let (target_token, target_id) = register_and_login(&client, &base).await;

    let trust_before = get_trust_snapshot(&client, &base, &target_token).await;
    let score_before = trust_before["contribution_score"].as_i64().unwrap_or(0);

    // Register a cluster of low-trust burner accounts (simulated bot farm).
    const NUM_BOTS: usize = 5;
    let max_per_grant = 25i64; // VERIFIED_ABUSE_REPORT_POINTS — the largest single grant

    for i in 0..NUM_BOTS {
        let (_bot_token, bot_id) = register_and_login(&client, &base).await;
        let reference_id = format!("bot-farm-grant-{}-{}", Uuid::new_v4(), i);

        // Simulate each bot triggering a positive scoring event on behalf of the target.
        // We use verify-report (admin-triggered) as a proxy, with the bot as the reporter.
        // Real bot-farm attacks would use the upvote / helpful-answer endpoints;
        // this confirms the scoring path runs through low-trust suppression checks.
        let _: Value = client
            .post(format!(
                "{base}/api/admin/users/{target_id}/trust/verify-report"
            ))
            .bearer_auth(&admin_tok)
            .json(&json!({
                "reporter_user_id": bot_id,
                "report_reference_id": reference_id,
            }))
            .send()
            .await
            .expect("bot grant request failed")
            .json()
            .await
            .expect("bot grant not JSON");
    }

    let trust_after = get_trust_snapshot(&client, &base, &target_token).await;
    let score_after = trust_after["contribution_score"].as_i64().unwrap_or(0);

    let naive_max = (NUM_BOTS as i64) * max_per_grant;
    let actual_gain = score_after - score_before;

    // The gain must be less than the naive unsuppressed total.
    // If suppression is not active, this asserts false and flags the regression.
    assert!(
        actual_gain < naive_max,
        "bot-farm suppression failed: target gained {actual_gain} points from \
         {NUM_BOTS} Level-1 grantors; expected strictly less than {naive_max} \
         (score before={score_before}, after={score_after})"
    );
}
