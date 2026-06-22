use super::RemoteWorkerGatewayRuntime;
use crate::api::ResolveServerRequestRequest;
use crate::api::ResolveServerRequestResponse;
use crate::error::GatewayError;
use crate::remote_worker::GatewayRemoteWorker;
use crate::scope::GatewayRequestContext;
use tracing::warn;

impl RemoteWorkerGatewayRuntime {
    pub(super) async fn resolve_server_request_impl(
        &self,
        context: GatewayRequestContext,
        request: ResolveServerRequestRequest,
    ) -> Result<ResolveServerRequestResponse, GatewayError> {
        let Some(pending_request) = self
            .scope_registry
            .pending_server_request(&request.request_id)
        else {
            return Err(GatewayError::NotFound(format!(
                "server request not found: {}",
                request.request_id
            )));
        };

        if pending_request.context != context {
            return Err(GatewayError::NotFound(format!(
                "server request not found: {}",
                request.request_id
            )));
        }

        if request.response.kind() != pending_request.kind {
            let response_kind = request.response.kind().metric_tag();
            self.observability.record_server_request_lifecycle_event(
                "client_server_request_invalid_response",
                response_kind,
            );
            warn!(
                tenant_id = context.tenant_id.as_str(),
                project_id = context.project_id.as_deref(),
                expected_response_kind = pending_request.kind.metric_tag(),
                response_kind,
                request_id = %request.request_id,
                thread_id = pending_request.thread_id.as_str(),
                "gateway HTTP rejected server request response because its type does not match the pending request"
            );
            return Err(GatewayError::InvalidRequest(
                "server request response type does not match pending request".to_string(),
            ));
        }

        let request_id = request.request_id.clone();
        let response_kind = request.response.kind().metric_tag();
        let Some(worker_id) = pending_request.worker_id else {
            self.record_server_request_delivery_failure(response_kind);
            warn!(
                tenant_id = context.tenant_id.as_str(),
                project_id = context.project_id.as_deref(),
                response_kind,
                request_id = %request_id,
                thread_id = pending_request.thread_id.as_str(),
                "gateway HTTP could not deliver server request response because the pending request has no remote worker route"
            );
            return Err(GatewayError::Upstream(format!(
                "server request {} is missing a remote worker route",
                request.request_id
            )));
        };
        if let Err(error) = self.fail_if_worker_account_exhausted(
            &context,
            "serverRequest/respond",
            &pending_request.thread_id,
            worker_id,
        ) {
            self.record_server_request_delivery_failure(response_kind);
            warn!(
                tenant_id = context.tenant_id.as_str(),
                project_id = context.project_id.as_deref(),
                response_kind,
                worker_id,
                worker_websocket_url = self
                    .workers
                    .get(worker_id)
                    .map(GatewayRemoteWorker::websocket_url)
                    .unwrap_or("unavailable"),
                request_id = %request_id,
                thread_id = pending_request.thread_id.as_str(),
                error = ?error,
                "gateway HTTP could not deliver server request response because the owning account is exhausted"
            );
            return Err(error);
        }
        let worker = match self.worker(worker_id) {
            Ok(worker) => worker,
            Err(error) => {
                self.record_server_request_delivery_failure(response_kind);
                warn!(
                    tenant_id = context.tenant_id.as_str(),
                    project_id = context.project_id.as_deref(),
                    response_kind,
                    worker_id,
                    worker_websocket_url = self
                        .workers
                        .get(worker_id)
                        .map(GatewayRemoteWorker::websocket_url)
                        .unwrap_or("unavailable"),
                    request_id = %request_id,
                    thread_id = pending_request.thread_id.as_str(),
                    error = ?error,
                    "gateway HTTP could not deliver server request response because the remote worker route is unavailable"
                );
                return Err(error);
            }
        };
        let result = request.response.into_result().map_err(|err| {
            GatewayError::InvalidRequest(format!("invalid server request response payload: {err}"))
        })?;
        self.observability
            .record_server_request_lifecycle_event("client_server_request_answered", response_kind);
        if let Err(err) = worker
            .request_handle()
            .resolve_server_request(request_id.clone(), result)
            .await
        {
            let error = err.to_string();
            self.record_server_request_delivery_failure_after_answer(response_kind);
            warn!(
                tenant_id = context.tenant_id.as_str(),
                project_id = context.project_id.as_deref(),
                response_kind,
                worker_id,
                worker_websocket_url = worker.websocket_url(),
                request_id = %request_id,
                thread_id = pending_request.thread_id.as_str(),
                error = error.as_str(),
                "gateway HTTP failed to deliver server request response to remote worker"
            );
            return Err(GatewayError::Upstream(format!(
                "server request resolve failed: {err}"
            )));
        }
        self.observability.record_server_request_lifecycle_event(
            "client_server_request_delivered",
            response_kind,
        );
        self.scope_registry
            .clear_pending_server_request(&request_id);

        Ok(ResolveServerRequestResponse { status: "accepted" })
    }
}
