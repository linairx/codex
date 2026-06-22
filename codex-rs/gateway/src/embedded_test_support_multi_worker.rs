use super::*;

#[path = "embedded_test_support_multi_worker_bootstrap.rs"]
mod embedded_test_support_multi_worker_bootstrap;

pub(crate) use self::embedded_test_support_multi_worker_bootstrap::*;

#[path = "embedded_test_support_multi_worker_notifications.rs"]
mod embedded_test_support_multi_worker_notifications;

#[allow(unused_imports)]
pub(crate) use self::embedded_test_support_multi_worker_notifications::*;

#[path = "embedded_test_support_multi_worker_server_requests.rs"]
mod embedded_test_support_multi_worker_server_requests;

#[allow(unused_imports)]
pub(crate) use self::embedded_test_support_multi_worker_server_requests::*;

#[path = "embedded_test_support_multi_worker_filesystem.rs"]
mod embedded_test_support_multi_worker_filesystem;

#[allow(unused_imports)]
pub(crate) use self::embedded_test_support_multi_worker_filesystem::*;

#[path = "embedded_test_support_multi_worker_reconnect_bootstrap.rs"]
mod embedded_test_support_multi_worker_reconnect_bootstrap;

#[allow(unused_imports)]
pub(crate) use self::embedded_test_support_multi_worker_reconnect_bootstrap::*;

pub(crate) async fn start_mock_remote_multi_connection_apps_list_server(
    apps: Vec<(&'static str, &'static str)>,
) -> String {
    let listener = TcpListener::bind("127.0.0.1:0")
        .await
        .expect("listener should bind");
    let addr = listener.local_addr().expect("listener address");
    tokio::spawn(async move {
        loop {
            let (stream, _) = listener.accept().await.expect("accept should succeed");
            let apps = apps.clone();
            tokio::spawn(async move {
                let mut websocket = tokio_tungstenite::accept_async(stream)
                    .await
                    .expect("websocket upgrade should succeed");
                expect_remote_initialize(&mut websocket).await;

                loop {
                    let request = read_websocket_request(&mut websocket).await;
                    let result = match request.method.as_str() {
                        "app/list" => paginated_apps_list_result(&request, &apps),
                        method => panic!("unexpected request method: {method}"),
                    };
                    write_websocket_message(
                        &mut websocket,
                        JSONRPCMessage::Response(JSONRPCResponse {
                            id: request.id,
                            result,
                        }),
                    )
                    .await;
                }
            });
        }
    });
    format!("ws://{addr}")
}

pub(crate) async fn start_mock_remote_multi_connection_mcp_server_status_list_server(
    names: Vec<&'static str>,
) -> String {
    let listener = TcpListener::bind("127.0.0.1:0")
        .await
        .expect("listener should bind");
    let addr = listener.local_addr().expect("listener address");
    tokio::spawn(async move {
        loop {
            let (stream, _) = listener.accept().await.expect("accept should succeed");
            let names = names.clone();
            tokio::spawn(async move {
                let mut websocket = tokio_tungstenite::accept_async(stream)
                    .await
                    .expect("websocket upgrade should succeed");
                expect_remote_initialize(&mut websocket).await;

                loop {
                    let request = read_websocket_request(&mut websocket).await;
                    let result = match request.method.as_str() {
                        "mcpServerStatus/list" => {
                            paginated_mcp_server_status_list_result(&request, &names)
                        }
                        method => panic!("unexpected request method: {method}"),
                    };
                    write_websocket_message(
                        &mut websocket,
                        JSONRPCMessage::Response(JSONRPCResponse {
                            id: request.id,
                            result,
                        }),
                    )
                    .await;
                }
            });
        }
    });
    format!("ws://{addr}")
}

pub(crate) async fn start_mock_remote_multi_connection_experimental_feature_list_server(
    names: Vec<&'static str>,
) -> String {
    let listener = TcpListener::bind("127.0.0.1:0")
        .await
        .expect("listener should bind");
    let addr = listener.local_addr().expect("listener address");
    tokio::spawn(async move {
        loop {
            let (stream, _) = listener.accept().await.expect("accept should succeed");
            let names = names.clone();
            tokio::spawn(async move {
                let mut websocket = tokio_tungstenite::accept_async(stream)
                    .await
                    .expect("websocket upgrade should succeed");
                expect_remote_initialize(&mut websocket).await;

                loop {
                    let request = read_websocket_request(&mut websocket).await;
                    let result = match request.method.as_str() {
                        "experimentalFeature/list" => {
                            paginated_experimental_feature_list_result(&request, &names)
                        }
                        method => panic!("unexpected request method: {method}"),
                    };
                    write_websocket_message(
                        &mut websocket,
                        JSONRPCMessage::Response(JSONRPCResponse {
                            id: request.id,
                            result,
                        }),
                    )
                    .await;
                }
            });
        }
    });
    format!("ws://{addr}")
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum LegacyApprovalExercise {
    Exercise,
    Skip,
}
