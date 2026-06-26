use super::*;

#[test]
pub(crate) fn log_deduplicated_thread_list_entry_includes_selected_and_discarded_workers() {
    let logs = capture_logs(|| {
        log_deduplicated_thread_list_entry(
            &GatewayRequestContext {
                tenant_id: "tenant-visible".to_string(),
                project_id: Some("project-visible".to_string()),
            },
            DeduplicatedThreadListEntryLog {
                thread_id: "thread-visible",
                selected_worker_id: Some(2),
                selected_worker_websocket_url: "ws://worker-c.invalid",
                discarded_worker_id: Some(7),
                discarded_worker_websocket_url: "ws://worker-h.invalid",
                selected_updated_at: 40,
                discarded_updated_at: 32,
                selected_created_at: 12,
                discarded_created_at: 8,
            },
        );
    });

    assert!(logs.contains("deduplicating repeated thread/list entry across downstream workers"));
    assert!(logs.contains("tenant-visible"));
    assert!(logs.contains("project-visible"));
    assert!(logs.contains("thread_id=\"thread-visible\""));
    assert!(logs.contains("selected_worker_id=Some(2)"));
    assert!(logs.contains("selected_worker_websocket_url=\"ws://worker-c.invalid\""));
    assert!(logs.contains("discarded_worker_id=Some(7)"));
    assert!(logs.contains("discarded_worker_websocket_url=\"ws://worker-h.invalid\""));
    assert!(logs.contains("selected_updated_at=40"));
    assert!(logs.contains("discarded_updated_at=32"));
    assert!(logs.contains("selected_created_at=12"));
    assert!(logs.contains("discarded_created_at=8"));
}

#[test]
pub(crate) fn log_recovered_visible_thread_worker_route_includes_scope_and_worker() {
    let logs = capture_logs(|| {
        super::super::super::super::log_recovered_visible_thread_worker_route(
            &GatewayRequestContext {
                tenant_id: "tenant-visible".to_string(),
                project_id: Some("project-visible".to_string()),
            },
            "thread-visible",
            Some(3),
            "ws://worker-d.invalid",
        );
    });

    assert!(
        logs.contains("recovered missing visible thread route via downstream thread/read probe")
    );
    assert!(logs.contains("tenant-visible"));
    assert!(logs.contains("project-visible"));
    assert!(logs.contains("thread_id=\"thread-visible\""));
    assert!(logs.contains("worker_id=Some(3)"));
    assert!(logs.contains("worker_websocket_url=\"ws://worker-d.invalid\""));
}

#[test]
pub(crate) fn log_failed_visible_thread_worker_route_recovery_includes_attempted_workers() {
    let logs = capture_logs(|| {
        super::super::super::super::log_failed_visible_thread_worker_route_recovery(
            &GatewayRequestContext {
                tenant_id: "tenant-visible".to_string(),
                project_id: Some("project-visible".to_string()),
            },
            "thread-visible",
            &[Some(1), Some(4), None],
            &[
                "ws://worker-b.invalid",
                "ws://worker-e.invalid",
                "<unknown>",
            ],
        );
    });

    assert!(
        logs.contains("failed to recover visible thread route via downstream thread/read probe")
    );
    assert!(logs.contains("tenant-visible"));
    assert!(logs.contains("project-visible"));
    assert!(logs.contains("thread_id=\"thread-visible\""));
    assert!(logs.contains("attempted_worker_ids=[Some(1), Some(4), None]"));
    assert!(logs.contains(
            "attempted_worker_websocket_urls=[\"ws://worker-b.invalid\", \"ws://worker-e.invalid\", \"<unknown>\"]"
        ));
}
