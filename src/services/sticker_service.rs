use base64::Engine;
use diesel::prelude::*;
use uuid::Uuid;

use crate::db::Pool;
use crate::errors::AppError;
use crate::models::sticker::{NewSticker, Sticker, StickerDetail, StickerListItem};
use crate::schema::stickers::dsl as sticker_dsl;

const ALLOWED_MIME_TYPES: [&str; 4] = ["image/png", "image/webp", "image/gif", "image/jpeg"];
const MAX_STICKER_SIZE_BYTES: usize = 256 * 1024;
const MAX_STICKERS_PER_USER: i64 = 120;
const MAX_TOTAL_BYTES_PER_USER: i64 = 8 * 1024 * 1024;

#[derive(Debug, serde::Deserialize)]
pub struct UploadStickerInput {
    pub group_name: String,
    pub name: String,
    pub mime_type: String,
    pub content_base64: String,
}

pub fn upload_sticker(
    pool: &Pool,
    uploader_id: Uuid,
    uploader_role: &str,
    input: UploadStickerInput,
) -> Result<StickerDetail, AppError> {
    let mut conn = pool.get()?;

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
        .get_result(&mut conn)?;

    if existing_count >= MAX_STICKERS_PER_USER {
        return Err(AppError::BadRequest(format!(
            "sticker quota exceeded ({MAX_STICKERS_PER_USER} per user)"
        )));
    }

    let used_bytes: Option<i64> = sticker_dsl::stickers
        .filter(sticker_dsl::uploader_id.eq(uploader_id))
        .select(diesel::dsl::sum(sticker_dsl::size_bytes))
        .first(&mut conn)
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

    let entity = NewSticker {
        id: Uuid::new_v4(),
        uploader_id,
        group_name: group_name.to_string(),
        name: name.to_string(),
        mime_type: input.mime_type,
        content_base64: input.content_base64.trim().to_string(),
        size_bytes: decoded.len() as i32,
        status,
    };

    diesel::insert_into(sticker_dsl::stickers)
        .values(&entity)
        .execute(&mut conn)?;

    let saved = sticker_dsl::stickers
        .find(entity.id)
        .select(Sticker::as_select())
        .first::<Sticker>(&mut conn)?;

    Ok(StickerDetail::from(saved))
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
