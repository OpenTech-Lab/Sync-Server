use chrono::{DateTime, Utc};
use diesel::prelude::*;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::schema::server_news;

#[derive(Debug, Clone, Queryable, Selectable, Identifiable, Serialize, Deserialize)]
#[diesel(table_name = server_news)]
pub struct ServerNews {
    pub id: Uuid,
    pub title: String,
    pub summary: Option<String>,
    pub markdown_content: String,
    pub created_by: Option<Uuid>,
    pub published_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Insertable)]
#[diesel(table_name = server_news)]
pub struct NewServerNews {
    pub id: Uuid,
    pub title: String,
    pub summary: Option<String>,
    pub markdown_content: String,
    pub created_by: Option<Uuid>,
}
