use super::*;

#[path = "embedded_test_support_reconnect_v2_same_session_recovery_turn.rs"]
mod embedded_test_support_reconnect_v2_same_session_recovery_turn;

#[path = "embedded_test_support_reconnect_v2_same_session_recovery_realtime.rs"]
mod embedded_test_support_reconnect_v2_same_session_recovery_realtime;

pub(crate) use self::embedded_test_support_reconnect_v2_same_session_recovery_realtime::*;
pub(crate) use self::embedded_test_support_reconnect_v2_same_session_recovery_turn::*;

pub(crate) async fn handle_same_session_recovery_turn_and_realtime_request(
    websocket: &mut tokio_tungstenite::WebSocketStream<tokio::net::TcpStream>,
    context: &SameSessionRecoveryContext,
    request: &codex_app_server_protocol::JSONRPCRequest,
    _thread_name: &mut Option<String>,
) -> bool {
    if handle_same_session_recovery_turn_request(
        websocket,
        context.thread_id,
        request,
        &context.turn_id,
    )
    .await
    {
        return true;
    }

    handle_same_session_recovery_realtime_request(websocket, context.thread_id, request).await
}
