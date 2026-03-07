use std::sync::Arc;

use futures_util::StreamExt;
use redis::AsyncCommands;
use serde::{Deserialize, Serialize};
use steel_core::SteelError;

use super::room::RoomManager;

/// A broadcast message published through Redis pub/sub.
///
/// Serialized to JSON for transport across server instances.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PubSubMessage {
    /// The channel name (matches ChannelDefinition.channel).
    pub channel: String,
    /// The room within the channel.
    pub room: String,
    /// The event name (e.g., "user.created").
    pub event: String,
    /// The event payload.
    pub data: serde_json::Value,
}

/// Redis pub/sub backend for cross-instance WebSocket broadcast.
///
/// When a server instance needs to broadcast to a room, it publishes
/// to a Redis channel. All instances subscribe to these channels and
/// route messages to their locally connected clients.
#[derive(Clone)]
pub struct RedisPubSub {
    pool: Arc<deadpool_redis::Pool>,
}

impl RedisPubSub {
    /// Creates a new Redis pub/sub backend.
    pub fn new(pool: Arc<deadpool_redis::Pool>) -> Self {
        Self { pool }
    }

    /// Returns the Redis channel name for a given WebSocket channel.
    fn redis_channel(channel: &str) -> String {
        format!("steel:ws:{channel}")
    }

    /// Publishes a broadcast message to Redis so all instances receive it.
    pub async fn publish(&self, msg: &PubSubMessage) -> Result<(), SteelError> {
        let mut conn = self
            .pool
            .get()
            .await
            .map_err(|e| SteelError::Internal(format!("Redis connection failed: {e}")))?;

        let payload = serde_json::to_string(msg).map_err(|e| {
            SteelError::Internal(format!("Failed to serialize pub/sub message: {e}"))
        })?;

        let redis_channel = Self::redis_channel(&msg.channel);
        conn.publish::<_, _, ()>(&redis_channel, &payload)
            .await
            .map_err(|e| SteelError::Internal(format!("Redis publish failed: {e}")))?;

        tracing::debug!(
            channel = %msg.channel,
            room = %msg.room,
            event = %msg.event,
            "Published broadcast via Redis pub/sub"
        );

        Ok(())
    }

    /// Starts a subscriber task that listens on Redis pub/sub and routes
    /// messages to the local room manager.
    ///
    /// This spawns a background Tokio task that runs until the returned
    /// `tokio::task::JoinHandle` is aborted.
    pub fn start_subscriber(
        &self,
        channel_name: &str,
        room_manager: RoomManager,
        redis_url: &str,
    ) -> tokio::task::JoinHandle<()> {
        let redis_channel = Self::redis_channel(channel_name);
        let redis_url = redis_url.to_string();

        tokio::spawn(async move {
            if let Err(e) = Self::subscriber_loop(&redis_url, &redis_channel, &room_manager).await {
                tracing::error!(
                    error = %e,
                    channel = %redis_channel,
                    "Redis subscriber exited with error"
                );
            }
        })
    }

    /// Internal subscriber loop — connects to Redis and processes messages.
    async fn subscriber_loop(
        redis_url: &str,
        redis_channel: &str,
        room_manager: &RoomManager,
    ) -> Result<(), SteelError> {
        let client = redis::Client::open(redis_url)
            .map_err(|e| SteelError::Internal(format!("Redis client creation failed: {e}")))?;

        let mut pubsub_conn = client
            .get_async_pubsub()
            .await
            .map_err(|e| SteelError::Internal(format!("Redis pub/sub connection failed: {e}")))?;

        pubsub_conn
            .subscribe(redis_channel)
            .await
            .map_err(|e| SteelError::Internal(format!("Redis subscribe failed: {e}")))?;

        tracing::info!(channel = %redis_channel, "Redis pub/sub subscriber started");

        // Use into_on_message to consume the PubSub and get an owned stream
        let mut msg_stream = pubsub_conn.into_on_message();

        while let Some(msg) = msg_stream.next().await {
            let payload: String = match msg.get_payload() {
                Ok(p) => p,
                Err(e) => {
                    tracing::warn!(error = %e, "Failed to get pub/sub payload");
                    continue;
                }
            };

            let broadcast: PubSubMessage = match serde_json::from_str(&payload) {
                Ok(m) => m,
                Err(e) => {
                    tracing::warn!(error = %e, "Failed to parse pub/sub message");
                    continue;
                }
            };

            // Build server message and broadcast to local clients
            let server_msg = steel_core::WsServerMessage::Broadcast {
                room: broadcast.room.clone(),
                event: broadcast.event,
                data: broadcast.data,
            };

            if let Ok(text) = serde_json::to_string(&server_msg) {
                room_manager.broadcast_to_room(&broadcast.room, &text).await;
            }
        }

        tracing::warn!(channel = %redis_channel, "Redis pub/sub stream ended");
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn redis_channel_name() {
        assert_eq!(
            RedisPubSub::redis_channel("notifications"),
            "steel:ws:notifications"
        );
    }

    #[test]
    fn pubsub_message_serde_roundtrip() {
        let msg = PubSubMessage {
            channel: "notifications".to_string(),
            room: "org:123".to_string(),
            event: "user.created".to_string(),
            data: serde_json::json!({"id": "abc"}),
        };
        let json = serde_json::to_string(&msg).unwrap();
        let back: PubSubMessage = serde_json::from_str(&json).unwrap();
        assert_eq!(back.channel, "notifications");
        assert_eq!(back.room, "org:123");
        assert_eq!(back.event, "user.created");
    }
}
