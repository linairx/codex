use super::RemoteWorkerGatewayRuntime;
use crate::adapter::thread_list_request;
use crate::api::GatewayAccountCapacityStatus;
use crate::api::GatewayExecutionMode;
use crate::api::GatewayHealthResponse;
use crate::api::GatewayHealthStatus;
use crate::api::GatewayRemoteUnlabeledAccountWorker;
use crate::api::GatewaySortDirection;
use crate::api::GatewayThread;
use crate::api::GatewayThreadSortKey;
use crate::api::GatewayV2CompatibilityMode;
use crate::api::ListThreadsRequest;
use crate::error::GatewayError;
use codex_app_server_client::TypedRequestError;
use codex_app_server_protocol::JSONRPCErrorError;
use codex_app_server_protocol::ThreadListResponse as AppServerThreadListResponse;
use std::cmp::Reverse;

pub(super) const DEFAULT_THREAD_LIST_LIMIT: usize = 20;
const GATEWAY_THREAD_CURSOR_PREFIX: &str = "offset:";

pub(super) fn account_capacity_error_reason(error: &TypedRequestError) -> Option<String> {
    let TypedRequestError::Server { source, .. } = error else {
        return None;
    };

    is_account_capacity_error(source).then(|| source.message.clone())
}

pub(super) fn is_account_capacity_error(error: &JSONRPCErrorError) -> bool {
    if error.code == 429 {
        return true;
    }

    let message = error.message.to_ascii_lowercase();
    [
        "rate limit",
        "rate_limit",
        "usage limit",
        "usage_limit",
        "credits depleted",
        "billing",
        "quota",
    ]
    .iter()
    .any(|needle| message.contains(needle))
}

pub(super) fn health_response(runtime: &RemoteWorkerGatewayRuntime) -> GatewayHealthResponse {
    let remote_workers = runtime.worker_health.snapshot();
    let remote_unlabeled_account_workers: Vec<GatewayRemoteUnlabeledAccountWorker> =
        if remote_workers.len() > 1 {
            remote_workers
                .iter()
                .filter(|worker| {
                    worker
                        .account_id
                        .as_deref()
                        .map(str::trim)
                        .is_none_or(str::is_empty)
                })
                .map(|worker| GatewayRemoteUnlabeledAccountWorker {
                    worker_id: worker.worker_id,
                    websocket_url: worker.websocket_url.clone(),
                })
                .collect()
        } else {
            Vec::new()
        };
    let remote_unlabeled_account_worker_ids: Vec<usize> = remote_unlabeled_account_workers
        .iter()
        .map(|worker| worker.worker_id)
        .collect();
    let remote_unlabeled_account_worker_count = remote_unlabeled_account_workers.len();
    let remote_account_labels_complete = remote_unlabeled_account_worker_ids.is_empty();
    let status = if remote_workers.iter().all(|worker| worker.healthy) {
        GatewayHealthStatus::Ok
    } else if remote_workers.iter().any(|worker| worker.healthy) {
        GatewayHealthStatus::Degraded
    } else {
        GatewayHealthStatus::Unavailable
    };

    let project_worker_routes = runtime.scope_registry.project_worker_routes(
        |worker_id| runtime.worker_health.is_healthy(worker_id),
        |worker_id| runtime.worker_health.account_id(worker_id),
        |worker_id| {
            runtime
                .worker_health
                .account_capacity(worker_id)
                .unwrap_or(GatewayAccountCapacityStatus::Exhausted)
        },
    );

    GatewayHealthResponse {
        status,
        runtime_mode: "remote".to_string(),
        execution_mode: GatewayExecutionMode::WorkerManaged,
        v2_compatibility: if remote_workers.len() == 1 {
            GatewayV2CompatibilityMode::RemoteSingleWorker
        } else {
            GatewayV2CompatibilityMode::RemoteMultiWorker
        },
        v2_transport: runtime.v2_transport,
        v2_connections: runtime.v2_connection_health.snapshot(),
        pending_server_request_count: runtime.scope_registry.pending_server_request_count(),
        pending_server_request_kind_counts: runtime
            .scope_registry
            .pending_server_request_kind_counts(),
        pending_server_request_route_counts: runtime
            .scope_registry
            .pending_server_request_route_counts(),
        pending_server_request_oldest_at: runtime.scope_registry.pending_server_request_oldest_at(),
        remote_workers: Some(remote_workers),
        remote_account_labels_complete: Some(remote_account_labels_complete),
        remote_unlabeled_account_worker_count: Some(remote_unlabeled_account_worker_count),
        remote_unlabeled_account_worker_ids: Some(remote_unlabeled_account_worker_ids),
        remote_unlabeled_account_workers: Some(remote_unlabeled_account_workers),
        project_worker_routes: Some(project_worker_routes.clone()),
        worker_pool: Some(
            runtime
                .worker_pool
                .snapshot_with_worker_health_and_project_routes(
                    &runtime.worker_health,
                    &project_worker_routes,
                ),
        ),
    }
}

impl RemoteWorkerGatewayRuntime {
    pub(super) async fn fetch_all_threads(
        &self,
        worker: &crate::remote_worker::GatewayRemoteWorker,
        request: &ListThreadsRequest,
    ) -> Result<Vec<GatewayThread>, GatewayError> {
        let mut all_threads = Vec::new();
        let mut cursor = None;

        loop {
            let response: AppServerThreadListResponse = worker
                .request_handle()
                .request_typed(thread_list_request(
                    self.next_request_id(),
                    ListThreadsRequest {
                        cursor: cursor.clone(),
                        limit: Some(u32::MAX),
                        sort_key: request.sort_key,
                        sort_direction: request.sort_direction,
                        archived: request.archived,
                        cwd: request.cwd.clone(),
                        search_term: request.search_term.clone(),
                    },
                ))
                .await?;
            let next_cursor = response.next_cursor.clone();
            all_threads.extend(response.data.into_iter().map(Into::into));
            if next_cursor.is_none() {
                break;
            }
            cursor = next_cursor;
        }

        Ok(all_threads)
    }

    pub(super) fn sort_threads(&self, threads: &mut [GatewayThread], request: &ListThreadsRequest) {
        match request.sort_key.unwrap_or(GatewayThreadSortKey::CreatedAt) {
            GatewayThreadSortKey::CreatedAt => {
                if request.sort_direction.unwrap_or(GatewaySortDirection::Desc)
                    == GatewaySortDirection::Asc
                {
                    threads.sort_by_key(|thread| (thread.created_at, thread.id.clone()));
                } else {
                    threads.sort_by_key(|thread| Reverse((thread.created_at, thread.id.clone())));
                }
            }
            GatewayThreadSortKey::UpdatedAt => {
                if request.sort_direction.unwrap_or(GatewaySortDirection::Desc)
                    == GatewaySortDirection::Asc
                {
                    threads.sort_by_key(|thread| (thread.updated_at, thread.id.clone()));
                } else {
                    threads.sort_by_key(|thread| Reverse((thread.updated_at, thread.id.clone())));
                }
            }
        }
    }

    pub(super) fn decode_cursor(cursor: Option<&str>) -> Result<usize, GatewayError> {
        let Some(cursor) = cursor else {
            return Ok(0);
        };
        let Some(offset) = cursor.strip_prefix(GATEWAY_THREAD_CURSOR_PREFIX) else {
            return Err(GatewayError::InvalidRequest(format!(
                "invalid gateway thread cursor: {cursor}"
            )));
        };
        offset.parse::<usize>().map_err(|_| {
            GatewayError::InvalidRequest(format!("invalid gateway thread cursor: {cursor}"))
        })
    }

    pub(super) fn encode_cursor(offset: usize) -> String {
        format!("{GATEWAY_THREAD_CURSOR_PREFIX}{offset}")
    }
}
