use chrono::Utc;
use diesel::prelude::*;
use uuid::Uuid;

use crate::db::Pool;
use crate::errors::AppError;
use crate::models::encrypted_backup::{EncryptedBackup, NewEncryptedBackup};
use crate::schema::encrypted_backups::dsl as backup_dsl;

pub fn upsert_backup(
    pool: &Pool,
    owner_id: Uuid,
    encrypted_payload: &str,
) -> Result<EncryptedBackup, AppError> {
    let mut conn = pool.get()?;
    let now = Utc::now();

    diesel::insert_into(crate::schema::encrypted_backups::table)
        .values(NewEncryptedBackup {
            user_id: owner_id,
            encrypted_blob: encrypted_payload.to_string(),
        })
        .on_conflict(backup_dsl::user_id)
        .do_update()
        .set((
            backup_dsl::encrypted_blob.eq(encrypted_payload),
            backup_dsl::updated_at.eq(now),
        ))
        .execute(&mut conn)?;

    backup_dsl::encrypted_backups
        .filter(backup_dsl::user_id.eq(owner_id))
        .select(EncryptedBackup::as_select())
        .first::<EncryptedBackup>(&mut conn)
        .map_err(AppError::from)
}

pub fn get_backup(pool: &Pool, owner_id: Uuid) -> Result<Option<EncryptedBackup>, AppError> {
    let mut conn = pool.get()?;
    backup_dsl::encrypted_backups
        .filter(backup_dsl::user_id.eq(owner_id))
        .select(EncryptedBackup::as_select())
        .first::<EncryptedBackup>(&mut conn)
        .optional()
        .map_err(AppError::from)
}

pub fn delete_backup(pool: &Pool, owner_id: Uuid) -> Result<usize, AppError> {
    let mut conn = pool.get()?;
    diesel::delete(backup_dsl::encrypted_backups.filter(backup_dsl::user_id.eq(owner_id)))
        .execute(&mut conn)
        .map_err(AppError::from)
}
