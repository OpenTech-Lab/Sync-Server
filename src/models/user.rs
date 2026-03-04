use chrono::{DateTime, Utc};
use diesel::prelude::*;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::schema::users;

#[derive(Debug, Clone, Queryable, Selectable, Identifiable)]
#[diesel(table_name = users)]
pub struct User {
    pub id: Uuid,
    pub username: String,
    pub email: String,
    pub password_hash: String,
    pub avatar_base64: Option<String>,
    pub message_public_key: Option<String>,
    pub role: String,
    pub is_active: bool,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub last_seen_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Insertable)]
#[diesel(table_name = users)]
pub struct NewUser {
    pub id: Uuid,
    pub username: String,
    pub email: String,
    pub password_hash: String,
    pub role: String,
}

/// Public representation of a user — never includes the password hash.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UserPublic {
    pub id: Uuid,
    pub username: String,
    pub email: String,
    pub avatar_base64: Option<String>,
    pub role: String,
    pub is_active: bool,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UserProfilePublic {
    pub id: Uuid,
    pub username: String,
    pub avatar_base64: Option<String>,
    pub message_public_key: Option<String>,
}

impl From<User> for UserPublic {
    fn from(u: User) -> Self {
        UserPublic {
            id: u.id,
            username: u.username,
            email: u.email,
            avatar_base64: u.avatar_base64,
            role: u.role,
            is_active: u.is_active,
            created_at: u.created_at,
        }
    }
}

impl From<User> for UserProfilePublic {
    fn from(u: User) -> Self {
        UserProfilePublic {
            id: u.id,
            username: u.username,
            avatar_base64: u.avatar_base64,
            message_public_key: u.message_public_key,
        }
    }
}
