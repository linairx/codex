use super::*;
use pretty_assertions::assert_eq;

#[test]
pub(crate) fn pending_client_responses_settle_completed_responses_before_teardown() {
    let (tx, mut rx) = mpsc::channel(2);
    let mut pending_client_responses = super::super::super::super::PendingClientResponses {
        tx,
        tasks: Vec::new(),
        count: 2,
        active: HashMap::from([
            (
                RequestId::String("command-exec-complete".to_string()),
                super::super::super::super::PendingClientRequestRoute {
                    method: "command/exec".to_string(),
                    request_context: GatewayRequestContext {
                        tenant_id: "tenant-visible".to_string(),
                        project_id: Some("project-visible".to_string()),
                    },
                    worker_id: Some(0),
                    worker_websocket_url: "ws://worker-a.invalid".to_string(),
                    started_at: Instant::now(),
                },
            ),
            (
                RequestId::String("command-exec-pending".to_string()),
                super::super::super::super::PendingClientRequestRoute {
                    method: "command/exec".to_string(),
                    request_context: GatewayRequestContext {
                        tenant_id: "tenant-visible".to_string(),
                        project_id: Some("project-visible".to_string()),
                    },
                    worker_id: Some(1),
                    worker_websocket_url: "ws://worker-b.invalid".to_string(),
                    started_at: Instant::now(),
                },
            ),
        ]),
    };

    pending_client_responses
        .tx
        .try_send(super::super::super::super::PendingClientResponse {
            request_id: RequestId::String("command-exec-complete".to_string()),
            request_context: GatewayRequestContext {
                tenant_id: "tenant-visible".to_string(),
                project_id: Some("project-visible".to_string()),
            },
            method: "command/exec".to_string(),
            started_at: Instant::now(),
            result: Ok(Ok(serde_json::json!({ "exitCode": 0 }))),
        })
        .expect("completed background response should enqueue");

    pending_client_responses.settle_completed_responses(&mut rx);

    assert_eq!(pending_client_responses.count, 1);
    assert_eq!(
        pending_client_responses
            .active
            .keys()
            .cloned()
            .collect::<Vec<_>>(),
        vec![RequestId::String("command-exec-pending".to_string())]
    );
}

#[test]
pub(crate) fn log_aborted_pending_client_requests_includes_scope_outcome_and_request_ids() {
    let logs = capture_logs(|| {
        super::super::super::super::log_aborted_pending_client_requests(
            &GatewayRequestContext {
                tenant_id: "tenant-visible".to_string(),
                project_id: Some("project-visible".to_string()),
            },
            "client_disconnected",
            Some("gateway websocket receive failed: closed"),
            &HashMap::from([
                (
                    RequestId::String("command-exec-1".to_string()),
                    super::super::super::super::PendingClientRequestRoute {
                        method: "command/exec".to_string(),
                        request_context: GatewayRequestContext {
                            tenant_id: "tenant-visible".to_string(),
                            project_id: Some("project-visible".to_string()),
                        },
                        worker_id: Some(0),
                        worker_websocket_url: "ws://worker-a.invalid".to_string(),
                        started_at: Instant::now(),
                    },
                ),
                (
                    RequestId::String("command-exec-2".to_string()),
                    super::super::super::super::PendingClientRequestRoute {
                        method: "command/exec".to_string(),
                        request_context: GatewayRequestContext {
                            tenant_id: "tenant-visible".to_string(),
                            project_id: Some("project-visible".to_string()),
                        },
                        worker_id: Some(1),
                        worker_websocket_url: "ws://worker-b.invalid".to_string(),
                        started_at: Instant::now(),
                    },
                ),
            ]),
        );
    });

    assert!(logs.contains(
        "aborting pending gateway v2 client requests because the northbound connection ended"
    ));
    assert!(logs.contains("tenant-visible"));
    assert!(logs.contains("project-visible"));
    assert!(logs.contains("outcome=\"client_disconnected\""));
    assert!(logs.contains("detail=\"gateway websocket receive failed: closed\""));
    assert!(logs.contains("pending_client_request_count=2"));
    assert!(logs.contains(
        "pending_client_request_ids=[String(\"command-exec-1\"), String(\"command-exec-2\")]"
    ));
    assert!(logs.contains("pending_client_request_methods=[\"command/exec\", \"command/exec\"]"));
    assert!(logs.contains("pending_client_request_worker_ids=[0, 1]"));
    assert!(logs.contains(
            "pending_client_request_worker_websocket_urls=[\"ws://worker-a.invalid\", \"ws://worker-b.invalid\"]"
        ));
    assert!(logs.contains("pending_client_request_worker_counts=["));
    assert!(logs.contains("worker_id: Some(0)"));
    assert!(logs.contains("worker_id: Some(1)"));
    assert!(logs.contains("pending_client_request_method_counts=["));
    assert!(logs.contains("pending_client_request_count: 2"));
}

#[test]
pub(crate) fn observe_aborted_pending_client_requests_records_request_metrics_and_audit_log() {
    let metrics = in_memory_metrics();
    let observability = GatewayObservability::new(Some(metrics.clone()), true);

    let logs = capture_logs(|| {
        super::super::super::super::observe_aborted_pending_client_requests(
            &observability,
            "client_disconnected",
            &HashMap::from([(
                RequestId::String("command-exec-1".to_string()),
                super::super::super::super::PendingClientRequestRoute {
                    method: "command/exec".to_string(),
                    request_context: GatewayRequestContext {
                        tenant_id: "tenant-visible".to_string(),
                        project_id: Some("project-visible".to_string()),
                    },
                    worker_id: Some(0),
                    worker_websocket_url: "ws://worker-a.invalid".to_string(),
                    started_at: Instant::now(),
                },
            )]),
        );
    });

    assert!(logs.contains("codex_gateway.audit"), "{logs}");
    assert!(logs.contains("gateway v2 request completed"), "{logs}");
    assert!(logs.contains("method=\"command/exec\""), "{logs}");
    assert!(logs.contains("outcome=\"client_disconnected\""), "{logs}");
    assert_v2_request_metrics(&metrics, &[("command/exec", "client_disconnected", 1)]);
}

#[test]
pub(crate) fn observe_aborted_pending_client_requests_records_client_send_timeout_outcome() {
    let metrics = in_memory_metrics();
    let observability = GatewayObservability::new(Some(metrics.clone()), true);

    let logs = capture_logs(|| {
        super::super::super::super::observe_aborted_pending_client_requests(
            &observability,
            "client_send_timed_out",
            &HashMap::from([(
                RequestId::String("command-exec-timeout".to_string()),
                super::super::super::super::PendingClientRequestRoute {
                    method: "command/exec".to_string(),
                    request_context: GatewayRequestContext {
                        tenant_id: "tenant-visible".to_string(),
                        project_id: Some("project-visible".to_string()),
                    },
                    worker_id: Some(0),
                    worker_websocket_url: "ws://worker-a.invalid".to_string(),
                    started_at: Instant::now(),
                },
            )]),
        );
    });

    assert!(logs.contains("codex_gateway.audit"), "{logs}");
    assert!(logs.contains("gateway v2 request completed"), "{logs}");
    assert!(logs.contains("method=\"command/exec\""), "{logs}");
    assert!(logs.contains("outcome=\"client_send_timed_out\""), "{logs}");
    assert_v2_request_metrics(&metrics, &[("command/exec", "client_send_timed_out", 1)]);
}
