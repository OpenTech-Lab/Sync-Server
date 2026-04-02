use base64::Engine;
use diesel::{prelude::*, PgConnection};
use uuid::Uuid;

use crate::db::Pool;
use crate::errors::AppError;
use crate::models::sticker::{NewSticker, Sticker, StickerDetail, StickerListItem};
use crate::schema::stickers::dsl as sticker_dsl;

const ALLOWED_MIME_TYPES: [&str; 4] = ["image/png", "image/webp", "image/gif", "image/jpeg"];
const MAX_STICKER_SIZE_BYTES: usize = 256 * 1024;
const MAX_STICKERS_PER_USER: i64 = 120;
const MAX_TOTAL_BYTES_PER_USER: i64 = 8 * 1024 * 1024;

#[derive(Debug, Clone, serde::Deserialize)]
pub struct UploadStickerInput {
    pub group_name: String,
    pub name: String,
    pub mime_type: String,
    pub content_base64: String,
    pub group_author: Option<String>,
}

#[derive(Debug, serde::Serialize)]
pub struct StickerGroupPublic {
    pub name: String,
    pub author: Option<String>,
    pub tab_sticker_id: Option<Uuid>,
    pub total: usize,
}

pub fn upload_sticker(
    pool: &Pool,
    uploader_id: Uuid,
    uploader_role: &str,
    input: UploadStickerInput,
) -> Result<StickerDetail, AppError> {
    let mut conn = pool.get()?;
    upload_sticker_conn(&mut conn, uploader_id, uploader_role, input)
}

pub fn upload_sticker_conn(
    conn: &mut PgConnection,
    uploader_id: Uuid,
    uploader_role: &str,
    input: UploadStickerInput,
) -> Result<StickerDetail, AppError> {
    let name = input.name.trim();
    if name.is_empty() || name.len() > 80 {
        return Err(AppError::BadRequest(
            "name is required and must be <= 80 chars".into(),
        ));
    }
    let group_name = input.group_name.trim();
    if group_name.is_empty() || group_name.len() > 40 {
        return Err(AppError::BadRequest(
            "group_name is required and must be <= 40 chars".into(),
        ));
    }

    if !ALLOWED_MIME_TYPES.contains(&input.mime_type.as_str()) {
        return Err(AppError::BadRequest(
            "mime_type must be one of image/png,image/webp,image/gif,image/jpeg".into(),
        ));
    }

    let decoded = base64::engine::general_purpose::STANDARD
        .decode(input.content_base64.trim())
        .map_err(|_| AppError::BadRequest("content_base64 must be valid base64".into()))?;

    if decoded.is_empty() || decoded.len() > MAX_STICKER_SIZE_BYTES {
        return Err(AppError::BadRequest(format!(
            "sticker size must be between 1 and {MAX_STICKER_SIZE_BYTES} bytes"
        )));
    }

    let existing_count: i64 = sticker_dsl::stickers
        .filter(sticker_dsl::uploader_id.eq(uploader_id))
        .count()
        .get_result(conn)?;

    if existing_count >= MAX_STICKERS_PER_USER {
        return Err(AppError::BadRequest(format!(
            "sticker quota exceeded ({MAX_STICKERS_PER_USER} per user)"
        )));
    }

    let used_bytes: Option<i64> = sticker_dsl::stickers
        .filter(sticker_dsl::uploader_id.eq(uploader_id))
        .select(diesel::dsl::sum(sticker_dsl::size_bytes))
        .first(conn)
        .optional()?
        .flatten();

    let next_total = used_bytes.unwrap_or(0) + decoded.len() as i64;
    if next_total > MAX_TOTAL_BYTES_PER_USER {
        return Err(AppError::BadRequest(format!(
            "storage quota exceeded ({MAX_TOTAL_BYTES_PER_USER} bytes per user)"
        )));
    }

    let status = if uploader_role == "admin" {
        "active"
    } else {
        "pending"
    }
    .to_string();

    // group_author is only meaningful on the __tab__ sentinel sticker
    let group_author = if name == "__tab__" {
        input
            .group_author
            .as_deref()
            .map(|a| a.trim().to_string())
            .filter(|a| !a.is_empty())
    } else {
        None
    };

    let entity = NewSticker {
        id: Uuid::new_v4(),
        uploader_id,
        group_name: group_name.to_string(),
        name: name.to_string(),
        mime_type: input.mime_type,
        content_base64: input.content_base64.trim().to_string(),
        size_bytes: decoded.len() as i32,
        status,
        group_author,
    };

    diesel::insert_into(sticker_dsl::stickers)
        .values(&entity)
        .execute(conn)?;

    let saved = sticker_dsl::stickers
        .find(entity.id)
        .select(Sticker::as_select())
        .first::<Sticker>(conn)?;

    Ok(StickerDetail::from(saved))
}

pub fn rename_sticker_group(
    pool: &Pool,
    old_name: &str,
    new_name: &str,
) -> Result<usize, AppError> {
    let old_name = old_name.trim();
    let new_name = new_name.trim();

    if old_name.is_empty() || new_name.is_empty() {
        return Err(AppError::BadRequest(
            "old_name and new_name are required".into(),
        ));
    }
    if new_name.len() > 40 {
        return Err(AppError::BadRequest("new_name must be <= 40 chars".into()));
    }

    let mut conn = pool.get()?;

    let changed =
        diesel::update(sticker_dsl::stickers.filter(sticker_dsl::group_name.eq(old_name)))
            .set((
                sticker_dsl::group_name.eq(new_name),
                sticker_dsl::updated_at.eq(chrono::Utc::now()),
            ))
            .execute(&mut conn)?;

    if changed == 0 {
        return Err(AppError::NotFound);
    }

    Ok(changed)
}

pub fn supported_mime_types() -> &'static [&'static str] {
    &ALLOWED_MIME_TYPES
}

pub fn list_stickers(
    pool: &Pool,
    requester_id: Uuid,
    requester_role: &str,
) -> Result<Vec<StickerListItem>, AppError> {
    let mut conn = pool.get()?;

    let rows = if requester_role == "admin" {
        sticker_dsl::stickers
            .order(sticker_dsl::group_name.asc())
            .order(sticker_dsl::created_at.desc())
            .select(Sticker::as_select())
            .load::<Sticker>(&mut conn)?
    } else {
        sticker_dsl::stickers
            .filter(
                sticker_dsl::status
                    .eq("active")
                    .or(sticker_dsl::uploader_id.eq(requester_id)),
            )
            .order(sticker_dsl::group_name.asc())
            .order(sticker_dsl::created_at.desc())
            .select(Sticker::as_select())
            .load::<Sticker>(&mut conn)?
    };

    Ok(rows.into_iter().map(StickerListItem::from).collect())
}

pub fn get_sticker(
    pool: &Pool,
    requester_id: Uuid,
    requester_role: &str,
    sticker_id: Uuid,
) -> Result<StickerDetail, AppError> {
    let mut conn = pool.get()?;
    let sticker = sticker_dsl::stickers
        .find(sticker_id)
        .select(Sticker::as_select())
        .first::<Sticker>(&mut conn)
        .optional()?
        .ok_or(AppError::NotFound)?;

    let can_view = requester_role == "admin"
        || sticker.status == "active"
        || sticker.uploader_id == requester_id;

    if !can_view {
        return Err(AppError::Forbidden);
    }

    Ok(StickerDetail::from(sticker))
}

pub fn update_group_author(
    pool: &Pool,
    group_name: &str,
    author: Option<&str>,
) -> Result<(), AppError> {
    let group_name = group_name.trim();
    if group_name.is_empty() {
        return Err(AppError::BadRequest("group_name is required".into()));
    }

    let author_value = author.map(|a| a.trim()).filter(|a| !a.is_empty());

    let mut conn = pool.get()?;

    let changed = diesel::update(
        sticker_dsl::stickers
            .filter(sticker_dsl::group_name.eq(group_name))
            .filter(sticker_dsl::name.eq("__tab__")),
    )
    .set((
        sticker_dsl::group_author.eq(author_value),
        sticker_dsl::updated_at.eq(chrono::Utc::now()),
    ))
    .execute(&mut conn)?;

    if changed == 0 {
        return Err(AppError::NotFound);
    }

    Ok(())
}

pub fn list_sticker_groups(pool: &Pool) -> Result<Vec<StickerGroupPublic>, AppError> {
    let mut conn = pool.get()?;

    let rows = sticker_dsl::stickers
        .filter(sticker_dsl::status.eq("active"))
        .order(sticker_dsl::group_name.asc())
        .order(sticker_dsl::created_at.asc())
        .select(Sticker::as_select())
        .load::<Sticker>(&mut conn)?;

    let mut groups: Vec<StickerGroupPublic> = Vec::new();
    for sticker in rows {
        if let Some(g) = groups.iter_mut().find(|g| g.name == sticker.group_name) {
            if sticker.name == "__tab__" {
                g.tab_sticker_id = Some(sticker.id);
                g.author = sticker.group_author;
            } else {
                g.total += 1;
            }
        } else {
            let (tab_sticker_id, author, total) = if sticker.name == "__tab__" {
                (Some(sticker.id), sticker.group_author, 0)
            } else {
                (None, None, 1)
            };
            groups.push(StickerGroupPublic {
                name: sticker.group_name,
                author,
                tab_sticker_id,
                total,
            });
        }
    }

    // Only expose groups that have a tab sticker (properly created groups)
    groups.retain(|g| g.tab_sticker_id.is_some());

    Ok(groups)
}

pub fn moderate_sticker(
    pool: &Pool,
    sticker_id: Uuid,
    action: &str,
) -> Result<StickerListItem, AppError> {
    if action != "approve" && action != "reject" {
        return Err(AppError::BadRequest(
            "action must be one of: approve,reject".into(),
        ));
    }

    let status = if action == "approve" {
        "active"
    } else {
        "rejected"
    };

    let mut conn = pool.get()?;

    let changed = diesel::update(sticker_dsl::stickers.find(sticker_id))
        .set((
            sticker_dsl::status.eq(status),
            sticker_dsl::updated_at.eq(chrono::Utc::now()),
        ))
        .execute(&mut conn)?;

    if changed == 0 {
        return Err(AppError::NotFound);
    }

    let updated = sticker_dsl::stickers
        .find(sticker_id)
        .select(Sticker::as_select())
        .first::<Sticker>(&mut conn)?;

    Ok(StickerListItem::from(updated))
}
