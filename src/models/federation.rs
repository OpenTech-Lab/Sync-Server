use chrono::{DateTime, Utc};
use diesel::prelude::*;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use uuid::Uuid;

use crate::schema::{
    federation_actor_keys, federation_deliveries, federation_inbox_activities,
    federation_remote_messages,
};

#[derive(Debug, Clone, Queryable, Selectable, Identifiable)]
#[diesel(table_name = federation_actor_keys)]
pub struct FederationActorKey {
    pub id: Uuid,
    pub actor_username: String,
    pub key_id: String,
    pub public_key_pem: String,
    pub private_key_pkcs8: String,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Insertable)]
#[diesel(table_name = federation_actor_keys)]
pub struct NewFederationActorKey {
    pub id: Uuid,
    pub actor_username: String,
    pub key_id: String,
    pub public_key_pem: String,
    pub private_key_pkcs8: String,
}

#[derive(Debug, Clone, Queryable, Selectable, Identifiable)]
#[diesel(table_name = federation_inbox_activities)]
pub struct FederationInboxActivity {
    pub id: Uuid,
    pub activity_id: String,
    pub actor: String,
    pub recipient_username: String,
    pub activity_type: String,
    pub payload: Value,
    pub received_at: DateTime<Utc>,
}

#[derive(Debug, Insertable)]
#[diesel(table_name = federation_inbox_activities)]
pub struct NewFederationInboxActivity {
    pub id: Uuid,
    pub activity_id: String,
    pub actor: String,
    pub recipient_username: String,
    pub activity_type: String,
    pub payload: Value,
}

#[derive(Debug, Clone, Queryable, Selectable, Identifiable, Serialize, Deserialize)]
#[diesel(table_name = federation_deliveries)]
pub struct FederationDelivery {
    pub id: Uuid,
    pub activity_id: String,
    pub sender_username: String,
    pub destination: String,
    pub status: String,
    pub attempts: i32,
    pub last_error: Option<String>,
    pub next_attempt_at: Option<DateTime<Utc>>,
    pub delivered_at: Option<DateTime<Utc>>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Insertable)]
#[diesel(table_name = federation_deliveries)]
pub struct NewFederationDelivery {
    pub id: Uuid,
    pub activity_id: String,
    pub sender_username: String,
    pub destination: String,
    pub status: String,
}

#[derive(Debug, Clone, Queryable, Selectable, Identifiable)]
#[diesel(table_name = federation_remote_messages)]
pub struct FederationRemoteMessage {
    pub id: Uuid,
    pub activity_id: String,
    pub object_id: Option<String>,
    pub actor: String,
    pub recipient_username: String,
    pub content: String,
    pub received_at: DateTime<Utc>,
}

#[derive(Debug, Insertable)]
#[diesel(table_name = federation_remote_messages)]
pub struct NewFederationRemoteMessage {
    pub id: Uuid,
    pub activity_id: String,
    pub object_id: Option<String>,
    pub actor: String,
    pub recipient_username: String,
    pub content: String,
}
