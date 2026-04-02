use chrono::{DateTime, Utc};
use diesel::dsl::{exists, select};
use diesel::prelude::*;
use diesel::PgConnection;
use serde::Serialize;
use serde_json::json;
use std::collections::{HashMap, HashSet};
use uuid::Uuid;

use crate::db::Pool;
use crate::errors::AppError;
use crate::models::moderation::{
    ModerationReport, NewModerationReport, NewUserBlock, UserBlock, UserSafetyStatePublic,
};
use crate::models::user::User;
use crate::schema::{messages, moderation_reports, room_messages, user_blocks, users};

pub const CURRENT_SAFETY_TERMS_VERSION: i32 = 1;

const BLOCKED_TERMS: &[&str] = &[
    "child porn",
    "cp",
    "cunt",
    "faggot",
    "fuck",
    "fucker",
    "fucking",
    "kill yourself",
    "kike",
    "nigger",
    "nigga",
    "rape",
    "rapist",
    "retard",
    "shit",
];

pub const REPORT_SOURCE_USER: &str = "user_report";
pub const REPORT_SOURCE_BLOCK: &str = "block_action";
pub const REPORT_KIND_USER_PROFILE: &str = "user_profile";
pub const REPORT_KIND_DIRECT_MESSAGE: &str = "direct_message";
pub const REPORT_KIND_ROOM_MESSAGE: &str = "room_message";
pub const REPORT_STATUS_OPEN: &str = "open";
pub const REPORT_STATUS_RESOLVED: &str = "resolved";
pub const REPORT_STATUS_DISMISSED: &str = "dismissed";
pub const RESOLUTION_DISMISS: &str = "dismiss";
pub const RESOLUTION_REMOVE_CONTENT: &str = "remove_content";
pub const RESOLUTION_SUSPEND_USER: &str = "suspend_user";
pub const RESOLUTION_REMOVE_AND_SUSPEND: &str = "remove_content_and_suspend_user";

#[derive(Debug, Clone, Serialize)]
pub struct BlockedUserView {
    pub user_id: Uuid,
    pub username: String,
    pub avatar_base64: Option<String>,
    pub blocked_at: DateTime<Utc>,
}

#[derive(Debug, Clone)]
pub struct ReportInput {
    pub reported_user_id: Uuid,
    pub source: String,
    pub content_kind: String,
    pub content_id: Option<String>,
    pub reason_code: String,
    pub reporter_note: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ModerationReportStatusFilter {
    Open,
    Resolved,
    Dismissed,
    Any,
}

impl ModerationReportStatusFilter {
    pub fn parse(raw: Option<&str>) -> Result<Self, AppError> {
        match raw.map(|value| value.trim().to_lowercase()) {
            None => Ok(Self::Open),
            Some(value) if value.is_empty() || value == REPORT_STATUS_OPEN => Ok(Self::Open),
            Some(value) if value == REPORT_STATUS_RESOLVED => Ok(Self::Resolved),
            Some(value) if value == REPORT_STATUS_DISMISSED => Ok(Self::Dismissed),
            Some(value) if value == "all" => Ok(Self::Any),
            Some(_) => Err(AppError::BadRequest(
                "status must be one of open, resolved, dismissed, all".into(),
            )),
        }
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct AdminModerationReportView {
    pub id: Uuid,
    pub reporter_user_id: Uuid,
    pub reporter_username: String,
    pub reported_user_id: Uuid,
    pub reported_username: String,
    pub source: String,
    pub content_kind: String,
    pub content_id: Option<String>,
    pub reason_code: String,
    pub reporter_note: Option<String>,
    pub content_excerpt: Option<String>,
    pub status: String,
    pub resolution_action: Option<String>,
    pub resolution_notes: Option<String>,
    pub review_due_at: DateTime<Utc>,
    pub reviewed_by_user_id: Option<Uuid>,
    pub reviewed_at: Option<DateTime<Utc>>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

pub fn requires_terms_acceptance(user: &User) -> bool {
    user.ugc_terms_accepted_at.is_none() || user.ugc_terms_version < CURRENT_SAFETY_TERMS_VERSION
}

pub fn safety_state_for_user(user: &User) -> UserSafetyStatePublic {
    UserSafetyStatePublic {
        current_terms_version: CURRENT_SAFETY_TERMS_VERSION,
        accepted_terms_version: user.ugc_terms_version,
        terms_accepted_at: user.ugc_terms_accepted_at,
        requires_terms_acceptance: requires_terms_acceptance(user),
    }
}

pub fn record_terms_acceptance(
    pool: &Pool,
    user_id: Uuid,
    accepted_version: i32,
) -> Result<UserSafetyStatePublic, AppError> {
    let mut conn = pool.get()?;
    conn.transaction::<UserSafetyStatePublic, AppError, _>(|conn| {
        let user = record_terms_acceptance_conn(conn, user_id, accepted_version)?;
        Ok(safety_state_for_user(&user))
    })
}

pub fn record_terms_acceptance_conn(
    conn: &mut PgConnection,
    user_id: Uuid,
    accepted_version: i32,
) -> Result<User, AppError> {
    if accepted_version != CURRENT_SAFETY_TERMS_VERSION {
        return Err(AppError::BadRequest(
            "accepted_terms_version is not the current policy version".into(),
        ));
    }

    let user = diesel::update(users::table.find(user_id))
        .set((
            users::ugc_terms_version.eq(CURRENT_SAFETY_TERMS_VERSION),
            users::ugc_terms_accepted_at.eq(Some(Utc::now())),
        ))
        .get_result::<User>(conn)?;
    Ok(user)
}

pub fn list_blocked_user_ids(
    pool: &Pool,
    blocker_user_id: Uuid,
) -> Result<HashSet<Uuid>, AppError> {
    let mut conn = pool.get()?;
    list_blocked_user_ids_conn(&mut conn, blocker_user_id)
}

pub(crate) fn list_blocked_user_ids_conn(
    conn: &mut PgConnection,
    blocker_user_id: Uuid,
) -> Result<HashSet<Uuid>, AppError> {
    let ids = user_blocks::table
        .filter(user_blocks::blocker_user_id.eq(blocker_user_id))
        .select(user_blocks::blocked_user_id)
        .load::<Uuid>(conn)?;
    Ok(ids.into_iter().collect())
}

pub fn list_blocked_users(
    pool: &Pool,
    blocker_user_id: Uuid,
) -> Result<Vec<BlockedUserView>, AppError> {
    let mut conn = pool.get()?;
    let blocks = user_blocks::table
        .filter(user_blocks::blocker_user_id.eq(blocker_user_id))
        .select(UserBlock::as_select())
        .order(user_blocks::created_at.desc())
        .load::<UserBlock>(&mut conn)?;

    if blocks.is_empty() {
        return Ok(Vec::new());
    }

    let blocked_ids = blocks
        .iter()
        .map(|block| block.blocked_user_id)
        .collect::<Vec<_>>();
    let users_by_id = users::table
        .filter(users::id.eq_any(&blocked_ids))
        .select((users::id, users::username, users::avatar_base64))
        .load::<(Uuid, String, Option<String>)>(&mut conn)?
        .into_iter()
        .map(|(id, username, avatar_base64)| (id, (username, avatar_base64)))
        .collect::<HashMap<_, _>>();

    Ok(blocks
        .into_iter()
        .filter_map(|block| {
            users_by_id
                .get(&block.blocked_user_id)
                .map(|(username, avatar_base64)| BlockedUserView {
                    user_id: block.blocked_user_id,
                    username: username.clone(),
                    avatar_base64: avatar_base64.clone(),
                    blocked_at: block.created_at,
                })
        })
        .collect())
}

pub fn is_user_blocked(
    pool: &Pool,
    blocker_user_id: Uuid,
    blocked_user_id: Uuid,
) -> Result<bool, AppError> {
    let mut conn = pool.get()?;
    select(exists(
        user_blocks::table
            .filter(user_blocks::blocker_user_id.eq(blocker_user_id))
            .filter(user_blocks::blocked_user_id.eq(blocked_user_id)),
    ))
    .get_result::<bool>(&mut conn)
    .map_err(AppError::from)
}

pub fn is_block_active_between(pool: &Pool, user_a: Uuid, user_b: Uuid) -> Result<bool, AppError> {
    let mut conn = pool.get()?;
    select(exists(
        user_blocks::table.filter(
            user_blocks::blocker_user_id
                .eq(user_a)
                .and(user_blocks::blocked_user_id.eq(user_b))
                .or(user_blocks::blocker_user_id
                    .eq(user_b)
                    .and(user_blocks::blocked_user_id.eq(user_a))),
        ),
    ))
    .get_result::<bool>(&mut conn)
    .map_err(AppError::from)
}

pub fn ensure_no_block_between(pool: &Pool, user_a: Uuid, user_b: Uuid) -> Result<(), AppError> {
    if is_block_active_between(pool, user_a, user_b)? {
        return Err(AppError::Forbidden);
    }
    Ok(())
}

pub fn create_report(
    pool: &Pool,
    reporter_user_id: Uuid,
    input: ReportInput,
) -> Result<ModerationReport, AppError> {
    let mut conn = pool.get()?;
    conn.transaction::<ModerationReport, AppError, _>(|conn| {
        create_report_conn(conn, reporter_user_id, input)
    })
}

pub fn block_user(
    pool: &Pool,
    blocker_user_id: Uuid,
    blocked_user_id: Uuid,
    reason_code: Option<String>,
    reporter_note: Option<String>,
) -> Result<BlockedUserView, AppError> {
    let mut conn = pool.get()?;
    conn.transaction::<BlockedUserView, AppError, _>(|conn| {
        if blocker_user_id == blocked_user_id {
            return Err(AppError::BadRequest(
                "You cannot block your own account".into(),
            ));
        }

        let blocked_user = users::table
            .find(blocked_user_id)
            .first::<User>(conn)
            .optional()?
            .ok_or(AppError::NotFound)?;

        let exists_already = select(exists(
            user_blocks::table
                .filter(user_blocks::blocker_user_id.eq(blocker_user_id))
                .filter(user_blocks::blocked_user_id.eq(blocked_user_id)),
        ))
        .get_result::<bool>(conn)?;

        if !exists_already {
            let report = create_report_conn(
                conn,
                blocker_user_id,
                ReportInput {
                    reported_user_id: blocked_user_id,
                    source: REPORT_SOURCE_BLOCK.to_string(),
                    content_kind: REPORT_KIND_USER_PROFILE.to_string(),
                    content_id: None,
                    reason_code: normalize_reason_code(reason_code.as_deref(), "abusive_user"),
                    reporter_note: normalize_optional_text(reporter_note.as_deref()),
                },
            )?;

            diesel::insert_into(user_blocks::table)
                .values(&NewUserBlock {
                    blocker_user_id,
                    blocked_user_id,
                    report_id: Some(report.id),
                    reason_code: Some(report.reason_code.clone()),
                    reporter_note: report.reporter_note.clone(),
                })
                .execute(conn)?;
        }

        Ok(BlockedUserView {
            user_id: blocked_user.id,
            username: blocked_user.username,
            avatar_base64: blocked_user.avatar_base64,
            blocked_at: Utc::now(),
        })
    })
}

pub fn unblock_user(
    pool: &Pool,
    blocker_user_id: Uuid,
    blocked_user_id: Uuid,
) -> Result<(), AppError> {
    let mut conn = pool.get()?;
    diesel::delete(
        user_blocks::table
            .filter(user_blocks::blocker_user_id.eq(blocker_user_id))
            .filter(user_blocks::blocked_user_id.eq(blocked_user_id)),
    )
    .execute(&mut conn)?;
    Ok(())
}

pub fn list_reports(
    pool: &Pool,
    status_filter: ModerationReportStatusFilter,
    limit: i64,
) -> Result<Vec<AdminModerationReportView>, AppError> {
    let mut conn = pool.get()?;
    let limit = limit.clamp(1, 200);
    let mut query = moderation_reports::table.into_boxed();

    query = match status_filter {
        ModerationReportStatusFilter::Open => {
            query.filter(moderation_reports::status.eq(REPORT_STATUS_OPEN))
        }
        ModerationReportStatusFilter::Resolved => {
            query.filter(moderation_reports::status.eq(REPORT_STATUS_RESOLVED))
        }
        ModerationReportStatusFilter::Dismissed => {
            query.filter(moderation_reports::status.eq(REPORT_STATUS_DISMISSED))
        }
        ModerationReportStatusFilter::Any => query,
    };

    let reports = query
        .order((
            moderation_reports::status.asc(),
            moderation_reports::created_at.desc(),
        ))
        .limit(limit)
        .select(ModerationReport::as_select())
        .load::<ModerationReport>(&mut conn)?;

    reports_to_admin_views(&mut conn, reports)
}

pub fn resolve_report(
    pool: &Pool,
    report_id: Uuid,
    reviewed_by_user_id: Uuid,
    resolution_action: &str,
    resolution_notes: Option<&str>,
) -> Result<AdminModerationReportView, AppError> {
    let mut conn = pool.get()?;
    conn.transaction::<AdminModerationReportView, AppError, _>(|conn| {
        let report = moderation_reports::table
            .find(report_id)
            .select(ModerationReport::as_select())
            .first::<ModerationReport>(conn)
            .optional()?
            .ok_or(AppError::NotFound)?;

        if report.status != REPORT_STATUS_OPEN {
            return Err(AppError::Conflict(
                "Report has already been reviewed".into(),
            ));
        }

        let normalized_action = normalize_resolution_action(resolution_action)?;
        if normalized_action == RESOLUTION_REMOVE_CONTENT
            || normalized_action == RESOLUTION_REMOVE_AND_SUSPEND
        {
            remove_reported_content_conn(conn, &report)?;
        }
        if normalized_action == RESOLUTION_SUSPEND_USER
            || normalized_action == RESOLUTION_REMOVE_AND_SUSPEND
        {
            diesel::update(users::table.find(report.reported_user_id))
                .set(users::is_active.eq(false))
                .execute(conn)?;
        }

        let next_status = if normalized_action == RESOLUTION_DISMISS {
            REPORT_STATUS_DISMISSED
        } else {
            REPORT_STATUS_RESOLVED
        };
        let reviewed_at = Utc::now();
        let updated = diesel::update(moderation_reports::table.find(report.id))
            .set((
                moderation_reports::status.eq(next_status),
                moderation_reports::resolution_action.eq(Some(normalized_action.to_string())),
                moderation_reports::resolution_notes.eq(normalize_optional_text(resolution_notes)),
                moderation_reports::reviewed_by_user_id.eq(Some(reviewed_by_user_id)),
                moderation_reports::reviewed_at.eq(Some(reviewed_at)),
            ))
            .get_result::<ModerationReport>(conn)?;

        let mut views = reports_to_admin_views(conn, vec![updated])?;
        views.pop().ok_or(AppError::NotFound)
    })
}

pub fn ensure_text_is_allowed(field_name: &str, value: &str) -> Result<(), AppError> {
    if let Some(term) = detect_blocked_term(value) {
        return Err(AppError::BadRequest(format!(
            "{field_name} contains objectionable content ('{term}')"
        )));
    }
    Ok(())
}

pub fn detect_blocked_term(value: &str) -> Option<String> {
    let lowercase = value.trim().to_lowercase();
    if lowercase.is_empty() {
        return None;
    }

    let tokens = lowercase
        .split(|ch: char| !ch.is_ascii_alphanumeric())
        .filter(|token| !token.is_empty())
        .collect::<HashSet<_>>();

    for term in BLOCKED_TERMS {
        if term.contains(' ') {
            if lowercase.contains(term) {
                return Some((*term).to_string());
            }
            continue;
        }
        if tokens.contains(term) {
            return Some((*term).to_string());
        }
    }

    None
}

fn create_report_conn(
    conn: &mut PgConnection,
    reporter_user_id: Uuid,
    input: ReportInput,
) -> Result<ModerationReport, AppError> {
    if reporter_user_id == input.reported_user_id {
        return Err(AppError::BadRequest(
            "You cannot report your own account".into(),
        ));
    }

    users::table
        .find(input.reported_user_id)
        .first::<User>(conn)
        .optional()?
        .ok_or(AppError::NotFound)?;

    let source = normalize_report_source(&input.source)?;
    let content_kind = normalize_content_kind(&input.content_kind)?;
    let content_id = normalize_optional_text(input.content_id.as_deref());
    let reason_code = normalize_reason_code(Some(&input.reason_code), "other");
    let reporter_note = normalize_optional_text(input.reporter_note.as_deref());
    let content_excerpt = content_excerpt_conn(
        conn,
        input.reported_user_id,
        content_kind,
        content_id.as_deref(),
    )?;

    let new_report = NewModerationReport {
        id: Uuid::new_v4(),
        reporter_user_id,
        reported_user_id: input.reported_user_id,
        source: source.to_string(),
        content_kind: content_kind.to_string(),
        content_id,
        reason_code,
        reporter_note,
        content_excerpt,
        status: REPORT_STATUS_OPEN.to_string(),
        metadata: json!({}),
    };

    diesel::insert_into(moderation_reports::table)
        .values(&new_report)
        .get_result::<ModerationReport>(conn)
        .map_err(AppError::from)
}

fn reports_to_admin_views(
    conn: &mut PgConnection,
    reports: Vec<ModerationReport>,
) -> Result<Vec<AdminModerationReportView>, AppError> {
    if reports.is_empty() {
        return Ok(Vec::new());
    }

    let user_ids = reports
        .iter()
        .flat_map(|report| {
            [
                Some(report.reporter_user_id),
                Some(report.reported_user_id),
                report.reviewed_by_user_id,
            ]
        })
        .flatten()
        .collect::<HashSet<_>>()
        .into_iter()
        .collect::<Vec<_>>();

    let usernames_by_id = users::table
        .filter(users::id.eq_any(&user_ids))
        .select((users::id, users::username))
        .load::<(Uuid, String)>(conn)?
        .into_iter()
        .collect::<HashMap<_, _>>();

    Ok(reports
        .into_iter()
        .map(|report| AdminModerationReportView {
            id: report.id,
            reporter_user_id: report.reporter_user_id,
            reporter_username: usernames_by_id
                .get(&report.reporter_user_id)
                .cloned()
                .unwrap_or_else(|| report.reporter_user_id.to_string()),
            reported_user_id: report.reported_user_id,
            reported_username: usernames_by_id
                .get(&report.reported_user_id)
                .cloned()
                .unwrap_or_else(|| report.reported_user_id.to_string()),
            source: report.source,
            content_kind: report.content_kind,
            content_id: report.content_id,
            reason_code: report.reason_code,
            reporter_note: report.reporter_note,
            content_excerpt: report.content_excerpt,
            status: report.status,
            resolution_action: report.resolution_action,
            resolution_notes: report.resolution_notes,
            review_due_at: report.review_due_at,
            reviewed_by_user_id: report.reviewed_by_user_id,
            reviewed_at: report.reviewed_at,
            created_at: report.created_at,
            updated_at: report.updated_at,
        })
        .collect())
}

fn content_excerpt_conn(
    conn: &mut PgConnection,
    reported_user_id: Uuid,
    content_kind: &str,
    content_id: Option<&str>,
) -> Result<Option<String>, AppError> {
    match content_kind {
        REPORT_KIND_USER_PROFILE => {
            let user = users::table
                .find(reported_user_id)
                .first::<User>(conn)
                .optional()?
                .ok_or(AppError::NotFound)?;
            let description = user.description.unwrap_or_default();
            let excerpt = format!("username: {}\ndescription: {}", user.username, description);
            Ok(Some(excerpt.trim().to_string()))
        }
        REPORT_KIND_DIRECT_MESSAGE => {
            let Some(raw_id) = content_id else {
                return Err(AppError::BadRequest(
                    "content_id is required for direct_message reports".into(),
                ));
            };
            let message_id = Uuid::parse_str(raw_id)
                .map_err(|_| AppError::BadRequest("content_id must be a UUID".into()))?;
            let message = messages::table
                .find(message_id)
                .first::<crate::models::message::Message>(conn)
                .optional()?
                .ok_or(AppError::NotFound)?;
            if message.sender_id != reported_user_id || message.deleted_at.is_some() {
                return Err(AppError::NotFound);
            }
            Ok(Some(message.content))
        }
        REPORT_KIND_ROOM_MESSAGE => {
            let Some(raw_id) = content_id else {
                return Err(AppError::BadRequest(
                    "content_id is required for room_message reports".into(),
                ));
            };
            let message_id = Uuid::parse_str(raw_id)
                .map_err(|_| AppError::BadRequest("content_id must be a UUID".into()))?;
            let message = room_messages::table
                .find(message_id)
                .first::<crate::models::room::RoomMessage>(conn)
                .optional()?
                .ok_or(AppError::NotFound)?;
            if message.sender_id != reported_user_id || message.deleted_at.is_some() {
                return Err(AppError::NotFound);
            }
            Ok(Some(message.content))
        }
        _ => Err(AppError::BadRequest(
            "Unsupported report content kind".into(),
        )),
    }
}

fn remove_reported_content_conn(
    conn: &mut PgConnection,
    report: &ModerationReport,
) -> Result<(), AppError> {
    match report.content_kind.as_str() {
        REPORT_KIND_USER_PROFILE => {
            diesel::update(users::table.find(report.reported_user_id))
                .set((
                    users::avatar_base64.eq(None::<String>),
                    users::description.eq(None::<String>),
                ))
                .execute(conn)?;
            Ok(())
        }
        REPORT_KIND_DIRECT_MESSAGE => {
            let content_id = report
                .content_id
                .as_deref()
                .ok_or_else(|| AppError::BadRequest("Report content_id is missing".into()))?;
            let message_id = Uuid::parse_str(content_id)
                .map_err(|_| AppError::BadRequest("Report content_id is invalid".into()))?;
            diesel::update(messages::table.find(message_id))
                .set(messages::deleted_at.eq(Some(Utc::now())))
                .execute(conn)?;
            Ok(())
        }
        REPORT_KIND_ROOM_MESSAGE => {
            let content_id = report
                .content_id
                .as_deref()
                .ok_or_else(|| AppError::BadRequest("Report content_id is missing".into()))?;
            let message_id = Uuid::parse_str(content_id)
                .map_err(|_| AppError::BadRequest("Report content_id is invalid".into()))?;
            diesel::update(room_messages::table.find(message_id))
                .set(room_messages::deleted_at.eq(Some(Utc::now())))
                .execute(conn)?;
            Ok(())
        }
        _ => Err(AppError::BadRequest(
            "Unsupported report content kind".into(),
        )),
    }
}

fn normalize_report_source(raw: &str) -> Result<&'static str, AppError> {
    match raw.trim() {
        REPORT_SOURCE_USER => Ok(REPORT_SOURCE_USER),
        REPORT_SOURCE_BLOCK => Ok(REPORT_SOURCE_BLOCK),
        _ => Err(AppError::BadRequest("Invalid report source".into())),
    }
}

fn normalize_content_kind(raw: &str) -> Result<&'static str, AppError> {
    match raw.trim() {
        REPORT_KIND_USER_PROFILE => Ok(REPORT_KIND_USER_PROFILE),
        REPORT_KIND_DIRECT_MESSAGE => Ok(REPORT_KIND_DIRECT_MESSAGE),
        REPORT_KIND_ROOM_MESSAGE => Ok(REPORT_KIND_ROOM_MESSAGE),
        _ => Err(AppError::BadRequest("Invalid report content kind".into())),
    }
}

fn normalize_resolution_action(raw: &str) -> Result<&'static str, AppError> {
    match raw.trim() {
        RESOLUTION_DISMISS => Ok(RESOLUTION_DISMISS),
        RESOLUTION_REMOVE_CONTENT => Ok(RESOLUTION_REMOVE_CONTENT),
        RESOLUTION_SUSPEND_USER => Ok(RESOLUTION_SUSPEND_USER),
        RESOLUTION_REMOVE_AND_SUSPEND => Ok(RESOLUTION_REMOVE_AND_SUSPEND),
        _ => Err(AppError::BadRequest(
            "action must be one of dismiss, remove_content, suspend_user, remove_content_and_suspend_user"
                .into(),
        )),
    }
}

fn normalize_reason_code(raw: Option<&str>, fallback: &str) -> String {
    raw.and_then(|value| normalize_optional_text(Some(value)))
        .unwrap_or_else(|| fallback.to_string())
}

fn normalize_optional_text(raw: Option<&str>) -> Option<String> {
    raw.map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToString::to_string)
}
