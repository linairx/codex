use crate::northbound::v2_connection::GatewayV2DownstreamRouter;
use crate::northbound::v2_routing::config_read_response_matches_cwd;
use crate::northbound::v2_wire::jsonrpc_request_to_client_request;
use crate::northbound::v2_wire::request_params;
use codex_app_server_protocol::ClientNotification;
use codex_app_server_protocol::ClientRequest;
use codex_app_server_protocol::ConfigReadParams;
use codex_app_server_protocol::FsUnwatchParams;
use codex_app_server_protocol::FsWatchParams;
use codex_app_server_protocol::JSONRPCErrorError;
use codex_app_server_protocol::JSONRPCRequest;
use codex_app_server_protocol::RequestId;
use serde_json::Value;
use std::io;
use std::path::Path;

pub(crate) async fn fanout_mutating_connection_request(
    downstream: &GatewayV2DownstreamRouter,
    request: JSONRPCRequest,
) -> io::Result<Result<Value, JSONRPCErrorError>> {
    downstream.ensure_all_configured_workers_present_for(&request.method)?;
    let mut primary_result = None;

    for (index, worker) in downstream.workers.iter().enumerate() {
        let response = worker
            .request_handle
            .request(jsonrpc_request_to_client_request(request.clone())?)
            .await?;
        match response {
            Ok(result) => {
                if index == 0 {
                    primary_result = Some(result);
                }
            }
            Err(error) => return Ok(Err(error)),
        }
    }

    primary_result.map(Ok).ok_or_else(|| {
        io::Error::other("gateway v2 connection has no downstream app-server sessions")
    })
}

pub(crate) async fn fanout_fs_watch_request(
    downstream: &mut GatewayV2DownstreamRouter,
    request: JSONRPCRequest,
) -> io::Result<Result<Value, JSONRPCErrorError>> {
    downstream.ensure_all_configured_workers_present_for(&request.method)?;
    let params = request_params::<FsWatchParams>(&request)?;
    let mut primary_result = None;
    let mut watched_workers = Vec::new();

    for (index, worker) in downstream.workers.iter().enumerate() {
        let response = worker
            .request_handle
            .request(jsonrpc_request_to_client_request(request.clone())?)
            .await?;
        match response {
            Ok(result) => {
                watched_workers.push(worker.worker_id);
                if index == 0 {
                    primary_result = Some(result);
                }
            }
            Err(error) => {
                rollback_fs_watch_registrations(downstream, &params.watch_id, &watched_workers)
                    .await;
                return Ok(Err(error));
            }
        }
    }

    let Some(primary_result) = primary_result else {
        return Err(io::Error::other(
            "gateway v2 connection has no downstream app-server sessions",
        ));
    };

    downstream.record_fs_watch(params);
    Ok(Ok(primary_result))
}

pub(crate) async fn fanout_fs_unwatch_request(
    downstream: &mut GatewayV2DownstreamRouter,
    request: JSONRPCRequest,
) -> io::Result<Result<Value, JSONRPCErrorError>> {
    let params = request_params::<FsUnwatchParams>(&request)?;
    let result = fanout_mutating_connection_request(downstream, request).await?;
    if result.is_ok() {
        downstream.clear_fs_watch(&params.watch_id);
    }
    Ok(result)
}

async fn rollback_fs_watch_registrations(
    downstream: &GatewayV2DownstreamRouter,
    watch_id: &str,
    worker_ids: &[Option<usize>],
) {
    for worker_id in worker_ids {
        let Some(worker) = downstream.latest_worker_with_id(*worker_id) else {
            continue;
        };
        let _ = worker
            .request_handle
            .request(ClientRequest::FsUnwatch {
                request_id: RequestId::String(format!("gateway-rollback-fs-watch:{watch_id}")),
                params: FsUnwatchParams {
                    watch_id: watch_id.to_string(),
                },
            })
            .await;
    }
}

pub(crate) async fn route_config_read_request_if_supported(
    downstream: &GatewayV2DownstreamRouter,
    request: JSONRPCRequest,
) -> io::Result<Result<Value, JSONRPCErrorError>> {
    let params = request_params::<ConfigReadParams>(&request)?;
    let Some(cwd) = params.cwd.as_ref() else {
        if downstream.multi_worker_topology() {
            downstream.ensure_primary_worker_present_for("config/read")?;
        }
        let worker = downstream.primary_worker()?;
        return worker
            .request_handle
            .request(jsonrpc_request_to_client_request(request)?)
            .await;
    };
    downstream.ensure_all_configured_workers_present_for("config/read")?;
    let request_cwd = Path::new(cwd);
    let mut primary_result = None;
    let mut first_result = None;
    let mut first_error = None;

    for (index, worker) in downstream.workers.iter().enumerate() {
        let response = worker
            .request_handle
            .request(jsonrpc_request_to_client_request(request.clone())?)
            .await?;
        match response {
            Ok(result) => {
                if config_read_response_matches_cwd(&result, request_cwd) {
                    return Ok(Ok(result));
                }
                if index == 0 {
                    primary_result = Some(result.clone());
                }
                if first_result.is_none() {
                    first_result = Some(result);
                }
            }
            Err(error) => {
                if first_error.is_none() {
                    first_error = Some(error);
                }
            }
        }
    }

    if let Some(result) = primary_result.or(first_result) {
        return Ok(Ok(result));
    }

    match first_error {
        Some(error) => Ok(Err(error)),
        None => Err(io::Error::other(
            "gateway v2 connection has no downstream app-server sessions",
        )),
    }
}

pub(crate) async fn first_successful_connection_request(
    downstream: &GatewayV2DownstreamRouter,
    request: JSONRPCRequest,
) -> io::Result<Result<Value, JSONRPCErrorError>> {
    downstream.ensure_all_configured_workers_present_for(&request.method)?;
    let mut first_error = None;

    for worker in &downstream.workers {
        let response = worker
            .request_handle
            .request(jsonrpc_request_to_client_request(request.clone())?)
            .await?;
        match response {
            Ok(result) => return Ok(Ok(result)),
            Err(error) => {
                if first_error.is_none() {
                    first_error = Some(error);
                }
            }
        }
    }

    match first_error {
        Some(error) => Ok(Err(error)),
        None => Err(io::Error::other(
            "gateway v2 connection has no downstream app-server sessions",
        )),
    }
}

pub(crate) async fn fanout_connection_notification(
    downstream: &GatewayV2DownstreamRouter,
    notification: ClientNotification,
) -> io::Result<()> {
    for worker in &downstream.workers {
        worker.request_handle.notify(notification.clone()).await?;
    }
    Ok(())
}

pub(crate) fn is_multi_worker_fanout_login_request(request: &JSONRPCRequest) -> bool {
    request.method == "account/login/start"
        && request
            .params
            .as_ref()
            .and_then(|params| params.get("type"))
            .and_then(Value::as_str)
            .is_some_and(|login_type| matches!(login_type, "apiKey" | "chatgptAuthTokens"))
}
