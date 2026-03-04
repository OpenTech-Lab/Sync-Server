use diesel::prelude::*;
use uuid::Uuid;

use crate::db::Pool;
use crate::errors::AppError;
use crate::models::server_news::{NewServerNews, ServerNews};
use crate::schema::server_news::dsl as news_dsl;

const MAX_TITLE_CHARS: usize = 120;
const MAX_SUMMARY_CHARS: usize = 280;
const MAX_MARKDOWN_CHARS: usize = 20_000;

fn normalize_optional_text(value: &str) -> Option<String> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        None
    } else {
        Some(trimmed.to_string())
    }
}

pub fn create_news(
    pool: &Pool,
    admin_user_id: Uuid,
    title: &str,
    summary: Option<&str>,
    markdown_content: &str,
) -> Result<ServerNews, AppError> {
    let normalized_title = title.trim();
    if normalized_title.is_empty() {
        return Err(AppError::BadRequest("title cannot be empty".into()));
    }
    if normalized_title.chars().count() > MAX_TITLE_CHARS {
        return Err(AppError::BadRequest(format!(
            "title must be <= {MAX_TITLE_CHARS} characters"
        )));
    }

    let normalized_markdown = markdown_content.trim();
    if normalized_markdown.is_empty() {
        return Err(AppError::BadRequest(
            "markdown_content cannot be empty".into(),
        ));
    }
    if normalized_markdown.chars().count() > MAX_MARKDOWN_CHARS {
        return Err(AppError::BadRequest(format!(
            "markdown_content must be <= {MAX_MARKDOWN_CHARS} characters"
        )));
    }

    let normalized_summary = summary.and_then(normalize_optional_text);
    if normalized_summary
        .as_ref()
        .map(|value| value.chars().count() > MAX_SUMMARY_CHARS)
        .unwrap_or(false)
    {
        return Err(AppError::BadRequest(format!(
            "summary must be <= {MAX_SUMMARY_CHARS} characters"
        )));
    }

    let mut conn = pool.get()?;
    let payload = NewServerNews {
        id: Uuid::new_v4(),
        title: normalized_title.to_string(),
        summary: normalized_summary,
        markdown_content: normalized_markdown.to_string(),
        created_by: Some(admin_user_id),
    };

    diesel::insert_into(news_dsl::server_news)
        .values(&payload)
        .execute(&mut conn)?;

    news_dsl::server_news
        .find(payload.id)
        .select(ServerNews::as_select())
        .first::<ServerNews>(&mut conn)
        .map_err(AppError::from)
}

pub fn list_news(pool: &Pool, limit: i64) -> Result<Vec<ServerNews>, AppError> {
    let mut conn = pool.get()?;
    let safe_limit = limit.clamp(1, 100);

    news_dsl::server_news
        .order(news_dsl::published_at.desc())
        .limit(safe_limit)
        .select(ServerNews::as_select())
        .load::<ServerNews>(&mut conn)
        .map_err(AppError::from)
}

pub fn get_news_by_id(pool: &Pool, news_id: Uuid) -> Result<Option<ServerNews>, AppError> {
    let mut conn = pool.get()?;

    news_dsl::server_news
        .find(news_id)
        .select(ServerNews::as_select())
        .first::<ServerNews>(&mut conn)
        .optional()
        .map_err(AppError::from)
}
