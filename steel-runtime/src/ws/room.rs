use std::collections::{HashMap, HashSet};
use std::sync::Arc;

use tokio::sync::{mpsc, RwLock};

/// A unique identifier for a connected WebSocket session.
pub type SessionId = String;

/// A sender that delivers text frames to a connected client.
pub type SessionSender = mpsc::UnboundedSender<String>;

/// Manages room subscriptions and message routing for a single channel.
///
/// Thread-safe via `Arc<RwLock<...>>` — designed for concurrent access
/// from multiple WebSocket sessions and the Redis pub/sub listener.
#[derive(Clone)]
pub struct RoomManager {
    /// room_name -> set of session IDs subscribed to that room.
    rooms: Arc<RwLock<HashMap<String, HashSet<SessionId>>>>,
    /// session_id -> sender for delivering messages to that session.
    sessions: Arc<RwLock<HashMap<SessionId, SessionSender>>>,
    /// session_id -> set of rooms that session is subscribed to.
    session_rooms: Arc<RwLock<HashMap<SessionId, HashSet<String>>>>,
}

impl RoomManager {
    /// Creates a new empty room manager.
    pub fn new() -> Self {
        Self {
            rooms: Arc::new(RwLock::new(HashMap::new())),
            sessions: Arc::new(RwLock::new(HashMap::new())),
            session_rooms: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// Registers a new session with its message sender.
    pub async fn register_session(&self, session_id: &str, sender: SessionSender) {
        self.sessions
            .write()
            .await
            .insert(session_id.to_string(), sender);
        self.session_rooms
            .write()
            .await
            .insert(session_id.to_string(), HashSet::new());
    }

    /// Removes a session and all its room subscriptions.
    pub async fn remove_session(&self, session_id: &str) {
        self.sessions.write().await.remove(session_id);

        let rooms = self
            .session_rooms
            .write()
            .await
            .remove(session_id)
            .unwrap_or_default();

        let mut room_map = self.rooms.write().await;
        for room in rooms {
            if let Some(members) = room_map.get_mut(&room) {
                members.remove(session_id);
                if members.is_empty() {
                    room_map.remove(&room);
                }
            }
        }
    }

    /// Subscribes a session to a room.
    pub async fn subscribe(&self, session_id: &str, room: &str) {
        self.rooms
            .write()
            .await
            .entry(room.to_string())
            .or_default()
            .insert(session_id.to_string());

        if let Some(session_rooms) = self.session_rooms.write().await.get_mut(session_id) {
            session_rooms.insert(room.to_string());
        }
    }

    /// Unsubscribes a session from a room.
    pub async fn unsubscribe(&self, session_id: &str, room: &str) {
        let mut room_map = self.rooms.write().await;
        if let Some(members) = room_map.get_mut(room) {
            members.remove(session_id);
            if members.is_empty() {
                room_map.remove(room);
            }
        }

        if let Some(session_rooms) = self.session_rooms.write().await.get_mut(session_id) {
            session_rooms.remove(room);
        }
    }

    /// Broadcasts a text message to all sessions subscribed to a room.
    pub async fn broadcast_to_room(&self, room: &str, message: &str) {
        let rooms = self.rooms.read().await;
        let sessions = self.sessions.read().await;

        if let Some(members) = rooms.get(room) {
            for session_id in members {
                if let Some(sender) = sessions.get(session_id) {
                    // Ignore send errors — the session may have disconnected
                    let _ = sender.send(message.to_string());
                }
            }
        }
    }

    /// Returns the number of currently registered sessions.
    pub async fn session_count(&self) -> usize {
        self.sessions.read().await.len()
    }

    /// Returns the number of sessions in a specific room.
    pub async fn room_member_count(&self, room: &str) -> usize {
        self.rooms
            .read()
            .await
            .get(room)
            .map(|s| s.len())
            .unwrap_or(0)
    }
}

impl Default for RoomManager {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn register_and_remove_session() {
        let mgr = RoomManager::new();
        let (tx, _rx) = mpsc::unbounded_channel();

        mgr.register_session("s1", tx).await;
        assert_eq!(mgr.session_count().await, 1);

        mgr.remove_session("s1").await;
        assert_eq!(mgr.session_count().await, 0);
    }

    #[tokio::test]
    async fn subscribe_and_broadcast() {
        let mgr = RoomManager::new();
        let (tx1, mut rx1) = mpsc::unbounded_channel();
        let (tx2, mut rx2) = mpsc::unbounded_channel();

        mgr.register_session("s1", tx1).await;
        mgr.register_session("s2", tx2).await;

        mgr.subscribe("s1", "org:123").await;
        mgr.subscribe("s2", "org:123").await;

        assert_eq!(mgr.room_member_count("org:123").await, 2);

        mgr.broadcast_to_room("org:123", r#"{"hello":"world"}"#)
            .await;

        assert_eq!(rx1.recv().await.unwrap(), r#"{"hello":"world"}"#);
        assert_eq!(rx2.recv().await.unwrap(), r#"{"hello":"world"}"#);
    }

    #[tokio::test]
    async fn unsubscribe_stops_broadcast() {
        let mgr = RoomManager::new();
        let (tx1, mut rx1) = mpsc::unbounded_channel();
        let (tx2, _rx2) = mpsc::unbounded_channel();

        mgr.register_session("s1", tx1).await;
        mgr.register_session("s2", tx2).await;

        mgr.subscribe("s1", "room:a").await;
        mgr.subscribe("s2", "room:a").await;
        mgr.unsubscribe("s2", "room:a").await;

        assert_eq!(mgr.room_member_count("room:a").await, 1);

        mgr.broadcast_to_room("room:a", "msg").await;
        assert_eq!(rx1.recv().await.unwrap(), "msg");
    }

    #[tokio::test]
    async fn remove_session_cleans_up_rooms() {
        let mgr = RoomManager::new();
        let (tx, _rx) = mpsc::unbounded_channel();

        mgr.register_session("s1", tx).await;
        mgr.subscribe("s1", "room:a").await;
        mgr.subscribe("s1", "room:b").await;

        assert_eq!(mgr.room_member_count("room:a").await, 1);
        assert_eq!(mgr.room_member_count("room:b").await, 1);

        mgr.remove_session("s1").await;

        assert_eq!(mgr.room_member_count("room:a").await, 0);
        assert_eq!(mgr.room_member_count("room:b").await, 0);
    }

    #[tokio::test]
    async fn broadcast_to_empty_room() {
        let mgr = RoomManager::new();
        // Should not panic
        mgr.broadcast_to_room("nonexistent", "msg").await;
    }

    #[tokio::test]
    async fn disconnect_during_broadcast() {
        let mgr = RoomManager::new();
        let (tx, rx) = mpsc::unbounded_channel();

        mgr.register_session("s1", tx).await;
        mgr.subscribe("s1", "room:a").await;

        // Drop receiver to simulate disconnect
        drop(rx);

        // Should not panic — just ignore the send error
        mgr.broadcast_to_room("room:a", "msg").await;
    }
}
