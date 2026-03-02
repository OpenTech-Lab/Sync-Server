use anyhow::Result;
use redis::aio::PubSub;
use redis::AsyncCommands;
use serde::Serialize;

/// Obtain an async multiplexed connection from the client.
pub async fn get_async_conn(client: &redis::Client) -> Result<redis::aio::MultiplexedConnection> {
    client
        .get_multiplexed_async_connection()
        .await
        .map_err(|e| anyhow::anyhow!("Redis connection error: {}", e))
}

/// Publish a serialisable event to a Redis channel.
pub async fn publish<T: Serialize>(
    conn: &mut redis::aio::MultiplexedConnection,
    channel: &str,
    payload: &T,
) -> Result<()> {
    let json = serde_json::to_string(payload)?;
    conn.publish::<_, _, ()>(channel, json).await?;
    Ok(())
}

/// Subscribe to one or more channels and return the `PubSub` handle.
pub async fn subscribe(client: &redis::Client, channels: &[&str]) -> Result<PubSub> {
    let mut pubsub = client
        .get_async_pubsub()
        .await
        .map_err(|e| anyhow::anyhow!("Redis pubsub connect error: {}", e))?;
    for &ch in channels {
        pubsub
            .subscribe(ch)
            .await
            .map_err(|e| anyhow::anyhow!("Subscribe error: {}", e))?;
    }
    Ok(pubsub)
}

/// Channel name for a user's WebSocket mailbox.
pub fn user_channel(user_id: uuid::Uuid) -> String {
    format!("ws:user:{}", user_id)
}
