// Unit tests for trust_service internal logic.
// This file is included as a module of trust_service via:
//   #[cfg(test)] #[path = "../../tests/trust_unit_tests.rs"] mod tests;
// so `super::` refers to trust_service's private items.

use super::{
    assess_human_activity, build_snapshot, default_trust_policy, level_policy_for_active_days,
    normalize_trust_policy, outbound_message_limit_enforced, rank_at_least,
    rank_policy_for_score, DEFAULT_DAILY_COUNTER_RETENTION_DAYS,
    DEFAULT_SCORE_EVENT_RETENTION_DAYS, FROZEN_RECOVERY_WINDOW_HOURS,
    SUSPICIOUS_ACTIVITY_FREEZE_THRESHOLD, SUSPICIOUS_NEW_DAY_ACTIVITY_WINDOW_MINUTES,
};
use crate::models::trust::TrustEnforcementConfig;
use crate::models::trust::{TrustPolicyConfig, UserTrustStats};
use chrono::{Duration, Utc};
use uuid::Uuid;

#[test]
fn default_policy_covers_expected_thresholds() {
    let policy = default_trust_policy();
    assert!(policy.enforcement.enabled);
    assert!(policy.enforcement.outbound_messages_enabled);
    assert_eq!(
        policy.daily_counter_retention_days,
        DEFAULT_DAILY_COUNTER_RETENTION_DAYS
    );
    assert_eq!(
        policy.score_event_retention_days,
        DEFAULT_SCORE_EVENT_RETENTION_DAYS
    );
    assert_eq!(
        policy.level_policies[0].daily_outbound_messages_limit,
        Some(50)
    );
    assert_eq!(policy.level_policies[3].level, 4);
    assert_eq!(policy.level_policies[3].min_active_days, 30);
    assert_eq!(rank_policy_for_score(&policy, 0).rank, "F");
    assert_eq!(rank_policy_for_score(&policy, 5_000).rank, "A");
}

#[test]
fn trust_policy_normalization_rejects_overlapping_ranges() {
    let mut policy = default_trust_policy();
    policy.level_policies[1].min_active_days = 6;
    let error = normalize_trust_policy(policy).expect_err("policy should be invalid");
    assert!(error
        .to_string()
        .contains("level policy day ranges must not overlap"));
}

#[test]
fn trust_policy_normalization_sorts_and_deduplicates() {
    let mut policy = default_trust_policy();
    policy.safe_attachment_types = vec![
        "image/png".into(),
        " image/png ".into(),
        "application/pdf".into(),
    ];
    policy.level_policies.reverse();
    let normalized = normalize_trust_policy(policy).expect("policy should normalize");
    assert_eq!(normalized.level_policies[0].level, 1);
    assert_eq!(
        normalized.safe_attachment_types,
        vec!["application/pdf".to_string(), "image/png".to_string()]
    );
}

#[test]
fn trust_policy_deserialization_defaults_enforcement_flags() {
    let policy = default_trust_policy();
    let mut raw = serde_json::to_value(policy).expect("policy should serialize");
    let object = raw
        .as_object_mut()
        .expect("trust policy should serialize into an object");
    object.remove("enforcement");
    object.remove("daily_counter_retention_days");
    object.remove("score_event_retention_days");

    let parsed: TrustPolicyConfig =
        serde_json::from_value(raw).expect("legacy trust policy should deserialize");

    assert!(parsed.enforcement.enabled);
    assert!(parsed.enforcement.outbound_messages_enabled);
    assert!(parsed.enforcement.friend_adds_enabled);
    assert!(parsed.enforcement.attachment_sends_enabled);
    assert_eq!(
        parsed.daily_counter_retention_days,
        DEFAULT_DAILY_COUNTER_RETENTION_DAYS
    );
    assert_eq!(
        parsed.score_event_retention_days,
        DEFAULT_SCORE_EVENT_RETENTION_DAYS
    );
}

#[test]
fn trust_policy_normalization_rejects_invalid_retention_settings() {
    let mut policy = default_trust_policy();
    policy.daily_counter_retention_days = 0;
    let error = normalize_trust_policy(policy).expect_err("policy should be invalid");
    assert!(error
        .to_string()
        .contains("daily_counter_retention_days must be > 0"));
}

#[test]
fn rank_threshold_comparison_is_ordered_correctly() {
    assert!(rank_at_least("E", "E"));
    assert!(rank_at_least("A", "E"));
    assert!(!rank_at_least("F", "E"));
    assert!(!rank_at_least("unknown", "E"));
}

#[test]
fn trust_snapshot_reports_when_message_limits_are_disabled() {
    let mut policy = default_trust_policy();
    policy.enforcement.outbound_messages_enabled = false;

    let stats = UserTrustStats {
        user_id: Uuid::new_v4(),
        active_days: 3,
        contribution_score: 0,
        derived_level: 1,
        derived_rank: "F".to_string(),
        last_active_day: None,
        last_human_activity_at: None,
        suspicious_activity_streak: 0,
        automation_review_state: "clear".to_string(),
        created_at: Utc::now(),
        updated_at: Utc::now(),
    };

    let snapshot = build_snapshot(&policy, &stats, 12, 3, 0, stats.derived_level.clamp(1, 10) as u8, &stats.derived_rank);

    assert!(!outbound_message_limit_enforced(&policy));
    assert!(!snapshot.daily_outbound_messages_enforced);
    assert_eq!(snapshot.daily_outbound_messages_limit, Some(50));
    assert_eq!(snapshot.daily_outbound_messages_sent, 12);
    assert_eq!(snapshot.daily_outbound_messages_remaining, Some(38));
    assert!(snapshot.daily_attachment_sends_enforced);
    assert_eq!(snapshot.daily_attachment_send_limit, Some(5));
    assert_eq!(snapshot.daily_attachment_sends_sent, 3);
    assert_eq!(snapshot.daily_attachment_sends_remaining, Some(2));
    assert!(snapshot
        .allowed_attachment_types
        .contains(&"image/gif".to_string()));
}

#[test]
fn trust_snapshot_applies_rank_multiplier_to_message_caps() {
    let policy = default_trust_policy();
    let now = Utc::now();
    let stats = UserTrustStats {
        user_id: Uuid::new_v4(),
        active_days: 20,
        contribution_score: 750,
        derived_level: 3,
        derived_rank: "D".to_string(),
        last_active_day: Some(now.date_naive()),
        last_human_activity_at: Some(now),
        suspicious_activity_streak: 0,
        automation_review_state: "clear".to_string(),
        created_at: now,
        updated_at: now,
    };

    let snapshot = build_snapshot(&policy, &stats, 40, 2, 0, stats.derived_level.clamp(1, 10) as u8, &stats.derived_rank);

    assert!(snapshot.daily_outbound_messages_enforced);
    assert_eq!(snapshot.daily_outbound_messages_limit, Some(240));
    assert_eq!(snapshot.daily_outbound_messages_remaining, Some(200));
    assert_eq!(snapshot.daily_attachment_send_limit, Some(10));
    assert_eq!(snapshot.daily_attachment_sends_remaining, Some(8));
    assert_eq!(snapshot.next_level_active_days, Some(30));
}

#[test]
fn trust_snapshot_uses_rank_overrides_for_unlimited_message_caps() {
    let policy = default_trust_policy();
    let now = Utc::now();
    let stats = UserTrustStats {
        user_id: Uuid::new_v4(),
        active_days: 5,
        contribution_score: 6_000,
        derived_level: 1,
        derived_rank: "A".to_string(),
        last_active_day: Some(now.date_naive()),
        last_human_activity_at: Some(now),
        suspicious_activity_streak: 0,
        automation_review_state: "clear".to_string(),
        created_at: now,
        updated_at: now,
    };

    let snapshot = build_snapshot(&policy, &stats, 99, 4, 0, stats.derived_level.clamp(1, 10) as u8, &stats.derived_rank);

    assert_eq!(snapshot.daily_outbound_messages_limit, None);
    assert_eq!(snapshot.daily_outbound_messages_remaining, None);
    assert_eq!(snapshot.daily_attachment_send_limit, Some(5));
    assert_eq!(snapshot.daily_attachment_sends_remaining, Some(1));
    assert_eq!(snapshot.rank, "A");
}

#[test]
fn suspicious_rollover_activity_is_challenged_and_not_counted() {
    let now = Utc::now();
    let today = now.date_naive();
    let stats = UserTrustStats {
        user_id: Uuid::new_v4(),
        active_days: 7,
        contribution_score: 0,
        derived_level: 2,
        derived_rank: "F".to_string(),
        last_active_day: Some(today.pred_opt().expect("previous day should exist")),
        last_human_activity_at: Some(now - Duration::minutes(5)),
        suspicious_activity_streak: 0,
        automation_review_state: "clear".to_string(),
        created_at: now,
        updated_at: now,
    };

    let assessment = assess_human_activity(&stats, today, now);

    assert!(!assessment.should_advance_active_day);
    assert_eq!(assessment.suspicious_activity_streak, 1);
    assert_eq!(assessment.automation_review_state, "challenged");
}

#[test]
fn legitimate_new_day_activity_reduces_suspicion_and_advances() {
    let now = Utc::now();
    let today = now.date_naive();
    let stats = UserTrustStats {
        user_id: Uuid::new_v4(),
        active_days: 7,
        contribution_score: 0,
        derived_level: 2,
        derived_rank: "F".to_string(),
        last_active_day: Some(today.pred_opt().expect("previous day should exist")),
        last_human_activity_at: Some(
            now - Duration::minutes(SUSPICIOUS_NEW_DAY_ACTIVITY_WINDOW_MINUTES + 5),
        ),
        suspicious_activity_streak: 1,
        automation_review_state: "challenged".to_string(),
        created_at: now,
        updated_at: now,
    };

    let assessment = assess_human_activity(&stats, today, now);

    assert!(assessment.should_advance_active_day);
    assert_eq!(assessment.suspicious_activity_streak, 0);
    assert_eq!(assessment.automation_review_state, "clear");
}

#[test]
fn repeated_suspicious_attempts_escalate_to_frozen() {
    let now = Utc::now();
    let today = now.date_naive();
    let stats = UserTrustStats {
        user_id: Uuid::new_v4(),
        active_days: 7,
        contribution_score: 0,
        derived_level: 2,
        derived_rank: "F".to_string(),
        last_active_day: Some(today.pred_opt().expect("previous day should exist")),
        last_human_activity_at: Some(now - Duration::minutes(5)),
        suspicious_activity_streak: SUSPICIOUS_ACTIVITY_FREEZE_THRESHOLD - 1,
        automation_review_state: "challenged".to_string(),
        created_at: now,
        updated_at: now,
    };

    let assessment = assess_human_activity(&stats, today, now);

    assert!(!assessment.should_advance_active_day);
    assert_eq!(
        assessment.suspicious_activity_streak,
        SUSPICIOUS_ACTIVITY_FREEZE_THRESHOLD
    );
    assert_eq!(assessment.automation_review_state, "frozen");
}

#[test]
fn frozen_accounts_recover_after_quiet_period() {
    let now = Utc::now();
    let today = now.date_naive();
    let stats = UserTrustStats {
        user_id: Uuid::new_v4(),
        active_days: 42,
        contribution_score: 0,
        derived_level: 4,
        derived_rank: "F".to_string(),
        last_active_day: Some(today.pred_opt().expect("previous day should exist")),
        last_human_activity_at: Some(now - Duration::hours(FROZEN_RECOVERY_WINDOW_HOURS + 1)),
        suspicious_activity_streak: SUSPICIOUS_ACTIVITY_FREEZE_THRESHOLD,
        automation_review_state: "frozen".to_string(),
        created_at: now,
        updated_at: now,
    };

    let assessment = assess_human_activity(&stats, today, now);

    assert!(assessment.should_advance_active_day);
    assert_eq!(assessment.suspicious_activity_streak, 0);
    assert_eq!(assessment.automation_review_state, "clear");
}

// ── challenge_state mapping ──────────────────────────────────────────────

fn make_stats_with_review_state(automation_review_state: &str) -> UserTrustStats {
    let now = Utc::now();
    UserTrustStats {
        user_id: Uuid::new_v4(),
        active_days: 1,
        contribution_score: 0,
        derived_level: 1,
        derived_rank: "F".to_string(),
        last_active_day: None,
        last_human_activity_at: None,
        suspicious_activity_streak: 0,
        automation_review_state: automation_review_state.to_string(),
        created_at: now,
        updated_at: now,
    }
}

#[test]
fn challenge_state_clear_maps_to_none() {
    let policy = default_trust_policy();
    let stats = make_stats_with_review_state("clear");
    let snapshot = build_snapshot(&policy, &stats, 0, 0, 0, stats.derived_level.clamp(1, 10) as u8, &stats.derived_rank);
    assert_eq!(snapshot.challenge_state, "none");
}

#[test]
fn challenge_state_challenged_maps_to_challenged() {
    let policy = default_trust_policy();
    let stats = make_stats_with_review_state("challenged");
    let snapshot = build_snapshot(&policy, &stats, 0, 0, 0, stats.derived_level.clamp(1, 10) as u8, &stats.derived_rank);
    assert_eq!(snapshot.challenge_state, "challenged");
}

#[test]
fn challenge_state_frozen_maps_to_frozen() {
    let policy = default_trust_policy();
    let stats = make_stats_with_review_state("frozen");
    let snapshot = build_snapshot(&policy, &stats, 0, 0, 0, stats.derived_level.clamp(1, 10) as u8, &stats.derived_rank);
    assert_eq!(snapshot.challenge_state, "frozen");
}

#[test]
fn challenge_state_unknown_value_maps_to_none() {
    let policy = default_trust_policy();
    let stats = make_stats_with_review_state("some_internal_state_unknown_to_client");
    let snapshot = build_snapshot(&policy, &stats, 0, 0, 0, stats.derived_level.clamp(1, 10) as u8, &stats.derived_rank);
    assert_eq!(snapshot.challenge_state, "none");
}

// ── rank score boundary transitions ─────────────────────────────────────

#[test]
fn rank_transitions_at_correct_score_boundaries() {
    let policy = default_trust_policy();

    // Default policy: F(0–99), E(100–499), D(500–999), C(1000–2499),
    //                 B(2500–4999), A(5000–9999), S(10000+)
    assert_eq!(rank_policy_for_score(&policy, 0).rank, "F");
    assert_eq!(rank_policy_for_score(&policy, 99).rank, "F");
    assert_eq!(rank_policy_for_score(&policy, 100).rank, "E");
    assert_eq!(rank_policy_for_score(&policy, 499).rank, "E");
    assert_eq!(rank_policy_for_score(&policy, 500).rank, "D");
    assert_eq!(rank_policy_for_score(&policy, 999).rank, "D");
    assert_eq!(rank_policy_for_score(&policy, 1_000).rank, "C");
    assert_eq!(rank_policy_for_score(&policy, 2_499).rank, "C");
    assert_eq!(rank_policy_for_score(&policy, 2_500).rank, "B");
    assert_eq!(rank_policy_for_score(&policy, 4_999).rank, "B");
    assert_eq!(rank_policy_for_score(&policy, 5_000).rank, "A");
    assert_eq!(rank_policy_for_score(&policy, 9_999).rank, "A");
    assert_eq!(rank_policy_for_score(&policy, 10_000).rank, "S");
    assert_eq!(rank_policy_for_score(&policy, 999_999).rank, "S");
}

// ── level active-day boundary transitions ────────────────────────────────

#[test]
fn level_transitions_at_correct_active_day_boundaries() {
    let policy = default_trust_policy();

    // Default policy levels: 1(0d), 2(7d), 3(14d), 4(30d), 5(60d), ...
    assert_eq!(level_policy_for_active_days(&policy, 0).level, 1);
    assert_eq!(level_policy_for_active_days(&policy, 6).level, 1);
    assert_eq!(level_policy_for_active_days(&policy, 7).level, 2);
    assert_eq!(level_policy_for_active_days(&policy, 13).level, 2);
    assert_eq!(level_policy_for_active_days(&policy, 14).level, 3);
    assert_eq!(level_policy_for_active_days(&policy, 29).level, 3);
    assert_eq!(level_policy_for_active_days(&policy, 30).level, 4);
    assert_eq!(level_policy_for_active_days(&policy, 59).level, 4);
    assert_eq!(level_policy_for_active_days(&policy, 60).level, 5);
}

// ── daily limit remaining is clamped to zero ─────────────────────────────

#[test]
fn daily_outbound_messages_remaining_clamps_at_zero_when_over_limit() {
    let policy = default_trust_policy();
    let now = Utc::now();
    let stats = UserTrustStats {
        user_id: Uuid::new_v4(),
        active_days: 1,
        contribution_score: 0,
        derived_level: 1,
        derived_rank: "F".to_string(),
        last_active_day: Some(now.date_naive()),
        last_human_activity_at: Some(now),
        suspicious_activity_streak: 0,
        automation_review_state: "clear".to_string(),
        created_at: now,
        updated_at: now,
    };

    // Level 1 limit is 50; send 9999 to simulate an over-limit state.
    let snapshot = build_snapshot(&policy, &stats, 9999, 0, 0, stats.derived_level.clamp(1, 10) as u8, &stats.derived_rank);
    assert_eq!(snapshot.daily_outbound_messages_remaining, Some(0));
    assert_eq!(snapshot.daily_outbound_messages_sent, 9999);
}

#[test]
fn daily_attachment_sends_remaining_clamps_at_zero_when_over_limit() {
    let policy = default_trust_policy();
    let now = Utc::now();
    let stats = UserTrustStats {
        user_id: Uuid::new_v4(),
        active_days: 1,
        contribution_score: 0,
        derived_level: 1,
        derived_rank: "F".to_string(),
        last_active_day: Some(now.date_naive()),
        last_human_activity_at: Some(now),
        suspicious_activity_streak: 0,
        automation_review_state: "clear".to_string(),
        created_at: now,
        updated_at: now,
    };

    // Level 1 attachment limit is 5; send 9999 to simulate an over-limit state.
    let snapshot = build_snapshot(&policy, &stats, 0, 9999, 0, stats.derived_level.clamp(1, 10) as u8, &stats.derived_rank);
    assert_eq!(snapshot.daily_attachment_sends_remaining, Some(0));
    assert_eq!(snapshot.daily_attachment_sends_sent, 9999);
}

#[test]
fn daily_friend_adds_remaining_clamps_at_zero_when_over_limit() {
    let policy = default_trust_policy();
    let now = Utc::now();
    let stats = UserTrustStats {
        user_id: Uuid::new_v4(),
        active_days: 1,
        contribution_score: 0,
        derived_level: 1,
        derived_rank: "F".to_string(),
        last_active_day: Some(now.date_naive()),
        last_human_activity_at: Some(now),
        suspicious_activity_streak: 0,
        automation_review_state: "clear".to_string(),
        created_at: now,
        updated_at: now,
    };

    let snapshot = build_snapshot(&policy, &stats, 0, 0, 9999, stats.derived_level.clamp(1, 10) as u8, &stats.derived_rank);
    assert_eq!(snapshot.daily_friend_adds_remaining, Some(0));
    assert_eq!(snapshot.daily_friend_adds_sent, 9999);
}

// ── level 1 attachment type restrictions ─────────────────────────────────

#[test]
fn level_1_attachment_types_are_restricted_to_safe_image_video() {
    let policy = default_trust_policy();
    let now = Utc::now();
    let stats = UserTrustStats {
        user_id: Uuid::new_v4(),
        active_days: 1,
        contribution_score: 0,
        derived_level: 1,
        derived_rank: "F".to_string(),
        last_active_day: Some(now.date_naive()),
        last_human_activity_at: Some(now),
        suspicious_activity_streak: 0,
        automation_review_state: "clear".to_string(),
        created_at: now,
        updated_at: now,
    };

    let snapshot = build_snapshot(&policy, &stats, 0, 0, 0, stats.derived_level.clamp(1, 10) as u8, &stats.derived_rank);
    assert!(!snapshot.allowed_attachment_types.is_empty());
    // Level 1 must not allow arbitrary document types.
    assert!(!snapshot
        .allowed_attachment_types
        .contains(&"application/zip".to_string()));
    assert!(!snapshot
        .allowed_attachment_types
        .contains(&"application/octet-stream".to_string()));
}

#[test]
fn higher_level_allows_more_attachment_types_than_level_1() {
    let policy = default_trust_policy();
    let now = Utc::now();

    let level1_stats = UserTrustStats {
        user_id: Uuid::new_v4(),
        active_days: 1,
        contribution_score: 0,
        derived_level: 1,
        derived_rank: "F".to_string(),
        last_active_day: None,
        last_human_activity_at: None,
        suspicious_activity_streak: 0,
        automation_review_state: "clear".to_string(),
        created_at: now,
        updated_at: now,
    };
    let level5_stats = UserTrustStats {
        user_id: Uuid::new_v4(),
        active_days: 60,
        contribution_score: 0,
        derived_level: 5,
        derived_rank: "F".to_string(),
        last_active_day: None,
        last_human_activity_at: None,
        suspicious_activity_streak: 0,
        automation_review_state: "clear".to_string(),
        created_at: now,
        updated_at: now,
    };

    let snapshot1 = build_snapshot(&policy, &level1_stats, 0, 0, 0, level1_stats.derived_level.clamp(1, 10) as u8, &level1_stats.derived_rank);
    let snapshot5 = build_snapshot(&policy, &level5_stats, 0, 0, 0, level5_stats.derived_level.clamp(1, 10) as u8, &level5_stats.derived_rank);

    assert!(
        snapshot5.allowed_attachment_types.len() >= snapshot1.allowed_attachment_types.len(),
        "higher levels should not restrict attachment types more than lower levels"
    );
}

// ── daily cap daily-reset behaviour (pure logic) ─────────────────────────

#[test]
fn daily_outbound_limit_is_none_for_unlimited_rank() {
    let policy = default_trust_policy();
    let now = Utc::now();
    // Rank S has no message limit in default policy.
    let stats = UserTrustStats {
        user_id: Uuid::new_v4(),
        active_days: 1,
        contribution_score: 12_000,
        derived_level: 1,
        derived_rank: "S".to_string(),
        last_active_day: Some(now.date_naive()),
        last_human_activity_at: Some(now),
        suspicious_activity_streak: 0,
        automation_review_state: "clear".to_string(),
        created_at: now,
        updated_at: now,
    };

    let snapshot = build_snapshot(&policy, &stats, 100, 0, 0, stats.derived_level.clamp(1, 10) as u8, &stats.derived_rank);
    assert_eq!(
        snapshot.daily_outbound_messages_limit, None,
        "S-rank should have no message limit"
    );
    assert_eq!(
        snapshot.daily_outbound_messages_remaining, None,
        "remaining should also be None when limit is unlimited"
    );
}

// ── rank_engine_enabled feature flag ─────────────────────────────────────

/// When `rank_engine_enabled = false`, rank perks (multipliers and
/// overrides_level_limits) must NOT affect limit calculations even for a
/// high-contribution-score user.  Level-based caps must still apply.
#[test]
fn rank_engine_disabled_ignores_rank_perks_for_limit_calculations() {
    let mut policy = default_trust_policy();
    policy.enforcement.rank_engine_enabled = false;

    let now = Utc::now();
    // User has enough score for rank S (unlimited messages in default policy).
    let stats = UserTrustStats {
        user_id: Uuid::new_v4(),
        active_days: 1,
        contribution_score: 12_000,
        derived_level: 1,
        derived_rank: "S".to_string(),
        last_active_day: Some(now.date_naive()),
        last_human_activity_at: Some(now),
        suspicious_activity_streak: 0,
        automation_review_state: "clear".to_string(),
        created_at: now,
        updated_at: now,
    };

    let snapshot = build_snapshot(&policy, &stats, 0, 0, 0, stats.derived_level.clamp(1, 10) as u8, &stats.derived_rank);

    // With rank_engine_enabled = false the rank perk (overrides_level_limits)
    // is suppressed, so level 1's cap should apply instead of unlimited.
    let level1_limit = policy
        .level_policies
        .iter()
        .find(|p| p.level == 1)
        .and_then(|p| p.daily_outbound_messages_limit);
    assert_eq!(
        snapshot.daily_outbound_messages_limit, level1_limit,
        "rank perks should be suppressed when rank_engine_enabled is false"
    );

    // Displayed rank is still the user's earned rank (not the neutral F).
    assert_eq!(
        snapshot.rank, "S",
        "snapshot rank should still reflect derived_rank even when engine is disabled"
    );
}

/// When `rank_engine_enabled = true` (default), rank perk for S-rank
/// (overrides_level_limits) gives unlimited messages.
#[test]
fn rank_engine_enabled_applies_rank_perks() {
    let policy = default_trust_policy();
    assert!(policy.enforcement.rank_engine_enabled, "should default to true");

    let now = Utc::now();
    let stats = UserTrustStats {
        user_id: Uuid::new_v4(),
        active_days: 1,
        contribution_score: 12_000,
        derived_level: 1,
        derived_rank: "S".to_string(),
        last_active_day: Some(now.date_naive()),
        last_human_activity_at: Some(now),
        suspicious_activity_streak: 0,
        automation_review_state: "clear".to_string(),
        created_at: now,
        updated_at: now,
    };

    let snapshot = build_snapshot(&policy, &stats, 0, 0, 0, stats.derived_level.clamp(1, 10) as u8, &stats.derived_rank);
    assert_eq!(
        snapshot.daily_outbound_messages_limit, None,
        "S-rank with rank engine enabled should get unlimited messages"
    );
}

// ── Phase 6: Abuse simulations ────────────────────────────────────────────────

/// Sleeper-account passive aging: an account whose `last_active_day` is already
/// today should never advance active_days again on the same UTC day, regardless
/// of how many times activity is triggered.  This exercises the idempotency
/// guard that prevents passive account aging.
#[test]
fn sleeper_account_cannot_earn_active_day_same_utc_day() {
    let now = Utc::now();
    let today = now.date_naive();
    let stats = UserTrustStats {
        user_id: Uuid::new_v4(),
        active_days: 90,
        contribution_score: 0,
        derived_level: 6,
        derived_rank: "F".to_string(),
        // Already counted today — simulates a dormant account that logged in once.
        last_active_day: Some(today),
        last_human_activity_at: Some(now - Duration::hours(1)),
        suspicious_activity_streak: 0,
        automation_review_state: "clear".to_string(),
        created_at: now,
        updated_at: now,
    };

    // Repeated activity calls on the same day must not advance active_days.
    let assessment = assess_human_activity(&stats, today, now);

    assert!(
        !assessment.should_advance_active_day,
        "active_day must not advance again on the same UTC day (sleeper-account guard)"
    );
    // No new suspicion — same-day repeat is normal, not suspicious.
    assert_eq!(
        assessment.suspicious_activity_streak, 0,
        "same-day repeat should not increment suspicious_activity_streak"
    );
    assert_eq!(
        assessment.automation_review_state, "clear",
        "same-day repeat should not flag clear account as suspicious"
    );
}

/// Low-trust vote farms: a granter at level 1 (below the level-4 threshold)
/// and rank F (below the rank-E threshold) is below the minimum eligibility
/// bar.  The rank_at_least helper must confirm this ordering correctly so that
/// the eligibility check `level >= 4 || rank_at_least(rank, "E")` returns false
/// for every rank below E.
#[test]
fn low_trust_granter_ranks_below_eligibility_threshold() {
    // Rank order is F < E < D < C < B < A < S.
    // The granter eligibility threshold is rank >= "E" (rank_order >= 1).
    // Only rank F is below it.
    let ineligible_ranks = ["F"];
    for rank in &ineligible_ranks {
        assert!(
            !rank_at_least(rank, "E"),
            "rank {rank} should be below eligibility threshold E"
        );
    }

    // Ranks at or above E (E, D, C, B, A, S) must pass.
    let eligible_ranks = ["E", "D", "C", "B", "A", "S"];
    for rank in &eligible_ranks {
        assert!(
            rank_at_least(rank, "E"),
            "rank {rank} should meet or exceed eligibility threshold E"
        );
    }
}

/// Scripted daily progression (bot farm simulation): an account that triggers
/// new-day activity suspiciously fast on consecutive days accumulates
/// suspicious_activity_streak until it reaches SUSPICIOUS_ACTIVITY_FREEZE_THRESHOLD
/// and gets frozen.  This test walks the account through the full escalation
/// path: clear → challenged → … → frozen.
#[test]
fn scripted_daily_progression_escalates_to_frozen() {
    let mut now = Utc::now();
    let mut today = now.date_naive();

    let mut stats = UserTrustStats {
        user_id: Uuid::new_v4(),
        active_days: 0,
        contribution_score: 0,
        derived_level: 1,
        derived_rank: "F".to_string(),
        last_active_day: None,
        last_human_activity_at: None,
        suspicious_activity_streak: 0,
        automation_review_state: "clear".to_string(),
        created_at: now,
        updated_at: now,
    };

    // Simulate bot: each "day" it sends activity within the suspicious window.
    for day in 0..SUSPICIOUS_ACTIVITY_FREEZE_THRESHOLD {
        // Advance simulated clock by one calendar day.
        now = now + Duration::days(1);
        today = now.date_naive();

        // Bot triggers activity immediately at rollover (< SUSPICIOUS_NEW_DAY_ACTIVITY_WINDOW_MINUTES).
        let last_human_activity =
            now - Duration::minutes(SUSPICIOUS_NEW_DAY_ACTIVITY_WINDOW_MINUTES - 1);

        stats.last_active_day = Some(today.pred_opt().expect("prev day"));
        stats.last_human_activity_at = Some(last_human_activity);

        let assessment = assess_human_activity(&stats, today, now);

        // After each suspicious attempt, streak should grow.
        let expected_streak = day + 1;
        assert_eq!(
            assessment.suspicious_activity_streak, expected_streak,
            "day {day}: streak should be {expected_streak}"
        );
        assert!(
            !assessment.should_advance_active_day,
            "day {day}: suspicious attempt must not advance active_day"
        );

        // Apply assessment back to stats for the next iteration.
        stats.suspicious_activity_streak = assessment.suspicious_activity_streak;
        stats.automation_review_state = assessment.automation_review_state.to_string();
    }

    // After SUSPICIOUS_ACTIVITY_FREEZE_THRESHOLD suspicious days, account must be frozen.
    assert_eq!(
        stats.automation_review_state, "frozen",
        "account must be frozen after {} consecutive suspicious attempts",
        SUSPICIOUS_ACTIVITY_FREEZE_THRESHOLD
    );
}

/// A frozen account that attempts more activity within the recovery window
/// must remain frozen; it cannot self-clear by simply retrying.
#[test]
fn frozen_account_cannot_self_clear_within_recovery_window() {
    let now = Utc::now();
    let today = now.date_naive();
    let stats = UserTrustStats {
        user_id: Uuid::new_v4(),
        active_days: 10,
        contribution_score: 0,
        derived_level: 2,
        derived_rank: "F".to_string(),
        last_active_day: Some(today.pred_opt().expect("prev day")),
        // Last activity was LESS than the recovery window ago — not yet eligible to clear.
        last_human_activity_at: Some(now - Duration::hours(FROZEN_RECOVERY_WINDOW_HOURS - 1)),
        suspicious_activity_streak: SUSPICIOUS_ACTIVITY_FREEZE_THRESHOLD,
        automation_review_state: "frozen".to_string(),
        created_at: now,
        updated_at: now,
    };

    let assessment = assess_human_activity(&stats, today, now);

    assert!(
        !assessment.should_advance_active_day,
        "frozen account within recovery window must not advance active_day"
    );
    assert_eq!(
        assessment.automation_review_state, "frozen",
        "automation_review_state must stay 'frozen' within recovery window"
    );
}

/// A config with `rank_engine_enabled` missing deserializes to `true` (backwards-compatible).
#[test]
fn rank_engine_enabled_defaults_to_true_when_absent_from_config() {
    let enforcement_json = serde_json::json!({
        "enabled": true,
        "outbound_messages_enabled": true,
        "friend_adds_enabled": true,
        "attachment_sends_enabled": true
        // rank_engine_enabled is deliberately absent
    });
    let enforcement: TrustEnforcementConfig =
        serde_json::from_value(enforcement_json).expect("should deserialize");
    assert!(
        enforcement.rank_engine_enabled,
        "rank_engine_enabled must default to true for backwards compatibility"
    );
}
