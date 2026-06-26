use super::*;
use pretty_assertions::assert_eq;

pub(crate) async fn start_mock_remote_server_for_passthrough_request_with_optional_params_and_result(
    expected_method: &'static str,
    expected_params: Option<serde_json::Value>,
    response_result: serde_json::Value,
) -> String {
    let listener = TcpListener::bind("127.0.0.1:0")
        .await
        .expect("listener should bind");
    let addr = listener.local_addr().expect("listener address");
    tokio::spawn(async move {
        let (stream, _) = listener.accept().await.expect("accept should succeed");
        let mut websocket = tokio_tungstenite::accept_async(stream)
            .await
            .expect("websocket should accept");

        expect_remote_initialize(&mut websocket).await;

        let request = read_websocket_request(&mut websocket).await;
        assert_eq!(request.method, expected_method);
        assert_json_params_eq(request.params, expected_params);

        websocket
            .send(Message::Text(
                serde_json::to_string(&JSONRPCMessage::Response(JSONRPCResponse {
                    id: request.id,
                    result: response_result,
                }))
                .expect("response should serialize")
                .into(),
            ))
            .await
            .expect("response should send");

        tokio::time::sleep(Duration::from_millis(100)).await;
    });
    format!("ws://{addr}")
}

pub(crate) fn pending_command_exec_params(process_id: &str) -> serde_json::Value {
    serde_json::json!({
        "command": ["sh", "-lc", "sleep 1"],
        "processId": process_id,
        "tty": true,
        "streamStdin": true,
        "streamStdoutStderr": true,
        "outputBytesCap": null,
        "timeoutMs": null,
        "cwd": null,
        "env": null,
        "size": {
            "rows": 24,
            "cols": 80,
        },
        "sandboxPolicy": null,
    })
}

pub(crate) async fn start_mock_remote_server_for_pending_command_exec(
    first_request_observed_tx: oneshot::Sender<()>,
    finish_first_request_rx: oneshot::Receiver<()>,
) -> String {
    let listener = TcpListener::bind("127.0.0.1:0")
        .await
        .expect("listener should bind");
    let addr = listener.local_addr().expect("listener address");
    tokio::spawn(async move {
        let (stream, _) = listener.accept().await.expect("accept should succeed");
        let mut websocket = tokio_tungstenite::accept_async(stream)
            .await
            .expect("websocket should accept");

        expect_remote_initialize(&mut websocket).await;

        let request = read_websocket_request(&mut websocket).await;
        assert_eq!(request.method, "command/exec");
        assert_json_params_eq(
            request.params,
            Some(pending_command_exec_params("proc-pending-1")),
        );
        first_request_observed_tx
            .send(())
            .expect("first command/exec observation should send");
        finish_first_request_rx
            .await
            .expect("first command/exec completion signal should arrive");

        websocket
            .send(Message::Text(
                serde_json::to_string(&JSONRPCMessage::Response(JSONRPCResponse {
                    id: request.id,
                    result: serde_json::json!({
                        "exitCode": 0,
                        "stdout": "",
                        "stderr": "",
                    }),
                }))
                .expect("command/exec response should serialize")
                .into(),
            ))
            .await
            .expect("command/exec response should send");

        tokio::time::sleep(Duration::from_millis(100)).await;
    });
    format!("ws://{addr}")
}
