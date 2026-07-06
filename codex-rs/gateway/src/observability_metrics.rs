//! Metrics recording helpers for `GatewayObservability`.

use super::*;

impl GatewayObservability {
    pub(crate) fn record_http_request(
        &self,
        method: &str,
        route: &str,
        status_code: u16,
        duration: Duration,
    ) {
        let status = status_code.to_string();
        let status_class = status_class(status_code);
        let duration_ms = duration.as_millis().min(i64::MAX as u128) as i64;
        let tags = [
            ("method", method),
            ("route", route),
            ("status", status.as_str()),
            ("status_class", status_class),
        ];

        if let Some(metrics) = &self.metrics {
            if let Err(err) = metrics.counter(REQUEST_COUNT_METRIC, 1, &tags) {
                tracing::warn!("failed to record gateway request count metric: {err}");
            }
            if let Err(err) = metrics.histogram(REQUEST_DURATION_METRIC, duration_ms, &tags) {
                tracing::warn!("failed to record gateway request duration metric: {err}");
            }
        }
    }

    pub(crate) fn record_account_capacity_event(&self, worker_id: usize, event: &str) {
        let worker_id = worker_id.to_string();
        let tags = [("worker_id", worker_id.as_str()), ("event", event)];

        if let Some(metrics) = &self.metrics
            && let Err(err) = metrics.counter(ACCOUNT_CAPACITY_EVENT_COUNT_METRIC, 1, &tags)
        {
            tracing::warn!("failed to record gateway account capacity event metric: {err}");
        }
    }

    pub(crate) fn record_remote_account_label_event(&self, worker_id: usize, event: &str) {
        let worker_id = worker_id.to_string();
        let tags = [("worker_id", worker_id.as_str()), ("event", event)];

        if let Some(metrics) = &self.metrics
            && let Err(err) = metrics.counter(REMOTE_ACCOUNT_LABEL_EVENT_COUNT_METRIC, 1, &tags)
        {
            tracing::warn!("failed to record gateway remote account label event metric: {err}");
        }
    }

    pub(crate) fn record_project_worker_route_selected(
        &self,
        worker_id: usize,
        tenant_id: &str,
        project_id: &str,
        thread_id: &str,
        account_id: Option<&str>,
    ) {
        let worker_id_for_health = worker_id;
        let worker_id = worker_id.to_string();
        let tags = [
            ("worker_id", worker_id.as_str()),
            ("tenant_id", tenant_id),
            ("project_id", project_id),
            ("account_id", account_id.unwrap_or("none")),
        ];

        if let Some(metrics) = &self.metrics
            && let Err(err) = metrics.counter(PROJECT_WORKER_ROUTE_SELECTION_COUNT_METRIC, 1, &tags)
        {
            tracing::warn!("failed to record gateway project worker route selection metric: {err}");
        }

        if self.audit_logs_enabled {
            tracing::event!(
                target: "codex_gateway.audit",
                Level::INFO,
                worker_id = worker_id_for_health,
                tenant_id = tenant_id,
                project_id = project_id,
                thread_id = thread_id,
                account_id = account_id.unwrap_or("<none>"),
                "gateway project worker route selected",
            );
        }

        self.v2_connection_health
            .record_project_worker_route_selected(
                worker_id_for_health,
                tenant_id,
                project_id,
                thread_id,
                account_id,
            );
    }

    pub(crate) fn record_server_request_answer_delivery_failure(&self, response_kind: &str) {
        let tags = [("response_kind", response_kind)];

        if let Some(metrics) = &self.metrics
            && let Err(err) = metrics.counter(
                SERVER_REQUEST_ANSWER_DELIVERY_FAILURE_COUNT_METRIC,
                1,
                &tags,
            )
        {
            tracing::warn!(
                "failed to record gateway server-request answer delivery failure metric: {err}"
            );
        }
    }

    pub(crate) fn record_server_request_lifecycle_event(&self, event: &str, method: &str) {
        let tags = [("event", event), ("method", method)];

        if let Some(metrics) = &self.metrics
            && let Err(err) = metrics.counter(SERVER_REQUEST_LIFECYCLE_EVENT_COUNT_METRIC, 1, &tags)
        {
            tracing::warn!("failed to record gateway server-request lifecycle metric: {err}");
        }
    }

    pub(crate) fn record_v2_request(&self, method: &str, outcome: &str, duration: Duration) {
        let duration_ms = duration.as_millis().min(i64::MAX as u128) as i64;
        let tags = [("method", method), ("outcome", outcome)];

        self.v2_connection_health
            .record_request(method, outcome, duration);

        if let Some(metrics) = &self.metrics {
            if let Err(err) = metrics.counter(V2_REQUEST_COUNT_METRIC, 1, &tags) {
                tracing::warn!("failed to record gateway v2 request count metric: {err}");
            }
            if let Err(err) = metrics.histogram(V2_REQUEST_DURATION_METRIC, duration_ms, &tags) {
                tracing::warn!("failed to record gateway v2 request duration metric: {err}");
            }
        }
    }

    pub(crate) fn record_v2_connection(
        &self,
        outcome: &str,
        duration: Duration,
        pending_counts: GatewayV2ConnectionPendingCounts,
        max_pending_client_request_count: usize,
        max_server_request_backlog_count: usize,
    ) {
        let duration_ms = duration.as_millis().min(i64::MAX as u128) as i64;
        let tags = [("outcome", outcome)];

        if let Some(metrics) = &self.metrics {
            if let Err(err) = metrics.counter(V2_CONNECTION_COUNT_METRIC, 1, &tags) {
                tracing::warn!("failed to record gateway v2 connection count metric: {err}");
            }
            if let Err(err) = metrics.histogram(V2_CONNECTION_DURATION_METRIC, duration_ms, &tags) {
                tracing::warn!("failed to record gateway v2 connection duration metric: {err}");
            }
            if let Err(err) = metrics.histogram(
                V2_CONNECTION_PENDING_CLIENT_REQUEST_METRIC,
                pending_counts
                    .pending_client_request_count
                    .min(i64::MAX as usize) as i64,
                &tags,
            ) {
                tracing::warn!(
                    "failed to record gateway v2 connection pending client request metric: {err}"
                );
            }
            if let Err(err) = metrics.histogram(
                V2_CONNECTION_MAX_PENDING_CLIENT_REQUEST_METRIC,
                max_pending_client_request_count.min(i64::MAX as usize) as i64,
                &tags,
            ) {
                tracing::warn!(
                    "failed to record gateway v2 connection max pending client request metric: {err}"
                );
            }
            for counts in &pending_counts.pending_client_request_worker_counts {
                let worker_id = counts
                    .worker_id
                    .map_or_else(|| "none".to_string(), |worker_id| worker_id.to_string());
                let worker_tags = [("outcome", outcome), ("worker_id", worker_id.as_str())];
                if let Err(err) = metrics.histogram(
                    V2_CONNECTION_PENDING_CLIENT_REQUEST_BY_WORKER_METRIC,
                    counts.pending_client_request_count.min(i64::MAX as usize) as i64,
                    &worker_tags,
                ) {
                    tracing::warn!(
                        "failed to record gateway v2 connection pending client request by worker metric: {err}"
                    );
                }
            }
            for counts in &pending_counts.pending_client_request_method_counts {
                let method_tags = [("outcome", outcome), ("method", counts.method.as_str())];
                if let Err(err) = metrics.histogram(
                    V2_CONNECTION_PENDING_CLIENT_REQUEST_BY_METHOD_METRIC,
                    counts.pending_client_request_count.min(i64::MAX as usize) as i64,
                    &method_tags,
                ) {
                    tracing::warn!(
                        "failed to record gateway v2 connection pending client request by method metric: {err}"
                    );
                }
            }
            if let Err(err) = metrics.histogram(
                V2_CONNECTION_PENDING_SERVER_REQUEST_METRIC,
                pending_counts
                    .pending_server_request_count
                    .min(i64::MAX as usize) as i64,
                &tags,
            ) {
                tracing::warn!(
                    "failed to record gateway v2 connection pending server request metric: {err}"
                );
            }
            if let Err(err) = metrics.histogram(
                V2_CONNECTION_ANSWERED_BUT_UNRESOLVED_SERVER_REQUEST_METRIC,
                pending_counts
                    .answered_but_unresolved_server_request_count
                    .min(i64::MAX as usize) as i64,
                &tags,
            ) {
                tracing::warn!(
                    "failed to record gateway v2 connection answered-but-unresolved server request metric: {err}"
                );
            }
            let server_request_backlog_count = pending_counts
                .pending_server_request_count
                .saturating_add(pending_counts.answered_but_unresolved_server_request_count);
            if let Err(err) = metrics.histogram(
                V2_CONNECTION_SERVER_REQUEST_BACKLOG_METRIC,
                server_request_backlog_count.min(i64::MAX as usize) as i64,
                &tags,
            ) {
                tracing::warn!(
                    "failed to record gateway v2 connection server request backlog metric: {err}"
                );
            }
            if let Err(err) = metrics.histogram(
                V2_CONNECTION_MAX_SERVER_REQUEST_BACKLOG_METRIC,
                max_server_request_backlog_count.min(i64::MAX as usize) as i64,
                &tags,
            ) {
                tracing::warn!(
                    "failed to record gateway v2 connection max server request backlog metric: {err}"
                );
            }
            for counts in &pending_counts.server_request_backlog_worker_counts {
                let worker_id = counts
                    .worker_id
                    .map_or_else(|| "none".to_string(), |worker_id| worker_id.to_string());
                let worker_tags = [("outcome", outcome), ("worker_id", worker_id.as_str())];
                if let Err(err) = metrics.histogram(
                    V2_CONNECTION_PENDING_SERVER_REQUEST_BY_WORKER_METRIC,
                    counts.pending_server_request_count.min(i64::MAX as usize) as i64,
                    &worker_tags,
                ) {
                    tracing::warn!(
                        "failed to record gateway v2 connection pending server request by worker metric: {err}"
                    );
                }
                if let Err(err) = metrics.histogram(
                    V2_CONNECTION_ANSWERED_BUT_UNRESOLVED_SERVER_REQUEST_BY_WORKER_METRIC,
                    counts
                        .answered_but_unresolved_server_request_count
                        .min(i64::MAX as usize) as i64,
                    &worker_tags,
                ) {
                    tracing::warn!(
                        "failed to record gateway v2 connection answered-but-unresolved server request by worker metric: {err}"
                    );
                }
                if let Err(err) = metrics.histogram(
                    V2_CONNECTION_SERVER_REQUEST_BACKLOG_BY_WORKER_METRIC,
                    counts.server_request_backlog_count.min(i64::MAX as usize) as i64,
                    &worker_tags,
                ) {
                    tracing::warn!(
                        "failed to record gateway v2 connection server request backlog by worker metric: {err}"
                    );
                }
            }
            for counts in &pending_counts.server_request_backlog_method_counts {
                let method_tags = [("outcome", outcome), ("method", counts.method.as_str())];
                if let Err(err) = metrics.histogram(
                    V2_CONNECTION_PENDING_SERVER_REQUEST_BY_METHOD_METRIC,
                    counts.pending_server_request_count.min(i64::MAX as usize) as i64,
                    &method_tags,
                ) {
                    tracing::warn!(
                        "failed to record gateway v2 connection pending server request by method metric: {err}"
                    );
                }
                if let Err(err) = metrics.histogram(
                    V2_CONNECTION_ANSWERED_BUT_UNRESOLVED_SERVER_REQUEST_BY_METHOD_METRIC,
                    counts
                        .answered_but_unresolved_server_request_count
                        .min(i64::MAX as usize) as i64,
                    &method_tags,
                ) {
                    tracing::warn!(
                        "failed to record gateway v2 connection answered-but-unresolved server request by method metric: {err}"
                    );
                }
                if let Err(err) = metrics.histogram(
                    V2_CONNECTION_SERVER_REQUEST_BACKLOG_BY_METHOD_METRIC,
                    counts.server_request_backlog_count.min(i64::MAX as usize) as i64,
                    &method_tags,
                ) {
                    tracing::warn!(
                        "failed to record gateway v2 connection server request backlog by method metric: {err}"
                    );
                }
            }
        }
    }

    pub(crate) fn record_v2_server_request_rejection(&self, method: &str, reason: &str) {
        let tags = [("method", method), ("reason", reason)];

        self.v2_connection_health
            .record_server_request_rejection(method, reason);

        if let Some(metrics) = &self.metrics
            && let Err(err) = metrics.counter(V2_SERVER_REQUEST_REJECTION_COUNT_METRIC, 1, &tags)
        {
            tracing::warn!("failed to record gateway v2 server-request rejection metric: {err}");
        }
    }

    pub(crate) fn record_v2_client_request_rejection(&self, method: &str, reason: &str) {
        let tags = [("method", method), ("reason", reason)];

        self.v2_connection_health
            .record_client_request_rejection(method, reason);

        if let Some(metrics) = &self.metrics
            && let Err(err) = metrics.counter(V2_CLIENT_REQUEST_REJECTION_COUNT_METRIC, 1, &tags)
        {
            tracing::warn!("failed to record gateway v2 client-request rejection metric: {err}");
        }
    }

    pub(crate) fn record_v2_worker_reconnect(&self, worker_id: usize, outcome: &str) {
        self.v2_connection_health
            .record_worker_reconnect_event(worker_id, outcome);

        let worker_id = worker_id.to_string();
        let tags = [("worker_id", worker_id.as_str()), ("outcome", outcome)];

        if let Some(metrics) = &self.metrics
            && let Err(err) = metrics.counter(V2_WORKER_RECONNECT_COUNT_METRIC, 1, &tags)
        {
            tracing::warn!("failed to record gateway v2 worker reconnect metric: {err}");
        }
    }

    pub(crate) fn record_v2_fail_closed_request(
        &self,
        method: &str,
        reconnect_backoff_active: bool,
    ) {
        self.v2_connection_health
            .record_fail_closed_request(method, reconnect_backoff_active);

        let reconnect_backoff_active = if reconnect_backoff_active {
            "true"
        } else {
            "false"
        };
        let tags = [
            ("method", method),
            ("reconnect_backoff_active", reconnect_backoff_active),
        ];

        if let Some(metrics) = &self.metrics
            && let Err(err) = metrics.counter(V2_FAIL_CLOSED_REQUEST_COUNT_METRIC, 1, &tags)
        {
            tracing::warn!("failed to record gateway v2 fail-closed request metric: {err}");
        }
    }

    pub(crate) fn record_v2_upstream_request_failure(
        &self,
        method: &str,
        reconnect_backoff_active: bool,
    ) {
        self.v2_connection_health
            .record_upstream_request_failure(method, reconnect_backoff_active);

        let reconnect_backoff_active = if reconnect_backoff_active {
            "true"
        } else {
            "false"
        };
        let tags = [
            ("method", method),
            ("reconnect_backoff_active", reconnect_backoff_active),
        ];

        if let Some(metrics) = &self.metrics
            && let Err(err) = metrics.counter(V2_UPSTREAM_REQUEST_FAILURE_COUNT_METRIC, 1, &tags)
        {
            tracing::warn!("failed to record gateway v2 upstream request failure metric: {err}");
        }
    }

    pub(crate) fn record_v2_client_response_send_failure(&self, method: &str, outcome: &str) {
        let tags = [("method", method), ("outcome", outcome)];

        self.v2_connection_health
            .record_client_response_send_failure(method, outcome);

        if let Some(metrics) = &self.metrics
            && let Err(err) =
                metrics.counter(V2_CLIENT_RESPONSE_SEND_FAILURE_COUNT_METRIC, 1, &tags)
        {
            tracing::warn!(
                "failed to record gateway v2 client response send failure metric: {err}"
            );
        }
    }

    pub(crate) fn record_v2_downstream_shutdown_failure(&self, outcome: &str) {
        let tags = [("outcome", outcome)];

        self.v2_connection_health
            .record_downstream_shutdown_failure(outcome);

        if let Some(metrics) = &self.metrics
            && let Err(err) = metrics.counter(V2_DOWNSTREAM_SHUTDOWN_FAILURE_COUNT_METRIC, 1, &tags)
        {
            tracing::warn!("failed to record gateway v2 downstream shutdown failure metric: {err}");
        }
    }

    pub(crate) fn record_v2_close_frame_send_failure(&self, code: u16, outcome: &str) {
        let code_label = code.to_string();
        let tags = [("code", code_label.as_str()), ("outcome", outcome)];

        self.v2_connection_health
            .record_close_frame_send_failure(code, outcome);

        if let Some(metrics) = &self.metrics
            && let Err(err) = metrics.counter(V2_CLOSE_FRAME_SEND_FAILURE_COUNT_METRIC, 1, &tags)
        {
            tracing::warn!("failed to record gateway v2 close frame send failure metric: {err}");
        }
    }

    pub(crate) fn record_v2_suppressed_notification(&self, method: &str, reason: &str) {
        let tags = [("method", method), ("reason", reason)];

        self.v2_connection_health
            .record_suppressed_notification(method, reason);

        if let Some(metrics) = &self.metrics
            && let Err(err) = metrics.counter(V2_SUPPRESSED_NOTIFICATION_COUNT_METRIC, 1, &tags)
        {
            tracing::warn!("failed to record gateway v2 suppressed notification metric: {err}");
        }
    }

    pub(crate) fn record_v2_forwarded_notification(&self, method: &str) {
        let tags = [("method", method)];

        self.v2_connection_health
            .record_forwarded_notification(method);

        if let Some(metrics) = &self.metrics
            && let Err(err) = metrics.counter(V2_FORWARDED_NOTIFICATION_COUNT_METRIC, 1, &tags)
        {
            tracing::warn!("failed to record gateway v2 forwarded notification metric: {err}");
        }
    }

    pub(crate) fn record_v2_notification_send_failure(&self, method: &str, outcome: &str) {
        let tags = [("method", method), ("outcome", outcome)];

        self.v2_connection_health
            .record_notification_send_failure(method, outcome);

        if let Some(metrics) = &self.metrics
            && let Err(err) = metrics.counter(V2_NOTIFICATION_SEND_FAILURE_COUNT_METRIC, 1, &tags)
        {
            tracing::warn!("failed to record gateway v2 notification send failure metric: {err}");
        }
    }

    pub(crate) fn record_v2_server_request_forward_send_failure(
        &self,
        method: &str,
        outcome: &str,
    ) {
        let tags = [("method", method), ("outcome", outcome)];

        self.v2_connection_health
            .record_server_request_forward_send_failure(method, outcome);

        if let Some(metrics) = &self.metrics
            && let Err(err) = metrics.counter(
                V2_SERVER_REQUEST_FORWARD_SEND_FAILURE_COUNT_METRIC,
                1,
                &tags,
            )
        {
            tracing::warn!(
                "failed to record gateway v2 server-request forward send failure metric: {err}"
            );
        }
    }

    pub(crate) fn record_v2_server_request_answer_delivery_failure(&self, response_kind: &str) {
        let tags = [("response_kind", response_kind)];

        self.v2_connection_health
            .record_server_request_answer_delivery_failure(response_kind);

        if let Some(metrics) = &self.metrics
            && let Err(err) = metrics.counter(
                V2_SERVER_REQUEST_ANSWER_DELIVERY_FAILURE_COUNT_METRIC,
                1,
                &tags,
            )
        {
            tracing::warn!(
                "failed to record gateway v2 server-request answer delivery failure metric: {err}"
            );
        }
    }

    pub(crate) fn record_v2_server_request_rejection_delivery_failure(&self, method: &str) {
        let tags = [("method", method)];

        self.v2_connection_health
            .record_server_request_rejection_delivery_failure(method);

        if let Some(metrics) = &self.metrics
            && let Err(err) = metrics.counter(
                V2_SERVER_REQUEST_REJECTION_DELIVERY_FAILURE_COUNT_METRIC,
                1,
                &tags,
            )
        {
            tracing::warn!(
                "failed to record gateway v2 server-request rejection delivery failure metric: {err}"
            );
        }
    }

    pub(crate) fn record_v2_server_request_lifecycle_event(&self, event: &str, method: &str) {
        self.record_v2_server_request_lifecycle_events(event, method, 1);
    }

    pub(crate) fn record_v2_server_request_lifecycle_events(
        &self,
        event: &str,
        method: &str,
        count: i64,
    ) {
        if count <= 0 {
            return;
        }

        self.v2_connection_health
            .record_server_request_lifecycle_events(event, method, count as usize);

        let tags = [("event", event), ("method", method)];

        if let Some(metrics) = &self.metrics
            && let Err(err) =
                metrics.counter(V2_SERVER_REQUEST_LIFECYCLE_EVENT_COUNT_METRIC, count, &tags)
        {
            tracing::warn!("failed to record gateway v2 server-request lifecycle metric: {err}");
        }
    }

    pub(crate) fn record_v2_protocol_violation(&self, phase: &str, reason: &str) {
        self.v2_connection_health
            .record_protocol_violation(phase, reason);

        self.record_v2_protocol_violation_metric(phase, reason);
    }

    pub(crate) fn record_v2_worker_protocol_violation(
        &self,
        worker_id: Option<usize>,
        phase: &str,
        reason: &str,
    ) {
        self.v2_connection_health
            .record_protocol_violation_for_worker(phase, reason, worker_id);

        self.record_v2_protocol_violation_metric(phase, reason);
    }

    fn record_v2_protocol_violation_metric(&self, phase: &str, reason: &str) {
        let tags = [("phase", phase), ("reason", reason)];

        if let Some(metrics) = &self.metrics
            && let Err(err) = metrics.counter(V2_PROTOCOL_VIOLATION_COUNT_METRIC, 1, &tags)
        {
            tracing::warn!("failed to record gateway v2 protocol violation metric: {err}");
        }
    }

    pub(crate) fn record_v2_downstream_backpressure(&self, worker_id: Option<usize>) {
        self.v2_connection_health
            .record_downstream_backpressure(worker_id);

        let worker_id =
            worker_id.map_or_else(|| "none".to_string(), |worker_id| worker_id.to_string());
        let tags = [("worker_id", worker_id.as_str())];

        if let Some(metrics) = &self.metrics
            && let Err(err) = metrics.counter(V2_DOWNSTREAM_BACKPRESSURE_COUNT_METRIC, 1, &tags)
        {
            tracing::warn!("failed to record gateway v2 downstream backpressure metric: {err}");
        }
    }

    pub(crate) fn record_v2_client_send_timeout(&self) {
        self.v2_connection_health.record_client_send_timeout();

        if let Some(metrics) = &self.metrics
            && let Err(err) = metrics.counter(V2_CLIENT_SEND_TIMEOUT_COUNT_METRIC, 1, &[])
        {
            tracing::warn!("failed to record gateway v2 client send timeout metric: {err}");
        }
    }

    pub(crate) fn record_v2_thread_list_deduplication(&self, selected_worker_id: Option<usize>) {
        self.v2_connection_health
            .record_thread_list_deduplication(selected_worker_id);

        let selected_worker_id = selected_worker_id
            .map_or_else(|| "none".to_string(), |worker_id| worker_id.to_string());
        let tags = [("selected_worker_id", selected_worker_id.as_str())];

        if let Some(metrics) = &self.metrics
            && let Err(err) = metrics.counter(V2_THREAD_LIST_DEDUPLICATION_COUNT_METRIC, 1, &tags)
        {
            tracing::warn!("failed to record gateway v2 thread-list deduplication metric: {err}");
        }
    }

    pub(crate) fn record_v2_thread_route_recovery(&self, outcome: &str) {
        self.v2_connection_health
            .record_thread_route_recovery(outcome);

        let tags = [("outcome", outcome)];

        if let Some(metrics) = &self.metrics
            && let Err(err) = metrics.counter(V2_THREAD_ROUTE_RECOVERY_COUNT_METRIC, 1, &tags)
        {
            tracing::warn!("failed to record gateway v2 thread route recovery metric: {err}");
        }
    }

    pub(crate) fn record_v2_degraded_thread_discovery(
        &self,
        method: &str,
        reconnect_backoff_active: bool,
    ) {
        self.v2_connection_health
            .record_degraded_thread_discovery(method, reconnect_backoff_active);

        let reconnect_backoff_active = if reconnect_backoff_active {
            "true"
        } else {
            "false"
        };
        let tags = [
            ("method", method),
            ("reconnect_backoff_active", reconnect_backoff_active),
        ];

        if let Some(metrics) = &self.metrics
            && let Err(err) = metrics.counter(V2_DEGRADED_THREAD_DISCOVERY_COUNT_METRIC, 1, &tags)
        {
            tracing::warn!("failed to record gateway v2 degraded thread discovery metric: {err}");
        }
    }

    pub(crate) fn record_v2_account_capacity_event(
        &self,
        worker_id: usize,
        event: &str,
        context: Option<&GatewayRequestContext>,
        reason: Option<&str>,
    ) {
        self.v2_connection_health.record_account_capacity_event(
            worker_id,
            event,
            context.map(|context| context.tenant_id.as_str()),
            context.and_then(|context| context.project_id.as_deref()),
            reason,
        );

        let worker_id = worker_id.to_string();
        let tags = [("worker_id", worker_id.as_str()), ("event", event)];

        if let Some(metrics) = &self.metrics
            && let Err(err) = metrics.counter(V2_ACCOUNT_CAPACITY_EVENT_COUNT_METRIC, 1, &tags)
        {
            tracing::warn!("failed to record gateway v2 account capacity event metric: {err}");
        }
    }
}
