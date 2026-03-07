use chrono::{DateTime, Duration, NaiveDate, Utc};
use diesel::prelude::*;
use diesel::Connection;
use uuid::Uuid;

use crate::db::Pool;
use crate::errors::AppError;
use crate::models::message::Message;
use crate::models::trust::{
    NewDailyActionCounter, NewUserTrustStats, TrustSnapshot, UserTrustStats,
};
use crate::schema::{daily_action_counters, user_trust_stats};
use crate::services::message_service;

const ACTION_OUTBOUND_MESSAGE: &str = "outbound_message";
const DEFAULT_AUTOMATION_REVIEW_STATE: &str = "clear";

#[derive(Debug, Clone)]
pub enum SendMessageWithTrustResult {
    Sent {
        message: Message,
    },
    Limited {
        trust: TrustSnapshot,
        retry_after_seconds: i64,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct LevelBand {
    level: u8,
    min_active_days: i32,
    next_level_active_days: Option<i32>,
    daily_outbound_messages_limit: Option<i32>,
}

pub fn get_trust_snapshot(pool: &Pool, user_id: Uuid) -> Result<TrustSnapshot, AppError> {
    let mut conn = pool.get()?;
    let today = Utc::now().date_naive();
    let stats = user_trust_stats::table
        .find(user_id)
        .select(UserTrustStats::as_select())
        .first::<UserTrustStats>(&mut conn)
        .optional()?;

    let active_days = stats.as_ref().map(|value| value.active_days).unwrap_or(0);
    let contribution_score = stats
        .as_ref()
        .map(|value| value.contribution_score)
        .unwrap_or(0);
    let sent_today = daily_message_count(&mut conn, user_id, today)?;

    Ok(build_snapshot(active_days, contribution_score, sent_today))
}

pub fn record_human_activity(pool: &Pool, user_id: Uuid) -> Result<(), AppError> {
    let mut conn = pool.get()?;
    let today = Utc::now().date_naive();
    let now = Utc::now();
    conn.transaction(|conn| {
        let stats = ensure_user_trust_stats(conn, user_id)?;
        let _ = advance_active_day_if_needed(conn, stats, today, now)?;
        Ok(())
    })
}

pub fn send_message_with_trust(
    pool: &Pool,
    sender_id: Uuid,
    recipient_id: Uuid,
    content: String,
) -> Result<SendMessageWithTrustResult, AppError> {
    let mut conn = pool.get()?;
    let now = Utc::now();
    let today = now.date_naive();

    conn.transaction(|conn| {
        let stats = ensure_user_trust_stats(conn, sender_id)?;
        let stats = advance_active_day_if_needed(conn, stats, today, now)?;
        let sent_today = daily_message_count(conn, sender_id, today)?;
        let band = level_band_for_active_days(stats.active_days);

        if let Some(limit) = band.daily_outbound_messages_limit {
            if sent_today >= limit {
                return Ok(SendMessageWithTrustResult::Limited {
                    trust: build_snapshot(stats.active_days, stats.contribution_score, sent_today),
                    retry_after_seconds: seconds_until_next_utc_day(now),
                });
            }
        }

        let message =
            message_service::insert_message_conn(conn, sender_id, recipient_id, &content)?;
        increment_daily_counter(conn, sender_id, ACTION_OUTBOUND_MESSAGE, today)?;

        Ok(SendMessageWithTrustResult::Sent { message })
    })
}

fn build_snapshot(
    active_days: i32,
    contribution_score: i32,
    daily_outbound_messages_sent: i32,
) -> TrustSnapshot {
    let level_band = level_band_for_active_days(active_days);
    let daily_outbound_messages_remaining = level_band
        .daily_outbound_messages_limit
        .map(|limit| (limit - daily_outbound_messages_sent).max(0));

    TrustSnapshot {
        active_days,
        level: level_band.level,
        contribution_score,
        rank: rank_for_contribution_score(contribution_score).to_string(),
        next_level_active_days: level_band.next_level_active_days,
        level_progress_percent: level_progress_percent(active_days, level_band),
        daily_outbound_messages_limit: level_band.daily_outbound_messages_limit,
        daily_outbound_messages_sent,
        daily_outbound_messages_remaining,
    }
}

fn ensure_user_trust_stats(
    conn: &mut diesel::PgConnection,
    user_id: Uuid,
) -> Result<UserTrustStats, AppError> {
    diesel::insert_into(user_trust_stats::table)
        .values(&NewUserTrustStats {
            user_id,
            active_days: 0,
            contribution_score: 0,
            last_active_day: None,
            automation_review_state: DEFAULT_AUTOMATION_REVIEW_STATE.to_string(),
        })
        .on_conflict(user_trust_stats::user_id)
        .do_nothing()
        .execute(conn)?;

    user_trust_stats::table
        .find(user_id)
        .select(UserTrustStats::as_select())
        .first::<UserTrustStats>(conn)
        .map_err(AppError::from)
}

fn advance_active_day_if_needed(
    conn: &mut diesel::PgConnection,
    stats: UserTrustStats,
    today: NaiveDate,
    now: DateTime<Utc>,
) -> Result<UserTrustStats, AppError> {
    if stats.last_active_day == Some(today) {
        return Ok(stats);
    }

    diesel::update(user_trust_stats::table.find(stats.user_id))
        .set((
            user_trust_stats::active_days.eq(stats.active_days + 1),
            user_trust_stats::last_active_day.eq(Some(today)),
            user_trust_stats::updated_at.eq(now),
        ))
        .get_result::<UserTrustStats>(conn)
        .map_err(AppError::from)
}

fn daily_message_count(
    conn: &mut diesel::PgConnection,
    user_id: Uuid,
    today: NaiveDate,
) -> Result<i32, AppError> {
    daily_action_counters::table
        .filter(daily_action_counters::user_id.eq(user_id))
        .filter(daily_action_counters::action_key.eq(ACTION_OUTBOUND_MESSAGE))
        .filter(daily_action_counters::day_bucket.eq(today))
        .select(daily_action_counters::count)
        .first::<i32>(conn)
        .optional()
        .map(|value| value.unwrap_or(0))
        .map_err(AppError::from)
}

fn increment_daily_counter(
    conn: &mut diesel::PgConnection,
    user_id: Uuid,
    action_key_value: &str,
    today: NaiveDate,
) -> Result<(), AppError> {
    diesel::insert_into(daily_action_counters::table)
        .values(&NewDailyActionCounter {
            user_id,
            action_key: action_key_value.to_string(),
            day_bucket: today,
            count: 1,
        })
        .on_conflict((
            daily_action_counters::user_id,
            daily_action_counters::action_key,
            daily_action_counters::day_bucket,
        ))
        .do_update()
        .set((
            daily_action_counters::count.eq(daily_action_counters::count + 1),
            daily_action_counters::updated_at.eq(Utc::now()),
        ))
        .execute(conn)?;
    Ok(())
}

fn level_band_for_active_days(active_days: i32) -> LevelBand {
    match active_days {
        i32::MIN..=6 => LevelBand {
            level: 1,
            min_active_days: 0,
            next_level_active_days: Some(7),
            daily_outbound_messages_limit: Some(50),
        },
        7..=13 => LevelBand {
            level: 2,
            min_active_days: 7,
            next_level_active_days: Some(14),
            daily_outbound_messages_limit: Some(100),
        },
        14..=29 => LevelBand {
            level: 3,
            min_active_days: 14,
            next_level_active_days: Some(30),
            daily_outbound_messages_limit: Some(200),
        },
        30..=59 => LevelBand {
            level: 4,
            min_active_days: 30,
            next_level_active_days: Some(60),
            daily_outbound_messages_limit: Some(500),
        },
        60..=89 => LevelBand {
            level: 5,
            min_active_days: 60,
            next_level_active_days: Some(90),
            daily_outbound_messages_limit: Some(1_000),
        },
        90..=119 => LevelBand {
            level: 6,
            min_active_days: 90,
            next_level_active_days: Some(120),
            daily_outbound_messages_limit: None,
        },
        120..=179 => LevelBand {
            level: 7,
            min_active_days: 120,
            next_level_active_days: Some(180),
            daily_outbound_messages_limit: None,
        },
        _ => LevelBand {
            level: 8,
            min_active_days: 180,
            next_level_active_days: None,
            daily_outbound_messages_limit: None,
        },
    }
}

fn level_progress_percent(active_days: i32, level_band: LevelBand) -> u8 {
    let Some(next_level_active_days) = level_band.next_level_active_days else {
        return 100;
    };

    let span = (next_level_active_days - level_band.min_active_days).max(1);
    let progressed = (active_days - level_band.min_active_days).clamp(0, span);
    ((progressed * 100) / span) as u8
}

fn rank_for_contribution_score(contribution_score: i32) -> &'static str {
    match contribution_score {
        i32::MIN..=99 => "F",
        100..=499 => "E",
        500..=999 => "D",
        1_000..=2_499 => "C",
        2_500..=4_999 => "B",
        5_000..=9_999 => "A",
        _ => "S",
    }
}

fn seconds_until_next_utc_day(now: DateTime<Utc>) -> i64 {
    let next_day = (now.date_naive() + Duration::days(1))
        .and_hms_opt(0, 0, 0)
        .expect("valid midnight");
    (next_day - now.naive_utc()).num_seconds().max(1)
}

#[cfg(test)]
mod tests {
    use super::{build_snapshot, level_band_for_active_days, rank_for_contribution_score};

    #[test]
    fn level_bands_match_mvp_thresholds() {
        assert_eq!(level_band_for_active_days(0).level, 1);
        assert_eq!(level_band_for_active_days(6).level, 1);
        assert_eq!(level_band_for_active_days(7).level, 2);
        assert_eq!(level_band_for_active_days(30).level, 4);
        assert_eq!(level_band_for_active_days(90).level, 6);
        assert_eq!(level_band_for_active_days(180).level, 8);
    }

    #[test]
    fn contribution_score_maps_to_rank() {
        assert_eq!(rank_for_contribution_score(0), "F");
        assert_eq!(rank_for_contribution_score(499), "E");
        assert_eq!(rank_for_contribution_score(5_000), "A");
        assert_eq!(rank_for_contribution_score(10_000), "S");
    }

    #[test]
    fn snapshot_reports_remaining_messages() {
        let snapshot = build_snapshot(7, 0, 40);
        assert_eq!(snapshot.level, 2);
        assert_eq!(snapshot.daily_outbound_messages_limit, Some(100));
        assert_eq!(snapshot.daily_outbound_messages_remaining, Some(60));
        assert_eq!(snapshot.rank, "F");
    }
}
