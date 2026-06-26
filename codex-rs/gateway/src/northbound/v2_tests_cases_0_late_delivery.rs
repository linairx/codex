use super::*;
use crate::northbound::v2_connection::ClientServerRequestAnswer;
use crate::northbound::v2_connection::PendingClientRequestRoute;
use crate::northbound::v2_connection::PendingClientResponses;
use crate::northbound::v2_connection::PendingServerRequestRoute;
use crate::northbound::v2_connection_runtime::deliver_client_server_request_answer;
use pretty_assertions::assert_eq;
#[test]
fn pending_client_response_settlement_removes_active_request_before_delivery() {
    let (tx, _rx) = mpsc::channel(1);
    let request_id = RequestId::String("command-exec-1".to_string());
    let mut pending_client_responses = PendingClientResponses {
        tx,
        tasks: Vec::new(),
        count: 1,
        active: HashMap::from([(
            request_id.clone(),
            PendingClientRequestRoute {
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
    };

    pending_client_responses.settle_response(&request_id);

    assert_eq!(pending_client_responses.count, 0);
    assert!(pending_client_responses.active.is_empty());
}

#[tokio::test]
async fn deliver_client_server_request_answer_records_delivery_failure_lifecycle() {
    let metrics = in_memory_metrics();
    let observability = GatewayObservability::new(Some(metrics.clone()), false);
    let admission = GatewayAdmissionController::default();
    let scope_registry = GatewayScopeRegistry::default();
    let request_context = GatewayRequestContext::default();
    let connection = GatewayV2ConnectionContext {
        admission: &admission,
        observability: &observability,
        scope_registry: &scope_registry,
        request_context: &request_context,
        client_send_timeout: Duration::from_secs(1),
        max_pending_server_requests: 1,
        max_pending_client_requests: 1,
        opt_out_notification_methods: HashSet::new(),
    };
    let (event_tx, event_rx) = mpsc::channel(1);
    let downstream = GatewayV2DownstreamRouter {
        workers: Vec::new(),
        event_tx,
        event_rx,
        shutdown_txs: Vec::new(),
        event_tasks: Vec::new(),
        next_worker: 0,
        initialized_notification_sent: true,
        active_fs_watches: HashMap::new(),
        reconnect_retry_after: HashMap::new(),
        reconnect_state: None,
    };

    let mut err = None;
    let logs = capture_logs_async(async {
        err = Some(
            deliver_client_server_request_answer(
                &downstream,
                &connection,
                RequestId::String("gateway-request".to_string()),
                PendingServerRequestRoute {
                    worker_id: Some(0),
                    worker_websocket_url: test_worker_websocket_url(Some(0)),
                    downstream_request_id: RequestId::String("downstream-request".to_string()),
                    method: "item/tool/requestUserInput".to_string(),
                    thread_id: Some("thread-visible".to_string()),
                },
                ClientServerRequestAnswer::Response(serde_json::json!({
                    "approved": true,
                })),
            )
            .await
            .expect_err("missing downstream worker should fail delivery"),
        );
    })
    .await;
    let err = err.expect("delivery should fail");

    assert!(
        err.to_string()
            .contains("no downstream server-request route for worker Some(0)"),
        "unexpected error: {err}"
    );
    assert!(
        logs.contains("failed to deliver answered server request back to downstream worker"),
        "{logs}"
    );
    assert!(logs.contains("tenant_id=\"default\""), "{logs}");
    assert!(logs.contains("response_kind=\"response\""), "{logs}");
    assert!(logs.contains("worker_id=Some(0)"), "{logs}");
    assert!(
        logs.contains("worker_websocket_url=\"ws://worker-a.invalid\""),
        "{logs}"
    );
    assert!(
        logs.contains("gateway_request_id=String(\"gateway-request\")"),
        "{logs}"
    );
    assert!(
        logs.contains("downstream_request_id=String(\"downstream-request\")"),
        "{logs}"
    );
    assert!(logs.contains("thread_id=\"thread-visible\""), "{logs}");
    assert_v2_server_request_lifecycle_and_answer_delivery_failure_metrics(
        &metrics,
        &[
            ("client_server_request_answered", "response", 1),
            ("client_server_request_delivery_failed", "response", 1),
        ],
        &[("response", 1)],
    );
}
