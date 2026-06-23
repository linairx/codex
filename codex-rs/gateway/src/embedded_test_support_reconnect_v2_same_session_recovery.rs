use super::*;

#[path = "embedded_test_support_reconnect_v2_same_session_recovery_turn_and_realtime.rs"]
mod embedded_test_support_reconnect_v2_same_session_recovery_turn_and_realtime;

#[path = "embedded_test_support_reconnect_v2_same_session_recovery_thread_routes.rs"]
mod embedded_test_support_reconnect_v2_same_session_recovery_thread_routes;

#[path = "embedded_test_support_reconnect_v2_same_session_recovery_discovery.rs"]
mod embedded_test_support_reconnect_v2_same_session_recovery_discovery;

#[path = "embedded_test_support_reconnect_v2_same_session_recovery_thread_controls.rs"]
mod embedded_test_support_reconnect_v2_same_session_recovery_thread_controls;

pub(crate) use self::embedded_test_support_reconnect_v2_same_session_recovery_discovery::*;
pub(crate) use self::embedded_test_support_reconnect_v2_same_session_recovery_thread_controls::*;
pub(crate) use self::embedded_test_support_reconnect_v2_same_session_recovery_thread_routes::*;
pub(crate) use self::embedded_test_support_reconnect_v2_same_session_recovery_turn_and_realtime::*;

pub(crate) struct SameSessionRecoveryContext {
    pub(crate) thread_id: &'static str,
    pub(crate) preview: &'static str,
    pub(crate) review_thread_id: String,
    pub(crate) review_turn_id: String,
    pub(crate) turn_id: String,
    pub(crate) rollout_path: String,
    pub(crate) forked_thread_id: String,
    pub(crate) forked_preview: String,
    pub(crate) forked_rollout_path: String,
}

pub(crate) async fn start_reconnecting_v2_multi_connection_thread_server_for_same_session_recovery(
    thread_id: &'static str,
    preview: &'static str,
) -> String {
    let listener = TcpListener::bind("127.0.0.1:0")
        .await
        .expect("listener should bind");
    let addr = listener.local_addr().expect("listener address");
    tokio::spawn(async move {
        let mut connection_index = 0usize;
        loop {
            let (stream, _) = listener.accept().await.expect("accept should succeed");
            let current_connection_index = connection_index;
            connection_index = connection_index.saturating_add(1);
            tokio::spawn(async move {
                let mut websocket = tokio_tungstenite::accept_async(stream)
                    .await
                    .expect("websocket upgrade should succeed");

                expect_remote_initialize(&mut websocket).await;
                match current_connection_index {
                    0 | 2 => {
                        websocket
                            .close(None)
                            .await
                            .expect("close frame should send");
                    }
                    1 => {
                        websocket
                            .close(None)
                            .await
                            .expect("close frame should send");
                    }
                    _ => {
                        let mut thread_name = None::<String>;
                        let forked_preview = format!("{preview}-fork");
                        let context = SameSessionRecoveryContext {
                            thread_id,
                            preview,
                            review_thread_id: format!("{thread_id}-review"),
                            review_turn_id: format!("turn-review-{thread_id}"),
                            turn_id: format!("turn-{thread_id}"),
                            rollout_path: format!("{preview}/rollout.jsonl"),
                            forked_thread_id: format!("{thread_id}-fork"),
                            forked_rollout_path: format!("{forked_preview}/rollout.jsonl"),
                            forked_preview,
                        };
                        loop {
                            let request = read_websocket_request(&mut websocket).await;
                            if handle_same_session_recovery_turn_and_realtime_request(
                                &mut websocket,
                                &context,
                                &request,
                                &mut thread_name,
                            )
                            .await
                                || handle_same_session_recovery_thread_routes_request(
                                    &mut websocket,
                                    &context,
                                    &request,
                                    &mut thread_name,
                                )
                                .await
                                || handle_same_session_recovery_discovery_request(
                                    &mut websocket,
                                    &context,
                                    &request,
                                    &mut thread_name,
                                )
                                .await
                                || handle_same_session_recovery_thread_controls_request(
                                    &mut websocket,
                                    context.thread_id,
                                    context.preview,
                                    &request,
                                    &mut thread_name,
                                    &context.turn_id,
                                )
                                .await
                            {
                                continue;
                            }
                            panic!("unexpected request method: {}", request.method.as_str());
                        }
                    }
                }
            });
        }
    });
    format!("ws://{addr}")
}
