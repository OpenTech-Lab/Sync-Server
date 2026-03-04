use chrono::{DateTime, Utc};
use diesel::prelude::*;
use serde::Serialize;
use uuid::Uuid;

use crate::schema::encrypted_backups;

#[derive(Debug, Clone, Queryable, Selectable, Identifiable, Associations, Serialize)]
#[diesel(table_name = encrypted_backups, primary_key(user_id), belongs_to(crate::models::user::User, foreign_key = user_id))]
pub struct EncryptedBackup {
    pub user_id: Uuid,
    pub encrypted_blob: String,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Insertable)]
#[diesel(table_name = encrypted_backups)]
pub struct NewEncryptedBackup {
    pub user_id: Uuid,
    pub encrypted_blob: String,
}
