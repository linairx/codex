use super::RemoteWorkerGatewayRuntime;
use super::remote_runtime_views;
use crate::adapter::thread_read_request;
use crate::adapter::thread_start_request;
use crate::adapter::turn_interrupt_request;
use crate::adapter::turn_start_request;
use crate::api::CreateThreadRequest;
use crate::api::GatewayAccountCapacityStatus;
use crate::api::GatewayAccountLeaseState;
use crate::api::GatewayHealthResponse;
use crate::api::InterruptTurnResponse;
use crate::api::ListThreadsRequest;
use crate::api::ListThreadsResponse;
use crate::api::StartTurnRequest;
use crate::api::ThreadResponse;
use crate::api::TurnResponse;
use crate::error::GatewayError;
use crate::event::GatewayAccountThreadHandoffFailed;
use crate::event::GatewayAccountThreadHandoffSucceeded;
use crate::event::GatewayEvent;
use crate::runtime::GatewayRuntime;
use crate::scope::GatewayRequestContext;
use async_trait::async_trait;
use codex_app_server_protocol::ThreadReadResponse;
use codex_app_server_protocol::TurnInterruptResponse as AppServerTurnInterruptResponse;
use codex_app_server_protocol::TurnStartResponse;
use tracing::info;
use tracing::warn;

#[async_trait]
impl GatewayRuntime for RemoteWorkerGatewayRuntime {
    async fn create_thread(
        &self,
        context: GatewayRequestContext,
        request: CreateThreadRequest,
    ) -> Result<ThreadResponse, GatewayError> {
        let mut exhausted_worker_ids = Vec::new();
        for _ in 0..self.workers.len() {
            let worker = self.pick_worker_for_project(&context)?;
            match worker
                .request_handle()
                .request_typed::<codex_app_server_protocol::ThreadStartResponse>(
                    thread_start_request(self.next_request_id(), request.clone()),
                )
                .await
            {
                Ok(response) => {
                    if self
                        .worker_health
                        .mark_account_available_for_worker(worker.id())
                    {
                        let account_id = self.worker_health.account_id(worker.id());
                        self.observability.record_account_lease_event(
                            GatewayAccountLeaseState::Leased,
                            account_id.as_deref(),
                            Some(worker.id()),
                            Some(&context),
                            None,
                        );
                        let _ = self.events.send(GatewayEvent::account_lease_changed(
                            context.tenant_id.as_str(),
                            context.project_id.as_deref(),
                            worker.id(),
                            account_id,
                            GatewayAccountLeaseState::Leased,
                            None,
                        ));
                    }
                    if !exhausted_worker_ids.is_empty() {
                        let _ = self.events.send(GatewayEvent::account_failover_succeeded(
                            context.tenant_id.as_str(),
                            context.project_id.as_deref(),
                            worker.id(),
                            self.worker_health.account_id(worker.id()),
                            exhausted_worker_ids.clone(),
                        ));
                        self.observability.record_account_capacity_event(
                            worker.id(),
                            "thread_start_failover_success",
                        );
                        info!(
                            method = "thread/start",
                            tenant_id = context.tenant_id.as_str(),
                            project_id = context.project_id.as_deref(),
                            replacement_worker_id = worker.id(),
                            replacement_account_id = self.worker_health.account_id(worker.id()),
                            exhausted_worker_ids = ?exhausted_worker_ids,
                            "gateway HTTP thread/start retried on another account-backed worker after account capacity exhaustion"
                        );
                    }
                    self.scope_registry.register_thread_with_worker(
                        response.thread.id.clone(),
                        context.clone(),
                        Some(worker.id()),
                    );
                    self.scope_registry
                        .register_project_worker(context.clone(), worker.id());
                    if let Some(project_id) = context.project_id.as_deref() {
                        let account_id = self.worker_health.account_id(worker.id());
                        self.observability.record_project_worker_route_selected(
                            worker.id(),
                            context.tenant_id.as_str(),
                            project_id,
                            response.thread.id.as_str(),
                            account_id.as_deref(),
                        );
                        let _ = self
                            .events
                            .send(GatewayEvent::project_worker_route_selected(
                                crate::event::GatewayProjectWorkerRouteSelected {
                                    tenant_id: context.tenant_id.as_str(),
                                    project_id,
                                    thread_id: response.thread.id.as_str(),
                                    worker_id: worker.id(),
                                    account_id,
                                },
                            ));
                    }

                    return Ok(ThreadResponse {
                        thread: response.thread.into(),
                    });
                }
                Err(error) => {
                    if let Some(reason) =
                        remote_runtime_views::account_capacity_error_reason(&error)
                    {
                        self.mark_account_capacity_exhausted(
                            &context,
                            "thread/start",
                            worker.id(),
                            reason,
                        );
                        exhausted_worker_ids.push(worker.id());
                        continue;
                    }

                    let gateway_error = GatewayError::from(error);
                    if matches!(gateway_error, GatewayError::Upstream(_)) {
                        self.worker_health
                            .mark_unhealthy(worker.id(), Some(format!("{gateway_error:?}")));
                        continue;
                    }

                    return Err(gateway_error);
                }
            }
        }

        Err(GatewayError::Upstream(
            "no healthy remote workers with available account capacity are available".to_string(),
        ))
    }

    async fn list_threads(
        &self,
        context: GatewayRequestContext,
        request: ListThreadsRequest,
    ) -> Result<ListThreadsResponse, GatewayError> {
        let mut data = Vec::new();
        for worker in &self.workers {
            if !self.worker_health.is_healthy(worker.id()) {
                continue;
            }
            match self.fetch_all_threads(worker, &request).await {
                Ok(threads) => data.extend(threads),
                Err(GatewayError::Upstream(_)) => {
                    self.worker_health.mark_unhealthy(
                        worker.id(),
                        Some("thread list failed for remote worker".to_string()),
                    );
                }
                Err(error) => return Err(error),
            }
        }

        data.retain(|thread| self.scope_registry.thread_visible_to(&context, &thread.id));
        self.sort_threads(&mut data, &request);

        let start = Self::decode_cursor(request.cursor.as_deref())?;
        let limit = request
            .limit
            .map(|value| value as usize)
            .unwrap_or(remote_runtime_views::DEFAULT_THREAD_LIST_LIMIT);
        let end = start.saturating_add(limit).min(data.len());
        let page = if start >= data.len() {
            Vec::new()
        } else {
            data[start..end].to_vec()
        };
        let next_cursor = (end < data.len()).then(|| Self::encode_cursor(end));

        Ok(ListThreadsResponse {
            data: page,
            next_cursor,
            backwards_cursor: None,
        })
    }

    async fn read_thread(
        &self,
        context: GatewayRequestContext,
        thread_id: String,
    ) -> Result<ThreadResponse, GatewayError> {
        let worker = self.worker_for_thread(&context, &thread_id)?;
        let worker_id = worker.id();
        if self.worker_health.account_capacity(worker_id)
            == Some(GatewayAccountCapacityStatus::Exhausted)
        {
            let mut first_error = None::<String>;
            for replacement in &self.workers {
                let replacement_worker_id = replacement.id();
                if replacement_worker_id == worker_id
                    || !self.worker_health.is_healthy(replacement_worker_id)
                    || !self
                        .worker_health
                        .account_has_capacity(replacement_worker_id)
                {
                    continue;
                }

                match replacement
                    .request_handle()
                    .request_typed::<ThreadReadResponse>(thread_read_request(
                        self.next_request_id(),
                        thread_id.clone(),
                    ))
                    .await
                {
                    Ok(response) => {
                        if response.thread.id != thread_id {
                            if first_error.is_none() {
                                first_error = Some(format!(
                                    "replacement worker returned thread {} while restoring {thread_id}",
                                    response.thread.id
                                ));
                            }
                            continue;
                        }
                        self.scope_registry.register_thread_with_worker(
                            response.thread.id.clone(),
                            context.clone(),
                            Some(replacement_worker_id),
                        );
                        self.observability.record_account_capacity_event(
                            replacement_worker_id,
                            "thread_read_handoff_success",
                        );
                        let _ = self
                            .events
                            .send(GatewayEvent::account_thread_handoff_succeeded(
                                GatewayAccountThreadHandoffSucceeded {
                                    tenant_id: context.tenant_id.as_str(),
                                    project_id: context.project_id.as_deref(),
                                    method: "thread/read",
                                    thread_id: thread_id.as_str(),
                                    exhausted_worker_id: worker_id,
                                    exhausted_account_id: self.worker_health.account_id(worker_id),
                                    replacement_worker_id,
                                    replacement_account_id: self
                                        .worker_health
                                        .account_id(replacement_worker_id),
                                },
                            ));
                        info!(
                            method = "thread/read",
                            tenant_id = context.tenant_id.as_str(),
                            project_id = context.project_id.as_deref(),
                            thread_id = thread_id.as_str(),
                            exhausted_worker_id = worker_id,
                            exhausted_account_id = self.worker_health.account_id(worker_id),
                            replacement_worker_id,
                            replacement_account_id =
                                self.worker_health.account_id(replacement_worker_id),
                            "gateway HTTP restored a thread/read request on another account-backed worker after account capacity exhaustion"
                        );
                        return Ok(ThreadResponse {
                            thread: response.thread.into(),
                        });
                    }
                    Err(error) => {
                        if first_error.is_none() {
                            first_error = Some(error.to_string());
                        }
                    }
                }
            }

            let reason = first_error
                .map(|error| format!("replacement worker failed to read thread: {error}"))
                .unwrap_or_else(|| "no replacement worker restored the context".to_string());
            self.observability
                .record_account_capacity_event(worker_id, "thread_read_handoff_failure");
            let _ = self
                .events
                .send(GatewayEvent::account_thread_handoff_failed(
                    GatewayAccountThreadHandoffFailed {
                        tenant_id: context.tenant_id.as_str(),
                        project_id: context.project_id.as_deref(),
                        method: "thread/read",
                        thread_id: thread_id.as_str(),
                        exhausted_worker_id: worker_id,
                        exhausted_account_id: self.worker_health.account_id(worker_id),
                        reason: reason.as_str(),
                    },
                ));
            warn!(
                method = "thread/read",
                account_capacity_event = "thread_read_handoff_failure",
                tenant_id = context.tenant_id.as_str(),
                project_id = context.project_id.as_deref(),
                thread_id = thread_id.as_str(),
                exhausted_worker_id = worker_id,
                exhausted_account_id = self.worker_health.account_id(worker_id),
                reason = reason.as_str(),
                "gateway HTTP failed closed because no replacement account-backed worker restored thread/read"
            );
            return Err(GatewayError::Upstream(reason));
        }

        let response: ThreadReadResponse = worker
            .request_handle()
            .request_typed(thread_read_request(self.next_request_id(), thread_id))
            .await?;

        Ok(ThreadResponse {
            thread: response.thread.into(),
        })
    }

    async fn start_turn(
        &self,
        context: GatewayRequestContext,
        thread_id: String,
        request: StartTurnRequest,
    ) -> Result<TurnResponse, GatewayError> {
        if request.input.trim().is_empty() {
            return Err(GatewayError::InvalidRequest(
                "turn input must not be empty".to_string(),
            ));
        }
        self.fail_if_active_thread_account_exhausted(&context, "turn/start", &thread_id)?;
        let worker = self.worker_for_thread(&context, &thread_id)?;
        let worker_id = worker.id();
        let response: TurnStartResponse = match worker
            .request_handle()
            .request_typed(turn_start_request(
                self.next_request_id(),
                thread_id,
                request,
            ))
            .await
        {
            Ok(response) => response,
            Err(error) => {
                if let Some(reason) = remote_runtime_views::account_capacity_error_reason(&error) {
                    self.mark_account_capacity_exhausted(&context, "turn/start", worker_id, reason);
                }
                return Err(GatewayError::from(error));
            }
        };

        Ok(TurnResponse {
            turn: response.turn.into(),
        })
    }

    async fn interrupt_turn(
        &self,
        context: GatewayRequestContext,
        thread_id: String,
        turn_id: String,
    ) -> Result<InterruptTurnResponse, GatewayError> {
        self.fail_if_active_thread_account_exhausted(&context, "turn/interrupt", &thread_id)?;
        let worker = self.worker_for_thread(&context, &thread_id)?;
        let worker_id = worker.id();
        let _: AppServerTurnInterruptResponse = match worker
            .request_handle()
            .request_typed(turn_interrupt_request(
                self.next_request_id(),
                thread_id,
                turn_id,
            ))
            .await
        {
            Ok(response) => response,
            Err(error) => {
                if let Some(reason) = remote_runtime_views::account_capacity_error_reason(&error) {
                    self.mark_account_capacity_exhausted(
                        &context,
                        "turn/interrupt",
                        worker_id,
                        reason,
                    );
                }
                return Err(GatewayError::from(error));
            }
        };

        Ok(InterruptTurnResponse { status: "accepted" })
    }

    async fn resolve_server_request(
        &self,
        context: GatewayRequestContext,
        request: crate::api::ResolveServerRequestRequest,
    ) -> Result<crate::api::ResolveServerRequestResponse, GatewayError> {
        self.resolve_server_request_impl(context, request).await
    }

    fn health(&self) -> GatewayHealthResponse {
        remote_runtime_views::health_response(self)
    }

    fn subscribe(&self) -> tokio::sync::broadcast::Receiver<GatewayEvent> {
        self.events.subscribe()
    }

    fn event_visible_to(&self, context: &GatewayRequestContext, event: &GatewayEvent) -> bool {
        self.scope_registry.event_visible_to(context, event)
    }
}
