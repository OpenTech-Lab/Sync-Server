use uuid::Uuid;

use crate::services::redis_pubsub;

/// Publish a JSON event to a specific user's WebSocket mailbox channel.
///
/// Failures are logged but not propagated — delivery via WebSocket is
/// best-effort; the message is already persisted in the DB.
#[allow(dead_code)]
pub async fn publish_to_user(
    redis: &redis::Client,
    user_id: Uuid,
    event: &impl serde::Serialize,
) -> anyhow::Result<()> {
    let mut conn = redis_pubsub::get_async_conn(redis).await?;
    let channel = redis_pubsub::user_channel(user_id);
    redis_pubsub::publish(&mut conn, &channel, event).await
}
