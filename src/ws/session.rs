use actix_ws::{Message, MessageStream, Session};
use futures_util::StreamExt;
use std::time::{Duration, Instant};
use tokio::sync::mpsc;
use uuid::Uuid;

/// How often we check whether the client is still responsive.
const HEARTBEAT_INTERVAL: Duration = Duration::from_secs(25);
/// How long the client may be silent before we drop the connection.
const CLIENT_TIMEOUT: Duration = Duration::from_secs(60);

// ── Event enums (JSON-tagged) ─────────────────────────────────────────────────

/// Events sent from the server to the WebSocket client.
#[derive(Debug, serde::Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ServerEvent {
    Pong,
    #[allow(dead_code)]
    Error {
        message: String,
    },
}

/// Events received from the WebSocket client.
#[derive(Debug, serde::Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ClientEvent {
    Ping,
}

// ── Session driver ────────────────────────────────────────────────────────────

/// Drive a WebSocket session for `user_id`.
///
/// Spawns a Redis subscriber task that forwards incoming pubsub messages over
/// an mpsc channel, then loops over three arms:
///   1. WS messages from the client (ping/close/text)
///   2. Redis events forwarded by the subscriber task
///   3. Heartbeat tick — disconnect if no client message in `CLIENT_TIMEOUT`
pub async fn run_ws_session(
    user_id: Uuid,
    mut session: Session,
    mut msg_stream: MessageStream,
    redis_client: redis::Client,
) {
    let (redis_tx, mut redis_rx) = mpsc::unbounded_channel::<String>();

    // ── Spawn Redis subscriber ────────────────────────────────────────────────
    let channel = crate::services::redis_pubsub::user_channel(user_id);
    tokio::spawn(async move {
        match crate::services::redis_pubsub::subscribe(&redis_client, &[channel.as_str()]).await {
            Ok(mut pubsub) => {
                let mut stream = pubsub.on_message();
                while let Some(msg) = stream.next().await {
                    let payload: String = msg.get_payload().unwrap_or_default();
                    if redis_tx.send(payload).is_err() {
                        break; // receiver (session) has closed
                    }
                }
            }
            Err(e) => {
                tracing::error!(
                    error = %e,
                    %user_id,
                    "Redis subscribe failed; real-time delivery disabled for this session"
                );
            }
        }
    });

    // ── Session loop ──────────────────────────────────────────────────────────
    let mut last_heartbeat = Instant::now();
    let mut heartbeat_interval = tokio::time::interval(HEARTBEAT_INTERVAL);

    loop {
        tokio::select! {
            // Arm 1: incoming WebSocket frame from the client
            Some(msg) = msg_stream.next() => {
                match msg {
                    Ok(Message::Text(text)) => {
                        last_heartbeat = Instant::now();
                        if let Ok(ClientEvent::Ping) = serde_json::from_str::<ClientEvent>(&text) {
                            let pong = serde_json::to_string(&ServerEvent::Pong)
                                .unwrap_or_else(|_| r#"{"type":"pong"}"#.to_string());
                            if session.text(pong).await.is_err() {
                                break;
                            }
                        }
                    }
                    Ok(Message::Ping(bytes)) => {
                        last_heartbeat = Instant::now();
                        if session.pong(&bytes).await.is_err() {
                            break;
                        }
                    }
                    Ok(Message::Close(_)) | Err(_) => break,
                    _ => {} // ignore Binary and Continuation frames
                }
            }

            // Arm 2: event pushed from Redis pubsub subscriber
            Some(payload) = redis_rx.recv() => {
                if session.text(payload).await.is_err() {
                    break;
                }
            }

            // Arm 3: heartbeat tick — enforce CLIENT_TIMEOUT
            _ = heartbeat_interval.tick() => {
                if last_heartbeat.elapsed() > CLIENT_TIMEOUT {
                    tracing::info!(%user_id, "WS session timed out, closing");
                    break;
                }
            }
        }
    }

    let _ = session.close(None).await;
    tracing::debug!(%user_id, "WS session closed");
}
