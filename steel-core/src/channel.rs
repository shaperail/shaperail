use serde::{Deserialize, Serialize};

use crate::AuthRule;

/// Definition of a WebSocket channel, parsed from a `.channel.yaml` file.
///
/// ```yaml
/// channel: notifications
/// auth: [member, admin]
/// rooms: true
/// hooks:
///   on_connect: [log_connect]
///   on_disconnect: [log_disconnect]
///   on_message: [validate_message]
/// ```
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ChannelDefinition {
    /// Channel name (e.g., "notifications").
    pub channel: String,

    /// Authentication rule for connecting to this channel.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub auth: Option<AuthRule>,

    /// Whether this channel supports room subscriptions.
    #[serde(default)]
    pub rooms: bool,

    /// Lifecycle hook configuration.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub hooks: Option<ChannelHooks>,
}

/// Lifecycle hooks for a WebSocket channel.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ChannelHooks {
    /// Hooks executed when a client connects.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub on_connect: Option<Vec<String>>,

    /// Hooks executed when a client disconnects.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub on_disconnect: Option<Vec<String>>,

    /// Hooks executed when a client sends a message.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub on_message: Option<Vec<String>>,
}

/// Client-to-server WebSocket message format.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "action", rename_all = "lowercase")]
pub enum WsClientMessage {
    /// Subscribe to a room within the channel.
    Subscribe { room: String },
    /// Unsubscribe from a room.
    Unsubscribe { room: String },
    /// Send a message to a room.
    Message {
        room: String,
        data: serde_json::Value,
    },
    /// Respond to server ping (client pong).
    Pong,
}

/// Server-to-client WebSocket message format.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "lowercase")]
pub enum WsServerMessage {
    /// Broadcast data to subscribed clients.
    Broadcast {
        room: String,
        event: String,
        data: serde_json::Value,
    },
    /// Acknowledgement of subscription.
    Subscribed { room: String },
    /// Acknowledgement of unsubscription.
    Unsubscribed { room: String },
    /// Error message.
    Error { message: String },
    /// Server ping for heartbeat.
    Ping,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn channel_definition_minimal() {
        let yaml = r#"{"channel": "notifications"}"#;
        let cd: ChannelDefinition = serde_json::from_str(yaml).unwrap();
        assert_eq!(cd.channel, "notifications");
        assert!(cd.auth.is_none());
        assert!(!cd.rooms);
        assert!(cd.hooks.is_none());
    }

    #[test]
    fn channel_definition_full() {
        let json = r#"{
            "channel": "updates",
            "auth": ["member", "admin"],
            "rooms": true,
            "hooks": {
                "on_connect": ["log_connect"],
                "on_disconnect": ["log_disconnect"],
                "on_message": ["validate_message"]
            }
        }"#;
        let cd: ChannelDefinition = serde_json::from_str(json).unwrap();
        assert_eq!(cd.channel, "updates");
        assert!(cd.rooms);
        let hooks = cd.hooks.as_ref().unwrap();
        assert_eq!(hooks.on_connect.as_ref().unwrap(), &["log_connect"]);
        assert_eq!(hooks.on_disconnect.as_ref().unwrap(), &["log_disconnect"]);
        assert_eq!(hooks.on_message.as_ref().unwrap(), &["validate_message"]);
    }

    #[test]
    fn channel_definition_serde_roundtrip() {
        let cd = ChannelDefinition {
            channel: "chat".to_string(),
            auth: Some(AuthRule::Roles(vec!["member".to_string()])),
            rooms: true,
            hooks: Some(ChannelHooks {
                on_connect: Some(vec!["log_connect".to_string()]),
                on_disconnect: None,
                on_message: None,
            }),
        };
        let json = serde_json::to_string(&cd).unwrap();
        let back: ChannelDefinition = serde_json::from_str(&json).unwrap();
        assert_eq!(cd, back);
    }

    #[test]
    fn ws_client_message_subscribe() {
        let json = r#"{"action": "subscribe", "room": "org:123"}"#;
        let msg: WsClientMessage = serde_json::from_str(json).unwrap();
        match msg {
            WsClientMessage::Subscribe { room } => assert_eq!(room, "org:123"),
            _ => panic!("Expected Subscribe"),
        }
    }

    #[test]
    fn ws_client_message_unsubscribe() {
        let json = r#"{"action": "unsubscribe", "room": "org:123"}"#;
        let msg: WsClientMessage = serde_json::from_str(json).unwrap();
        match msg {
            WsClientMessage::Unsubscribe { room } => assert_eq!(room, "org:123"),
            _ => panic!("Expected Unsubscribe"),
        }
    }

    #[test]
    fn ws_client_message_message() {
        let json = r#"{"action": "message", "room": "org:123", "data": {"text": "hello"}}"#;
        let msg: WsClientMessage = serde_json::from_str(json).unwrap();
        match msg {
            WsClientMessage::Message { room, data } => {
                assert_eq!(room, "org:123");
                assert_eq!(data["text"], "hello");
            }
            _ => panic!("Expected Message"),
        }
    }

    #[test]
    fn ws_client_message_pong() {
        let json = r#"{"action": "pong"}"#;
        let msg: WsClientMessage = serde_json::from_str(json).unwrap();
        assert!(matches!(msg, WsClientMessage::Pong));
    }

    #[test]
    fn ws_server_message_broadcast() {
        let msg = WsServerMessage::Broadcast {
            room: "org:123".to_string(),
            event: "user.created".to_string(),
            data: serde_json::json!({"id": "abc"}),
        };
        let json = serde_json::to_string(&msg).unwrap();
        assert!(json.contains("\"type\":\"broadcast\""));
        assert!(json.contains("\"room\":\"org:123\""));
    }

    #[test]
    fn ws_server_message_subscribed() {
        let msg = WsServerMessage::Subscribed {
            room: "org:123".to_string(),
        };
        let json = serde_json::to_string(&msg).unwrap();
        assert!(json.contains("\"type\":\"subscribed\""));
    }

    #[test]
    fn ws_server_message_error() {
        let msg = WsServerMessage::Error {
            message: "bad request".to_string(),
        };
        let json = serde_json::to_string(&msg).unwrap();
        assert!(json.contains("\"type\":\"error\""));
        assert!(json.contains("bad request"));
    }

    #[test]
    fn ws_server_message_ping() {
        let msg = WsServerMessage::Ping;
        let json = serde_json::to_string(&msg).unwrap();
        assert!(json.contains("\"type\":\"ping\""));
    }
}
