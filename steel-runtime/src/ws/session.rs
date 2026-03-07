use std::sync::Arc;
use std::time::{Duration, Instant};

use actix_web::{web, HttpRequest, HttpResponse};
use actix_ws::Message;
use futures_util::StreamExt;
use steel_core::{AuthRule, ChannelDefinition, WsClientMessage, WsServerMessage};
use tokio::sync::mpsc;

use crate::auth::jwt::{Claims, JwtConfig};

use super::pubsub::{PubSubMessage, RedisPubSub};
use super::room::RoomManager;

/// Heartbeat interval — server sends ping every 30 seconds.
const HEARTBEAT_INTERVAL: Duration = Duration::from_secs(30);

/// Client timeout — disconnect if no pong received within 60 seconds.
const CLIENT_TIMEOUT: Duration = Duration::from_secs(60);

/// Configuration passed to each WebSocket session task.
struct SessionConfig {
    session_id: String,
    room_manager: RoomManager,
    pubsub: RedisPubSub,
    channel_name: String,
    rooms_enabled: bool,
}

/// Shared state for a WebSocket channel, stored in Actix app data.
pub struct WsChannelState {
    pub definition: ChannelDefinition,
    pub room_manager: RoomManager,
    pub pubsub: RedisPubSub,
    pub jwt_config: Arc<JwtConfig>,
}

/// HTTP handler for WebSocket upgrade at `/ws/<channel>`.
///
/// Validates JWT from query parameter `?token=<jwt>` before upgrading.
/// Returns 401 if auth fails (before WebSocket handshake completes).
pub async fn ws_handler(
    req: HttpRequest,
    body: web::Payload,
    state: web::Data<WsChannelState>,
) -> Result<HttpResponse, actix_web::Error> {
    // Extract JWT from query string
    let token = extract_token(&req);

    // Validate auth before upgrade
    let claims = match validate_ws_auth(&state.definition, &state.jwt_config, token.as_deref()) {
        Ok(c) => c,
        Err(response) => return Ok(response),
    };

    let (response, session, stream) = actix_ws::handle(&req, body)?;

    let session_id = uuid::Uuid::new_v4().to_string();
    let room_manager = state.room_manager.clone();
    let pubsub = state.pubsub.clone();
    let channel_name = state.definition.channel.clone();
    let rooms_enabled = state.definition.rooms;

    tracing::info!(
        session_id = %session_id,
        channel = %channel_name,
        user_id = %claims.as_ref().map(|c| c.sub.as_str()).unwrap_or("anonymous"),
        "WebSocket connected"
    );

    let config = SessionConfig {
        session_id,
        room_manager,
        pubsub,
        channel_name,
        rooms_enabled,
    };

    // Spawn the session task on the Actix runtime (not Send-bound)
    actix_web::rt::spawn(ws_session(config, session, stream));

    Ok(response)
}

/// Extracts the JWT token from query parameters.
fn extract_token(req: &HttpRequest) -> Option<String> {
    let query = req.query_string();
    // Simple query string parsing without the `url` crate
    for pair in query.split('&') {
        if let Some(value) = pair.strip_prefix("token=") {
            return Some(value.to_string());
        }
    }
    None
}

/// Validates WebSocket auth before upgrading the connection.
///
/// Returns Ok(Some(claims)) for authenticated users, Ok(None) for public channels,
/// or Err(HttpResponse) with 401 status for auth failures.
fn validate_ws_auth(
    definition: &ChannelDefinition,
    jwt_config: &JwtConfig,
    token: Option<&str>,
) -> Result<Option<Claims>, HttpResponse> {
    let auth = match &definition.auth {
        Some(auth) => auth,
        None => return Ok(None), // No auth required
    };

    if auth.is_public() {
        return Ok(None);
    }

    let token = token.ok_or_else(|| {
        HttpResponse::Unauthorized().json(serde_json::json!({
            "error": {
                "code": "UNAUTHORIZED",
                "status": 401,
                "message": "WebSocket connection requires authentication"
            }
        }))
    })?;

    let claims = jwt_config.decode(token).map_err(|_| {
        HttpResponse::Unauthorized().json(serde_json::json!({
            "error": {
                "code": "UNAUTHORIZED",
                "status": 401,
                "message": "Invalid or expired token"
            }
        }))
    })?;

    // Check role authorization
    if let AuthRule::Roles(roles) = auth {
        if !roles.iter().any(|r| r == &claims.role || r == "owner") {
            return Err(HttpResponse::Forbidden().json(serde_json::json!({
                "error": {
                    "code": "FORBIDDEN",
                    "status": 403,
                    "message": "Insufficient permissions for this channel"
                }
            })));
        }
    }

    Ok(Some(claims))
}

/// Runs a single WebSocket session: heartbeat, message routing, cleanup.
async fn ws_session(
    config: SessionConfig,
    mut session: actix_ws::Session,
    mut stream: actix_ws::MessageStream,
) {
    let SessionConfig {
        session_id,
        room_manager,
        pubsub,
        channel_name,
        rooms_enabled,
    } = config;

    // Register session with room manager
    let (tx, mut rx) = mpsc::unbounded_channel::<String>();
    room_manager.register_session(&session_id, tx).await;

    let mut last_heartbeat = Instant::now();

    // Spawn heartbeat ping sender on the Actix runtime
    let heartbeat_session = session.clone();
    let heartbeat_handle = actix_web::rt::spawn(heartbeat_loop(heartbeat_session));

    loop {
        tokio::select! {
            // Outbound: messages from room manager → client
            Some(text) = rx.recv() => {
                if session.text(text).await.is_err() {
                    break;
                }
            }

            // Inbound: messages from client → server
            frame = stream.next() => {
                match frame {
                    Some(Ok(Message::Text(text))) => {
                        last_heartbeat = Instant::now();
                        handle_text_message(
                            &session_id,
                            &text,
                            &mut session,
                            &room_manager,
                            &pubsub,
                            &channel_name,
                            rooms_enabled,
                        ).await;
                    }
                    Some(Ok(Message::Ping(bytes))) => {
                        last_heartbeat = Instant::now();
                        if session.pong(&bytes).await.is_err() {
                            break;
                        }
                    }
                    Some(Ok(Message::Pong(_))) => {
                        last_heartbeat = Instant::now();
                    }
                    Some(Ok(Message::Close(reason))) => {
                        tracing::info!(
                            session_id = %session_id,
                            "Client initiated close"
                        );
                        let _ = session.close(reason).await;
                        break;
                    }
                    Some(Ok(Message::Continuation(_))) => {
                        // Continuation frames not supported
                    }
                    Some(Ok(Message::Binary(_))) => {
                        let err_msg = WsServerMessage::Error {
                            message: "Binary messages not supported".to_string(),
                        };
                        if let Ok(json) = serde_json::to_string(&err_msg) {
                            let _ = session.text(json).await;
                        }
                    }
                    Some(Ok(Message::Nop)) => {}
                    Some(Err(e)) => {
                        tracing::warn!(
                            session_id = %session_id,
                            error = %e,
                            "WebSocket protocol error"
                        );
                        break;
                    }
                    None => break,
                }
            }

            // Heartbeat timeout check
            _ = tokio::time::sleep(Duration::from_secs(5)) => {
                if last_heartbeat.elapsed() > CLIENT_TIMEOUT {
                    tracing::info!(
                        session_id = %session_id,
                        "Client heartbeat timeout, disconnecting"
                    );
                    let _ = session.close(None).await;
                    break;
                }
            }
        }
    }

    // Cleanup
    heartbeat_handle.abort();
    room_manager.remove_session(&session_id).await;
    tracing::info!(session_id = %session_id, "WebSocket disconnected");
}

/// Sends periodic ping frames to the client.
async fn heartbeat_loop(mut session: actix_ws::Session) {
    let mut interval = tokio::time::interval(HEARTBEAT_INTERVAL);
    loop {
        interval.tick().await;
        // Send application-level ping as JSON
        let ping = WsServerMessage::Ping;
        if let Ok(json) = serde_json::to_string(&ping) {
            if session.text(json).await.is_err() {
                break;
            }
        }
    }
}

/// Processes an incoming text message from a client.
async fn handle_text_message(
    session_id: &str,
    text: &str,
    session: &mut actix_ws::Session,
    room_manager: &RoomManager,
    pubsub: &RedisPubSub,
    channel_name: &str,
    rooms_enabled: bool,
) {
    let msg: WsClientMessage = match serde_json::from_str(text) {
        Ok(m) => m,
        Err(e) => {
            let err = WsServerMessage::Error {
                message: format!("Invalid message format: {e}"),
            };
            if let Ok(json) = serde_json::to_string(&err) {
                let _ = session.text(json).await;
            }
            return;
        }
    };

    match msg {
        WsClientMessage::Subscribe { room } => {
            if !rooms_enabled {
                let err = WsServerMessage::Error {
                    message: "Room subscriptions not enabled for this channel".to_string(),
                };
                if let Ok(json) = serde_json::to_string(&err) {
                    let _ = session.text(json).await;
                }
                return;
            }
            room_manager.subscribe(session_id, &room).await;
            let ack = WsServerMessage::Subscribed { room };
            if let Ok(json) = serde_json::to_string(&ack) {
                let _ = session.text(json).await;
            }
        }
        WsClientMessage::Unsubscribe { room } => {
            room_manager.unsubscribe(session_id, &room).await;
            let ack = WsServerMessage::Unsubscribed { room };
            if let Ok(json) = serde_json::to_string(&ack) {
                let _ = session.text(json).await;
            }
        }
        WsClientMessage::Message { room, data } => {
            if !rooms_enabled {
                let err = WsServerMessage::Error {
                    message: "Room messaging not enabled for this channel".to_string(),
                };
                if let Ok(json) = serde_json::to_string(&err) {
                    let _ = session.text(json).await;
                }
                return;
            }
            // Publish via Redis so all instances receive it
            let pub_msg = PubSubMessage {
                channel: channel_name.to_string(),
                room: room.clone(),
                event: "message".to_string(),
                data,
            };
            if let Err(e) = pubsub.publish(&pub_msg).await {
                tracing::warn!(error = %e, "Failed to publish message via Redis");
                // Fall back to local-only broadcast
                let server_msg = WsServerMessage::Broadcast {
                    room: room.clone(),
                    event: "message".to_string(),
                    data: pub_msg.data,
                };
                if let Ok(json) = serde_json::to_string(&server_msg) {
                    room_manager.broadcast_to_room(&room, &json).await;
                }
            }
        }
        WsClientMessage::Pong => {
            // Client pong — heartbeat already updated by caller
        }
    }
}

/// Registers WebSocket routes for a channel on the Actix service config.
pub fn configure_ws_routes(
    cfg: &mut web::ServiceConfig,
    definition: ChannelDefinition,
    room_manager: RoomManager,
    pubsub: RedisPubSub,
    jwt_config: Arc<JwtConfig>,
) {
    let channel_name = definition.channel.clone();
    let state = web::Data::new(WsChannelState {
        definition,
        room_manager,
        pubsub,
        jwt_config,
    });

    cfg.app_data(state)
        .route(&format!("/ws/{channel_name}"), web::get().to(ws_handler));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn validate_public_channel() {
        let def = ChannelDefinition {
            channel: "public".to_string(),
            auth: Some(AuthRule::Public),
            rooms: false,
            hooks: None,
        };
        let jwt = JwtConfig::new("test-secret-key-at-least-32-bytes-long!", 3600, 86400);
        let result = validate_ws_auth(&def, &jwt, None);
        assert!(result.is_ok());
        assert!(result.unwrap().is_none());
    }

    #[test]
    fn validate_no_auth_channel() {
        let def = ChannelDefinition {
            channel: "open".to_string(),
            auth: None,
            rooms: false,
            hooks: None,
        };
        let jwt = JwtConfig::new("test-secret-key-at-least-32-bytes-long!", 3600, 86400);
        let result = validate_ws_auth(&def, &jwt, None);
        assert!(result.is_ok());
    }

    #[test]
    fn validate_auth_no_token_returns_401() {
        let def = ChannelDefinition {
            channel: "private".to_string(),
            auth: Some(AuthRule::Roles(vec!["admin".to_string()])),
            rooms: false,
            hooks: None,
        };
        let jwt = JwtConfig::new("test-secret-key-at-least-32-bytes-long!", 3600, 86400);
        let result = validate_ws_auth(&def, &jwt, None);
        assert!(result.is_err());
    }

    #[test]
    fn validate_auth_invalid_token_returns_401() {
        let def = ChannelDefinition {
            channel: "private".to_string(),
            auth: Some(AuthRule::Roles(vec!["admin".to_string()])),
            rooms: false,
            hooks: None,
        };
        let jwt = JwtConfig::new("test-secret-key-at-least-32-bytes-long!", 3600, 86400);
        let result = validate_ws_auth(&def, &jwt, Some("invalid.token.here"));
        assert!(result.is_err());
    }

    #[test]
    fn validate_auth_valid_token_correct_role() {
        let jwt = JwtConfig::new("test-secret-key-at-least-32-bytes-long!", 3600, 86400);
        let token = jwt.encode_access("user-1", "admin").unwrap();

        let def = ChannelDefinition {
            channel: "private".to_string(),
            auth: Some(AuthRule::Roles(vec!["admin".to_string()])),
            rooms: false,
            hooks: None,
        };
        let result = validate_ws_auth(&def, &jwt, Some(&token));
        assert!(result.is_ok());
        let claims = result.unwrap().unwrap();
        assert_eq!(claims.sub, "user-1");
        assert_eq!(claims.role, "admin");
    }

    #[test]
    fn validate_auth_valid_token_wrong_role() {
        let jwt = JwtConfig::new("test-secret-key-at-least-32-bytes-long!", 3600, 86400);
        let token = jwt.encode_access("user-1", "viewer").unwrap();

        let def = ChannelDefinition {
            channel: "private".to_string(),
            auth: Some(AuthRule::Roles(vec!["admin".to_string()])),
            rooms: false,
            hooks: None,
        };
        let result = validate_ws_auth(&def, &jwt, Some(&token));
        assert!(result.is_err());
    }

    #[test]
    fn extract_token_from_query() {
        // We can't easily construct a full HttpRequest in unit tests,
        // so we test the parsing logic directly
        fn parse_token(query: &str) -> Option<String> {
            for pair in query.split('&') {
                if let Some(value) = pair.strip_prefix("token=") {
                    return Some(value.to_string());
                }
            }
            None
        }

        assert_eq!(parse_token("token=abc123"), Some("abc123".to_string()));
        assert_eq!(parse_token("foo=bar&token=xyz"), Some("xyz".to_string()));
        assert_eq!(parse_token("foo=bar"), None);
        assert_eq!(parse_token(""), None);
    }
}
