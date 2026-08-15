use chrono::{DateTime, Utc};
use diesel::prelude::*;
use uuid::Uuid;

use crate::schema::agent_tokens;

#[derive(Debug, Clone, Queryable, Selectable, Identifiable)]
#[diesel(table_name = agent_tokens)]
pub struct AgentToken {
    pub id: Uuid,
    pub name: String,
    pub token_hash: String,
    pub token_prefix: String,
    pub created_by: Uuid,
    pub created_at: DateTime<Utc>,
    pub expires_at: Option<DateTime<Utc>>,
    pub last_used_at: Option<DateTime<Utc>>,
    pub revoked_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Insertable)]
#[diesel(table_name = agent_tokens)]
pub struct NewAgentToken {
    pub id: Uuid,
    pub name: String,
    pub token_hash: String,
    pub token_prefix: String,
    pub created_by: Uuid,
    pub expires_at: Option<DateTime<Utc>>,
}
