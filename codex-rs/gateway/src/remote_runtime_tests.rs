use super::RemoteWorkerGatewayRuntime;
use crate::api::GatewayAccountCapacityStatus;
use crate::api::GatewayExecutionMode;
use crate::api::GatewayHealthStatus;
use crate::api::GatewayServerRequestKind;
use crate::api::GatewayServerRequestResponse;
use crate::api::GatewaySortDirection;
use crate::api::GatewayThread;
use crate::api::GatewayThreadSortKey;
use crate::api::GatewayThreadStatus;
use crate::api::GatewayV2CompatibilityMode;
use crate::api::GatewayV2ConnectionHealth;
use crate::api::GatewayV2TransportConfig;
use crate::api::ListThreadsRequest;
use crate::api::ResolveServerRequestRequest;
use crate::config::GatewayRemoteSelectionPolicy;
use crate::error::GatewayError;
use crate::observability::GatewayObservability;
use crate::remote_health::RemoteWorkerHealthRegistry;
use crate::runtime::GatewayRuntime;
use crate::scope::GatewayRequestContext;
use crate::scope::GatewayScopeRegistry;
use crate::v2_connection_health::GatewayV2ConnectionHealthRegistry;
use codex_app_server_protocol::CommandExecutionApprovalDecision;
use codex_app_server_protocol::JSONRPCErrorError;
use codex_app_server_protocol::RequestId;
use opentelemetry_sdk::metrics::data::AggregatedMetrics;
use opentelemetry_sdk::metrics::data::MetricData;
use pretty_assertions::assert_eq;
use std::collections::BTreeMap;
use std::collections::HashMap;
use std::sync::Arc;
use std::sync::atomic::AtomicI64;
use std::sync::atomic::AtomicUsize;
use std::time::Duration;
use tokio::sync::broadcast;

fn test_thread(id: &str, created_at: i64, updated_at: i64) -> GatewayThread {
    GatewayThread {
        id: id.to_string(),
        preview: id.to_string(),
        ephemeral: false,
        model_provider: "openai".to_string(),
        created_at,
        updated_at,
        status: GatewayThreadStatus::Idle,
    }
}

fn empty_v2_connection_health() -> Arc<GatewayV2ConnectionHealthRegistry> {
    Arc::new(GatewayV2ConnectionHealthRegistry::default())
}

fn test_metrics() -> codex_otel::MetricsClient {
    codex_otel::MetricsClient::new(
        codex_otel::MetricsConfig::in_memory(
            "test",
            "codex-gateway",
            env!("CARGO_PKG_VERSION"),
            opentelemetry_sdk::metrics::InMemoryMetricExporter::default(),
        )
        .with_runtime_reader(),
    )
    .expect("metrics")
}

#[test]
fn detects_account_capacity_errors_without_treating_all_server_errors_as_capacity() {
    assert!(super::is_account_capacity_error(&JSONRPCErrorError {
        code: 429,
        data: None,
        message: "too many requests".to_string(),
    }));
    assert!(super::is_account_capacity_error(&JSONRPCErrorError {
        code: -32000,
        data: None,
        message: "workspace_member_usage_limit_reached".to_string(),
    }));
    assert!(!super::is_account_capacity_error(&JSONRPCErrorError {
        code: -32602,
        data: None,
        message: "cwd must be absolute".to_string(),
    }));
}

#[test]
fn sorts_threads_by_created_at_desc_by_default() {
    let runtime = RemoteWorkerGatewayRuntime {
        workers: Vec::new(),
        selection_policy: GatewayRemoteSelectionPolicy::RoundRobin,
        next_worker: AtomicUsize::new(0),
        next_request_id: Arc::new(AtomicI64::new(1)),
        events: broadcast::channel(4).0,
        scope_registry: Arc::new(GatewayScopeRegistry::default()),
        worker_health: Arc::new(crate::remote_health::RemoteWorkerHealthRegistry::new(
            Vec::new(),
        )),
        v2_transport: GatewayV2TransportConfig {
            initialize_timeout_seconds: 30,
            client_send_timeout_seconds: 10,
            reconnect_retry_backoff_seconds: 1,
            max_pending_server_requests: 64,
            max_pending_client_requests: 64,
        },
        v2_connection_health: empty_v2_connection_health(),
        observability: GatewayObservability::default(),
    };
    let mut threads = vec![
        test_thread("thread-1", 1, 10),
        test_thread("thread-2", 3, 5),
        test_thread("thread-3", 2, 7),
    ];

    runtime.sort_threads(&mut threads, &ListThreadsRequest::default());

    assert_eq!(
        threads
            .into_iter()
            .map(|thread| thread.id)
            .collect::<Vec<_>>(),
        vec![
            "thread-2".to_string(),
            "thread-3".to_string(),
            "thread-1".to_string()
        ]
    );
}

#[test]
fn sorts_threads_by_updated_at_ascending() {
    let runtime = RemoteWorkerGatewayRuntime {
        workers: Vec::new(),
        selection_policy: GatewayRemoteSelectionPolicy::RoundRobin,
        next_worker: AtomicUsize::new(0),
        next_request_id: Arc::new(AtomicI64::new(1)),
        events: broadcast::channel(4).0,
        scope_registry: Arc::new(GatewayScopeRegistry::default()),
        worker_health: Arc::new(crate::remote_health::RemoteWorkerHealthRegistry::new(
            Vec::new(),
        )),
        v2_transport: GatewayV2TransportConfig {
            initialize_timeout_seconds: 30,
            client_send_timeout_seconds: 10,
            reconnect_retry_backoff_seconds: 1,
            max_pending_server_requests: 64,
            max_pending_client_requests: 64,
        },
        v2_connection_health: empty_v2_connection_health(),
        observability: GatewayObservability::default(),
    };
    let mut threads = vec![
        test_thread("thread-1", 1, 10),
        test_thread("thread-2", 3, 5),
        test_thread("thread-3", 2, 7),
    ];

    runtime.sort_threads(
        &mut threads,
        &ListThreadsRequest {
            sort_key: Some(GatewayThreadSortKey::UpdatedAt),
            sort_direction: Some(GatewaySortDirection::Asc),
            ..ListThreadsRequest::default()
        },
    );

    assert_eq!(
        threads
            .into_iter()
            .map(|thread| thread.id)
            .collect::<Vec<_>>(),
        vec![
            "thread-2".to_string(),
            "thread-3".to_string(),
            "thread-1".to_string()
        ]
    );
}

#[test]
fn parses_gateway_thread_cursor_offsets() {
    assert_eq!(
        RemoteWorkerGatewayRuntime::decode_cursor(None).expect("offset"),
        0
    );
    assert_eq!(
        RemoteWorkerGatewayRuntime::decode_cursor(Some("offset:12")).expect("offset"),
        12
    );
}

#[test]
fn rejects_invalid_gateway_thread_cursor_offsets() {
    let error = RemoteWorkerGatewayRuntime::decode_cursor(Some("thread-12"))
        .expect_err("cursor should fail");

    assert!(matches!(
        error,
        GatewayError::InvalidRequest(message) if message == "invalid gateway thread cursor: thread-12"
    ));
}

#[test]
fn preserves_thread_scope_worker_id() {
    let registry = GatewayScopeRegistry::default();
    let context = GatewayRequestContext::default();
    registry.register_thread_with_worker("thread-1".to_string(), context, Some(2));

    assert_eq!(registry.thread_worker_id("thread-1"), Some(2));
}

#[test]
fn active_thread_request_fails_closed_when_account_capacity_is_exhausted() {
    let (events, mut event_rx) = broadcast::channel(4);
    let context = GatewayRequestContext {
        tenant_id: "tenant-a".to_string(),
        project_id: Some("project-a".to_string()),
    };
    let scope_registry = Arc::new(GatewayScopeRegistry::default());
    scope_registry.register_thread_with_worker("thread-1".to_string(), context.clone(), Some(1));
    let worker_health = Arc::new(RemoteWorkerHealthRegistry::new_with_accounts(vec![
        (
            "ws://127.0.0.1:8081".to_string(),
            Some("acct-a".to_string()),
        ),
        (
            "ws://127.0.0.1:8082".to_string(),
            Some("acct-b".to_string()),
        ),
    ]));
    worker_health.mark_account_exhausted_for_worker(1, "quota exceeded".to_string());
    let runtime = RemoteWorkerGatewayRuntime {
        workers: Vec::new(),
        selection_policy: GatewayRemoteSelectionPolicy::RoundRobin,
        next_worker: AtomicUsize::new(0),
        next_request_id: Arc::new(AtomicI64::new(1)),
        events,
        scope_registry,
        worker_health,
        v2_transport: GatewayV2TransportConfig {
            initialize_timeout_seconds: 30,
            client_send_timeout_seconds: 10,
            reconnect_retry_backoff_seconds: 1,
            max_pending_server_requests: 64,
            max_pending_client_requests: 64,
        },
        v2_connection_health: empty_v2_connection_health(),
        observability: GatewayObservability::default(),
    };

    let error = runtime
        .fail_if_active_thread_account_exhausted(&context, "turn/start", "thread-1")
        .expect_err("active thread request should fail closed");

    assert!(matches!(
        error,
        GatewayError::Upstream(message)
            if message == "thread thread-1 is pinned to worker 1 with exhausted account capacity for turn/start"
    ));
    let event = event_rx.try_recv().expect("handoff event");
    assert_eq!(event.method, "gateway/accountActiveThreadHandoffFailed");
    assert_eq!(event.thread_id.as_deref(), Some("thread-1"));
    assert_eq!(
        event.data,
        serde_json::json!({
            "tenantId": "tenant-a",
            "projectId": "project-a",
            "method": "turn/start",
            "threadId": "thread-1",
            "exhaustedWorkerId": 1,
            "exhaustedAccountId": "acct-b",
            "reason": "thread thread-1 is pinned to worker 1 with exhausted account capacity for turn/start",
        })
    );
}

#[test]
fn downstream_turn_start_capacity_error_marks_account_exhausted() {
    let (events, mut event_rx) = broadcast::channel(4);
    let context = GatewayRequestContext {
        tenant_id: "tenant-a".to_string(),
        project_id: Some("project-a".to_string()),
    };
    let worker_health = Arc::new(RemoteWorkerHealthRegistry::new_with_accounts(vec![(
        "ws://127.0.0.1:8081".to_string(),
        Some("acct-a".to_string()),
    )]));
    let runtime = RemoteWorkerGatewayRuntime {
        workers: Vec::new(),
        selection_policy: GatewayRemoteSelectionPolicy::RoundRobin,
        next_worker: AtomicUsize::new(0),
        next_request_id: Arc::new(AtomicI64::new(1)),
        events,
        scope_registry: Arc::new(GatewayScopeRegistry::default()),
        worker_health,
        v2_transport: GatewayV2TransportConfig {
            initialize_timeout_seconds: 30,
            client_send_timeout_seconds: 10,
            reconnect_retry_backoff_seconds: 1,
            max_pending_server_requests: 64,
            max_pending_client_requests: 64,
        },
        v2_connection_health: empty_v2_connection_health(),
        observability: GatewayObservability::default(),
    };

    runtime.mark_account_capacity_exhausted(
        &context,
        "turn/start",
        0,
        "rate limit exceeded".to_string(),
    );

    assert_eq!(
        runtime.worker_health.account_capacity(0),
        Some(GatewayAccountCapacityStatus::Exhausted)
    );
    let event = event_rx.try_recv().expect("capacity event");
    assert_eq!(event.method, "gateway/accountCapacityExhausted");
    assert_eq!(
        event.data,
        serde_json::json!({
            "tenantId": "tenant-a",
            "projectId": "project-a",
            "workerId": 0,
            "accountId": "acct-a",
            "reason": "rate limit exceeded",
        })
    );
}

#[tokio::test]
async fn interrupt_turn_fails_closed_when_account_capacity_is_exhausted() {
    let (events, mut event_rx) = broadcast::channel(4);
    let context = GatewayRequestContext {
        tenant_id: "tenant-a".to_string(),
        project_id: Some("project-a".to_string()),
    };
    let scope_registry = Arc::new(GatewayScopeRegistry::default());
    scope_registry.register_thread_with_worker("thread-1".to_string(), context.clone(), Some(0));
    let worker_health = Arc::new(RemoteWorkerHealthRegistry::new_with_accounts(vec![(
        "ws://127.0.0.1:8081".to_string(),
        Some("acct-a".to_string()),
    )]));
    worker_health.mark_account_exhausted_for_worker(0, "quota exceeded".to_string());
    let runtime = RemoteWorkerGatewayRuntime {
        workers: Vec::new(),
        selection_policy: GatewayRemoteSelectionPolicy::RoundRobin,
        next_worker: AtomicUsize::new(0),
        next_request_id: Arc::new(AtomicI64::new(1)),
        events,
        scope_registry,
        worker_health,
        v2_transport: GatewayV2TransportConfig {
            initialize_timeout_seconds: 30,
            client_send_timeout_seconds: 10,
            reconnect_retry_backoff_seconds: 1,
            max_pending_server_requests: 64,
            max_pending_client_requests: 64,
        },
        v2_connection_health: empty_v2_connection_health(),
        observability: GatewayObservability::default(),
    };

    let error = runtime
        .interrupt_turn(context, "thread-1".to_string(), "turn-1".to_string())
        .await
        .expect_err("interrupt should fail closed before worker lookup");

    assert!(matches!(
        error,
        GatewayError::Upstream(message)
            if message == "thread thread-1 is pinned to worker 0 with exhausted account capacity for turn/interrupt"
    ));
    let event = event_rx.try_recv().expect("handoff event");
    assert_eq!(event.method, "gateway/accountActiveThreadHandoffFailed");
    assert_eq!(event.thread_id.as_deref(), Some("thread-1"));
    assert_eq!(
        event.data,
        serde_json::json!({
            "tenantId": "tenant-a",
            "projectId": "project-a",
            "method": "turn/interrupt",
            "threadId": "thread-1",
            "exhaustedWorkerId": 0,
            "exhaustedAccountId": "acct-a",
            "reason": "thread thread-1 is pinned to worker 0 with exhausted account capacity for turn/interrupt",
        })
    );
}

#[tokio::test]
async fn server_request_response_fails_closed_when_account_capacity_is_exhausted() {
    let (events, mut event_rx) = broadcast::channel(4);
    let context = GatewayRequestContext {
        tenant_id: "tenant-a".to_string(),
        project_id: Some("project-a".to_string()),
    };
    let scope_registry = Arc::new(GatewayScopeRegistry::default());
    scope_registry.register_thread_with_worker("thread-1".to_string(), context.clone(), Some(0));
    scope_registry.register_pending_server_request_with_worker(
        RequestId::String("req-1".to_string()),
        GatewayServerRequestKind::ToolRequestUserInput,
        context.clone(),
        "thread-1".to_string(),
        Some(0),
    );
    let worker_health = Arc::new(RemoteWorkerHealthRegistry::new_with_accounts(vec![(
        "ws://127.0.0.1:8081".to_string(),
        Some("acct-a".to_string()),
    )]));
    worker_health.mark_account_exhausted_for_worker(0, "quota exceeded".to_string());
    let metrics = test_metrics();
    let runtime = RemoteWorkerGatewayRuntime {
        workers: Vec::new(),
        selection_policy: GatewayRemoteSelectionPolicy::RoundRobin,
        next_worker: AtomicUsize::new(0),
        next_request_id: Arc::new(AtomicI64::new(1)),
        events,
        scope_registry: scope_registry.clone(),
        worker_health,
        v2_transport: GatewayV2TransportConfig {
            initialize_timeout_seconds: 30,
            client_send_timeout_seconds: 10,
            reconnect_retry_backoff_seconds: 1,
            max_pending_server_requests: 64,
            max_pending_client_requests: 64,
        },
        v2_connection_health: empty_v2_connection_health(),
        observability: GatewayObservability::new(Some(metrics.clone()), true),
    };

    let request_id = RequestId::String("req-1".to_string());
    let error = runtime
        .resolve_server_request(
            context,
            ResolveServerRequestRequest {
                request_id: request_id.clone(),
                response: GatewayServerRequestResponse::ToolRequestUserInput {
                    answers: HashMap::new(),
                },
            },
        )
        .await
        .expect_err("server request response should fail closed before worker lookup");

    assert!(matches!(
        error,
        GatewayError::Upstream(message)
            if message == "thread thread-1 is pinned to worker 0 with exhausted account capacity for serverRequest/respond"
    ));
    assert!(scope_registry.pending_server_request(&request_id).is_some());
    let event = event_rx.try_recv().expect("handoff event");
    assert_eq!(event.method, "gateway/accountActiveThreadHandoffFailed");
    assert_eq!(event.thread_id.as_deref(), Some("thread-1"));
    assert_eq!(
        event.data,
        serde_json::json!({
            "tenantId": "tenant-a",
            "projectId": "project-a",
            "method": "serverRequest/respond",
            "threadId": "thread-1",
            "exhaustedWorkerId": 0,
            "exhaustedAccountId": "acct-a",
            "reason": "thread thread-1 is pinned to worker 0 with exhausted account capacity for serverRequest/respond",
        })
    );
    health_tests::assert_server_request_lifecycle_and_delivery_failure_metrics(
        &metrics,
        "toolRequestUserInput",
    );
}

#[tokio::test]
async fn server_request_response_fails_closed_using_pending_worker_without_thread_route() {
    let (events, mut event_rx) = broadcast::channel(4);
    let context = GatewayRequestContext {
        tenant_id: "tenant-a".to_string(),
        project_id: Some("project-a".to_string()),
    };
    let scope_registry = Arc::new(GatewayScopeRegistry::default());
    scope_registry.register_pending_server_request_with_worker(
        RequestId::String("req-1".to_string()),
        GatewayServerRequestKind::ToolRequestUserInput,
        context.clone(),
        "thread-1".to_string(),
        Some(0),
    );
    let worker_health = Arc::new(RemoteWorkerHealthRegistry::new_with_accounts(vec![(
        "ws://127.0.0.1:8081".to_string(),
        Some("acct-a".to_string()),
    )]));
    worker_health.mark_account_exhausted_for_worker(0, "quota exceeded".to_string());
    let runtime = RemoteWorkerGatewayRuntime {
        workers: Vec::new(),
        selection_policy: GatewayRemoteSelectionPolicy::RoundRobin,
        next_worker: AtomicUsize::new(0),
        next_request_id: Arc::new(AtomicI64::new(1)),
        events,
        scope_registry: scope_registry.clone(),
        worker_health,
        v2_transport: GatewayV2TransportConfig {
            initialize_timeout_seconds: 30,
            client_send_timeout_seconds: 10,
            reconnect_retry_backoff_seconds: 1,
            max_pending_server_requests: 64,
            max_pending_client_requests: 64,
        },
        v2_connection_health: empty_v2_connection_health(),
        observability: GatewayObservability::default(),
    };

    let request_id = RequestId::String("req-1".to_string());
    let error = runtime
        .resolve_server_request(
            context,
            ResolveServerRequestRequest {
                request_id: request_id.clone(),
                response: GatewayServerRequestResponse::ToolRequestUserInput {
                    answers: HashMap::new(),
                },
            },
        )
        .await
        .expect_err("server request response should fail closed before worker lookup");

    assert!(matches!(
        error,
        GatewayError::Upstream(message)
            if message == "thread thread-1 is pinned to worker 0 with exhausted account capacity for serverRequest/respond"
    ));
    assert!(scope_registry.pending_server_request(&request_id).is_some());
    let event = event_rx.try_recv().expect("handoff event");
    assert_eq!(event.method, "gateway/accountActiveThreadHandoffFailed");
    assert_eq!(event.thread_id.as_deref(), Some("thread-1"));
    assert_eq!(
        event.data,
        serde_json::json!({
            "tenantId": "tenant-a",
            "projectId": "project-a",
            "method": "serverRequest/respond",
            "threadId": "thread-1",
            "exhaustedWorkerId": 0,
            "exhaustedAccountId": "acct-a",
            "reason": "thread thread-1 is pinned to worker 0 with exhausted account capacity for serverRequest/respond",
        })
    );
}

#[tokio::test]
async fn server_request_response_type_mismatch_records_invalid_response_lifecycle() {
    let (events, _event_rx) = broadcast::channel(4);
    let context = GatewayRequestContext {
        tenant_id: "tenant-a".to_string(),
        project_id: Some("project-a".to_string()),
    };
    let scope_registry = Arc::new(GatewayScopeRegistry::default());
    scope_registry.register_pending_server_request_with_worker(
        RequestId::String("req-1".to_string()),
        GatewayServerRequestKind::ToolRequestUserInput,
        context.clone(),
        "thread-1".to_string(),
        Some(0),
    );
    let metrics = test_metrics();
    let runtime = RemoteWorkerGatewayRuntime {
        workers: Vec::new(),
        selection_policy: GatewayRemoteSelectionPolicy::RoundRobin,
        next_worker: AtomicUsize::new(0),
        next_request_id: Arc::new(AtomicI64::new(1)),
        events,
        scope_registry: scope_registry.clone(),
        worker_health: Arc::new(RemoteWorkerHealthRegistry::new_with_accounts(Vec::new())),
        v2_transport: GatewayV2TransportConfig {
            initialize_timeout_seconds: 30,
            client_send_timeout_seconds: 10,
            reconnect_retry_backoff_seconds: 1,
            max_pending_server_requests: 64,
            max_pending_client_requests: 64,
        },
        v2_connection_health: empty_v2_connection_health(),
        observability: GatewayObservability::new(Some(metrics.clone()), true),
    };

    let request_id = RequestId::String("req-1".to_string());
    let error = runtime
        .resolve_server_request(
            context,
            ResolveServerRequestRequest {
                request_id: request_id.clone(),
                response: GatewayServerRequestResponse::CommandExecutionApproval {
                    decision: CommandExecutionApprovalDecision::Decline,
                },
            },
        )
        .await
        .expect_err("mismatched response type should be rejected");

    assert!(matches!(
        error,
        GatewayError::InvalidRequest(message)
            if message == "server request response type does not match pending request"
    ));
    assert!(scope_registry.pending_server_request(&request_id).is_some());
    health_tests::assert_server_request_lifecycle_metric(
        &metrics,
        "client_server_request_invalid_response",
        "commandExecutionApproval",
    );
}

#[tokio::test]
async fn server_request_response_keeps_pending_request_when_remote_worker_route_is_missing() {
    let (events, _event_rx) = broadcast::channel(4);
    let context = GatewayRequestContext {
        tenant_id: "tenant-a".to_string(),
        project_id: Some("project-a".to_string()),
    };
    let scope_registry = Arc::new(GatewayScopeRegistry::default());
    scope_registry.register_thread_with_worker("thread-1".to_string(), context.clone(), Some(9));
    scope_registry.register_pending_server_request_with_worker(
        RequestId::String("req-1".to_string()),
        GatewayServerRequestKind::ToolRequestUserInput,
        context.clone(),
        "thread-1".to_string(),
        Some(9),
    );
    let worker_health = Arc::new(RemoteWorkerHealthRegistry::new_with_accounts(Vec::new()));
    let runtime = RemoteWorkerGatewayRuntime {
        workers: Vec::new(),
        selection_policy: GatewayRemoteSelectionPolicy::RoundRobin,
        next_worker: AtomicUsize::new(0),
        next_request_id: Arc::new(AtomicI64::new(1)),
        events,
        scope_registry: scope_registry.clone(),
        worker_health,
        v2_transport: GatewayV2TransportConfig {
            initialize_timeout_seconds: 30,
            client_send_timeout_seconds: 10,
            reconnect_retry_backoff_seconds: 1,
            max_pending_server_requests: 64,
            max_pending_client_requests: 64,
        },
        v2_connection_health: empty_v2_connection_health(),
        observability: GatewayObservability::default(),
    };

    let request_id = RequestId::String("req-1".to_string());
    let error = runtime
        .resolve_server_request(
            context,
            ResolveServerRequestRequest {
                request_id: request_id.clone(),
                response: GatewayServerRequestResponse::ToolRequestUserInput {
                    answers: HashMap::new(),
                },
            },
        )
        .await
        .expect_err("server request response should fail before clearing pending request");

    assert!(matches!(
        error,
        GatewayError::Upstream(message)
            if message == "remote worker route 9 is unavailable"
    ));
    let pending_request = scope_registry
        .pending_server_request(&request_id)
        .expect("pending request should still be registered");
    assert_eq!(
        pending_request.kind,
        GatewayServerRequestKind::ToolRequestUserInput
    );
    assert_eq!(
        pending_request.context,
        GatewayRequestContext {
            tenant_id: "tenant-a".to_string(),
            project_id: Some("project-a".to_string()),
        }
    );
    assert_eq!(pending_request.thread_id, "thread-1");
    assert_eq!(pending_request.worker_id, Some(9));
    assert!(pending_request.registered_at > 0);
}

#[test]
fn reports_reconnecting_remote_health_with_retry_metadata() {
    let worker_health = Arc::new(RemoteWorkerHealthRegistry::new(vec![
        "ws://127.0.0.1:8081".to_string(),
    ]));
    worker_health.mark_reconnecting(
        0,
        Some("remote app server event stream ended".to_string()),
        Duration::from_millis(250),
    );
    let runtime = RemoteWorkerGatewayRuntime {
        workers: Vec::new(),
        selection_policy: GatewayRemoteSelectionPolicy::RoundRobin,
        next_worker: AtomicUsize::new(0),
        next_request_id: Arc::new(AtomicI64::new(1)),
        events: broadcast::channel(4).0,
        scope_registry: Arc::new(GatewayScopeRegistry::default()),
        worker_health,
        v2_transport: GatewayV2TransportConfig {
            initialize_timeout_seconds: 30,
            client_send_timeout_seconds: 10,
            reconnect_retry_backoff_seconds: 1,
            max_pending_server_requests: 64,
            max_pending_client_requests: 64,
        },
        v2_connection_health: empty_v2_connection_health(),
        observability: GatewayObservability::default(),
    };

    let health = runtime.health();

    assert_eq!(health.status, GatewayHealthStatus::Unavailable);
    assert_eq!(
        health.v2_compatibility,
        GatewayV2CompatibilityMode::RemoteSingleWorker
    );
    assert_eq!(health.remote_account_labels_complete, Some(true));
    assert_eq!(health.remote_unlabeled_account_worker_count, Some(0));
    assert_eq!(health.remote_unlabeled_account_worker_ids, Some(Vec::new()));
    assert_eq!(health.remote_unlabeled_account_workers, Some(Vec::new()));
    let remote_workers = health.remote_workers.expect("remote workers");
    assert_eq!(remote_workers.len(), 1);
    assert_eq!(remote_workers[0].worker_id, 0);
    assert_eq!(remote_workers[0].websocket_url, "ws://127.0.0.1:8081");
    assert_eq!(remote_workers[0].healthy, false);
    assert_eq!(remote_workers[0].reconnecting, true);
    assert_eq!(remote_workers[0].reconnect_attempt_count, 0);
    assert_eq!(
        remote_workers[0].last_error.as_deref(),
        Some("remote app server event stream ended")
    );
    assert_eq!(remote_workers[0].last_state_change_at.is_some(), true);
    assert_eq!(remote_workers[0].last_error_at.is_some(), true);
    assert_eq!(remote_workers[0].next_reconnect_at.is_some(), true);
    assert_eq!(
        remote_workers[0]
            .reconnect_backoff_remaining_seconds
            .is_some_and(|remaining_seconds| remaining_seconds >= 0),
        true
    );
}

#[path = "remote_runtime_tests_health.rs"]
mod health_tests;
