use super::*;
use pretty_assertions::assert_eq;

#[tokio::test]
async fn remote_multi_worker_fails_closed_when_thread_id_handoff_has_no_account_replacement() {
    let (server, client, worker_b_thread) = thread_handoff_fail_closed_test_setup!();

    let thread_handoff_event_url = format!("http://{}/v1/events", server.local_addr());
    let thread_handoff_event_task = tokio::spawn(async move {
        let mut events_response = reqwest::Client::new()
            .get(thread_handoff_event_url)
            .send()
            .await
            .expect("event stream response");
        assert_eq!(events_response.status(), reqwest::StatusCode::OK);

        let mut buffered = String::new();
        loop {
            let chunk = events_response
                .chunk()
                .await
                .expect("event stream chunk")
                .expect("event stream not closed");
            buffered.push_str(std::str::from_utf8(&chunk).expect("utf8"));

            while let Some(event_end) = buffered.find("\n\n") {
                let event = buffered[..event_end].to_string();
                buffered.drain(..event_end + 2);

                if event.contains("event: gateway/accountThreadHandoffFailed") {
                    return event;
                }
            }
        }
    });
    sleep(Duration::from_millis(100)).await;

    let resume_result: Result<ThreadResumeResponse, _> = timeout(
        Duration::from_secs(5),
        client.request_typed(ClientRequest::ThreadResume {
            request_id: RequestId::Integer(4),
            params: ThreadResumeParams {
                thread_id: worker_b_thread.thread.id.clone(),
                history: None,
                path: None,
                model: None,
                model_provider: None,
                service_tier: None,
                cwd: None,
                approval_policy: None,
                approvals_reviewer: None,
                sandbox: None,
                config: None,
                base_instructions: None,
                developer_instructions: None,
                personality: None,
                ..Default::default()
            },
        }),
    )
    .await
    .expect("thread/resume fail-closed response should finish in time");
    let resume_error =
        resume_result.expect_err("thread/resume should fail closed without a replacement account");
    assert!(
            resume_error.to_string().contains(
                "thread 00000000-0000-0000-0000-0000000000b2 is pinned to worker 1 with exhausted account capacity for thread/resume, and no replacement worker restored the context"
            ),
            "{resume_error}"
        );

    let thread_handoff_event = timeout(Duration::from_secs(5), thread_handoff_event_task)
        .await
        .expect("timed out waiting for thread handoff failure event")
        .expect("thread handoff failure event task should finish");
    assert!(thread_handoff_event.contains("event: gateway/accountThreadHandoffFailed"));
    assert!(thread_handoff_event.contains("\"tenantId\":\"default\""));
    assert!(thread_handoff_event.contains("\"projectId\":null"));
    assert!(thread_handoff_event.contains("\"method\":\"thread/resume\""));
    assert!(thread_handoff_event.contains("\"threadId\":\"00000000-0000-0000-0000-0000000000b2\""));
    assert!(thread_handoff_event.contains("\"exhaustedWorkerId\":1"));
    assert!(thread_handoff_event.contains("\"exhaustedAccountId\":\"acct-b\""));
    assert!(thread_handoff_event.contains("no replacement worker restored the context"));

    let fork_handoff_event_url = format!("http://{}/v1/events", server.local_addr());
    let fork_handoff_event_task = tokio::spawn(async move {
        let mut events_response = reqwest::Client::new()
            .get(fork_handoff_event_url)
            .send()
            .await
            .expect("event stream response");
        assert_eq!(events_response.status(), reqwest::StatusCode::OK);

        let mut buffered = String::new();
        loop {
            let chunk = events_response
                .chunk()
                .await
                .expect("event stream chunk")
                .expect("event stream not closed");
            buffered.push_str(std::str::from_utf8(&chunk).expect("utf8"));

            while let Some(event_end) = buffered.find("\n\n") {
                let event = buffered[..event_end].to_string();
                buffered.drain(..event_end + 2);

                if event.contains("event: gateway/accountThreadHandoffFailed")
                    && event.contains("\"method\":\"thread/fork\"")
                {
                    return event;
                }
            }
        }
    });
    sleep(Duration::from_millis(100)).await;

    let fork_result: Result<ThreadForkResponse, _> = timeout(
        Duration::from_secs(5),
        client.request_typed(ClientRequest::ThreadFork {
            request_id: RequestId::Integer(5),
            params: ThreadForkParams {
                thread_id: worker_b_thread.thread.id.clone(),
                path: None,
                model: None,
                model_provider: None,
                service_tier: None,
                cwd: None,
                approval_policy: None,
                approvals_reviewer: None,
                sandbox: None,
                config: None,
                base_instructions: None,
                developer_instructions: None,
                ephemeral: true,
                ..Default::default()
            },
        }),
    )
    .await
    .expect("thread/fork fail-closed response should finish in time");
    let fork_error =
        fork_result.expect_err("thread/fork should fail closed without a replacement account");
    assert!(
            fork_error.to_string().contains(
                "thread 00000000-0000-0000-0000-0000000000b2 is pinned to worker 1 with exhausted account capacity for thread/fork, and no replacement worker restored the context"
            ),
            "{fork_error}"
        );

    let fork_handoff_event = timeout(Duration::from_secs(5), fork_handoff_event_task)
        .await
        .expect("timed out waiting for fork handoff failure event")
        .expect("fork handoff failure event task should finish");
    assert!(fork_handoff_event.contains("event: gateway/accountThreadHandoffFailed"));
    assert!(fork_handoff_event.contains("\"tenantId\":\"default\""));
    assert!(fork_handoff_event.contains("\"projectId\":null"));
    assert!(fork_handoff_event.contains("\"method\":\"thread/fork\""));
    assert!(fork_handoff_event.contains("\"threadId\":\"00000000-0000-0000-0000-0000000000b2\""));
    assert!(fork_handoff_event.contains("\"exhaustedWorkerId\":1"));
    assert!(fork_handoff_event.contains("\"exhaustedAccountId\":\"acct-b\""));
    assert!(fork_handoff_event.contains("no replacement worker restored the context"));

    let read_handoff_event_url = format!("http://{}/v1/events", server.local_addr());
    let read_handoff_event_task = tokio::spawn(async move {
        let mut events_response = reqwest::Client::new()
            .get(read_handoff_event_url)
            .send()
            .await
            .expect("event stream response");
        assert_eq!(events_response.status(), reqwest::StatusCode::OK);

        let mut buffered = String::new();
        loop {
            let chunk = events_response
                .chunk()
                .await
                .expect("event stream chunk")
                .expect("event stream not closed");
            buffered.push_str(std::str::from_utf8(&chunk).expect("utf8"));

            while let Some(event_end) = buffered.find("\n\n") {
                let event = buffered[..event_end].to_string();
                buffered.drain(..event_end + 2);

                if event.contains("event: gateway/accountThreadHandoffFailed")
                    && event.contains("\"method\":\"thread/read\"")
                {
                    return event;
                }
            }
        }
    });
    sleep(Duration::from_millis(100)).await;

    let read_result: Result<AppServerThreadReadResponse, _> = timeout(
        Duration::from_secs(5),
        client.request_typed(ClientRequest::ThreadRead {
            request_id: RequestId::Integer(6),
            params: ThreadReadParams {
                thread_id: worker_b_thread.thread.id.clone(),
                include_turns: false,
            },
        }),
    )
    .await
    .expect("thread/read fail-closed response should finish in time");
    let read_error =
        read_result.expect_err("thread/read should fail closed without a replacement account");
    assert!(
            read_error.to_string().contains(
                "thread 00000000-0000-0000-0000-0000000000b2 is pinned to worker 1 with exhausted account capacity for thread/read, and no replacement worker restored the context"
            ),
            "{read_error}"
        );

    let read_handoff_event = timeout(Duration::from_secs(5), read_handoff_event_task)
        .await
        .expect("timed out waiting for thread/read handoff failure event")
        .expect("thread/read handoff failure event task should finish");
    assert!(read_handoff_event.contains("event: gateway/accountThreadHandoffFailed"));
    assert!(read_handoff_event.contains("\"tenantId\":\"default\""));
    assert!(read_handoff_event.contains("\"projectId\":null"));
    assert!(read_handoff_event.contains("\"method\":\"thread/read\""));
    assert!(read_handoff_event.contains("\"threadId\":\"00000000-0000-0000-0000-0000000000b2\""));
    assert!(read_handoff_event.contains("\"exhaustedWorkerId\":1"));
    assert!(read_handoff_event.contains("\"exhaustedAccountId\":\"acct-b\""));
    assert!(read_handoff_event.contains("no replacement worker restored the context"));

    let rollback_handoff_event_url = format!("http://{}/v1/events", server.local_addr());
    let rollback_handoff_event_task = tokio::spawn(async move {
        let mut events_response = reqwest::Client::new()
            .get(rollback_handoff_event_url)
            .send()
            .await
            .expect("event stream response");
        assert_eq!(events_response.status(), reqwest::StatusCode::OK);

        let mut buffered = String::new();
        loop {
            let chunk = events_response
                .chunk()
                .await
                .expect("event stream chunk")
                .expect("event stream not closed");
            buffered.push_str(std::str::from_utf8(&chunk).expect("utf8"));

            while let Some(event_end) = buffered.find("\n\n") {
                let event = buffered[..event_end].to_string();
                buffered.drain(..event_end + 2);

                if event.contains("event: gateway/accountThreadHandoffFailed")
                    && event.contains("\"method\":\"thread/rollback\"")
                {
                    return event;
                }
            }
        }
    });
    sleep(Duration::from_millis(100)).await;

    let rollback_result: Result<ThreadRollbackResponse, _> = timeout(
        Duration::from_secs(5),
        client.request_typed(ClientRequest::ThreadRollback {
            request_id: RequestId::Integer(7),
            params: ThreadRollbackParams {
                thread_id: worker_b_thread.thread.id.clone(),
                num_turns: 1,
            },
        }),
    )
    .await
    .expect("thread/rollback fail-closed response should finish in time");
    let rollback_error = rollback_result
        .expect_err("thread/rollback should fail closed without a replacement account");
    assert!(
            rollback_error.to_string().contains(
                "thread 00000000-0000-0000-0000-0000000000b2 is pinned to worker 1 with exhausted account capacity for thread/rollback, and no replacement worker restored the context"
            ),
            "{rollback_error}"
        );

    let rollback_handoff_event = timeout(Duration::from_secs(5), rollback_handoff_event_task)
        .await
        .expect("timed out waiting for thread/rollback handoff failure event")
        .expect("thread/rollback handoff failure event task should finish");
    assert!(rollback_handoff_event.contains("event: gateway/accountThreadHandoffFailed"));
    assert!(rollback_handoff_event.contains("\"tenantId\":\"default\""));
    assert!(rollback_handoff_event.contains("\"projectId\":null"));
    assert!(rollback_handoff_event.contains("\"method\":\"thread/rollback\""));
    assert!(
        rollback_handoff_event.contains("\"threadId\":\"00000000-0000-0000-0000-0000000000b2\"")
    );
    assert!(rollback_handoff_event.contains("\"exhaustedWorkerId\":1"));
    assert!(rollback_handoff_event.contains("\"exhaustedAccountId\":\"acct-b\""));
    assert!(rollback_handoff_event.contains("no replacement worker restored the context"));

    let archive_handoff_event_url = format!("http://{}/v1/events", server.local_addr());
    let archive_handoff_event_task = tokio::spawn(async move {
        let mut events_response = reqwest::Client::new()
            .get(archive_handoff_event_url)
            .send()
            .await
            .expect("event stream response");
        assert_eq!(events_response.status(), reqwest::StatusCode::OK);

        let mut buffered = String::new();
        loop {
            let chunk = events_response
                .chunk()
                .await
                .expect("event stream chunk")
                .expect("event stream not closed");
            buffered.push_str(std::str::from_utf8(&chunk).expect("utf8"));

            while let Some(event_end) = buffered.find("\n\n") {
                let event = buffered[..event_end].to_string();
                buffered.drain(..event_end + 2);

                if event.contains("event: gateway/accountThreadHandoffFailed")
                    && event.contains("\"method\":\"thread/archive\"")
                {
                    return event;
                }
            }
        }
    });
    sleep(Duration::from_millis(100)).await;

    let archive_result: Result<ThreadArchiveResponse, _> = timeout(
        Duration::from_secs(5),
        client.request_typed(ClientRequest::ThreadArchive {
            request_id: RequestId::Integer(8),
            params: ThreadArchiveParams {
                thread_id: worker_b_thread.thread.id.clone(),
            },
        }),
    )
    .await
    .expect("thread/archive fail-closed response should finish in time");
    let archive_error = archive_result
        .expect_err("thread/archive should fail closed without a replacement account");
    assert!(
            archive_error.to_string().contains(
                "thread 00000000-0000-0000-0000-0000000000b2 is pinned to worker 1 with exhausted account capacity for thread/archive, and no replacement worker restored the context"
            ),
            "{archive_error}"
        );

    let archive_handoff_event = timeout(Duration::from_secs(5), archive_handoff_event_task)
        .await
        .expect("timed out waiting for thread/archive handoff failure event")
        .expect("thread/archive handoff event task should finish");
    assert!(archive_handoff_event.contains("event: gateway/accountThreadHandoffFailed"));
    assert!(archive_handoff_event.contains("\"tenantId\":\"default\""));
    assert!(archive_handoff_event.contains("\"projectId\":null"));
    assert!(archive_handoff_event.contains("\"method\":\"thread/archive\""));
    assert!(
        archive_handoff_event.contains("\"threadId\":\"00000000-0000-0000-0000-0000000000b2\"")
    );
    assert!(archive_handoff_event.contains("\"exhaustedWorkerId\":1"));
    assert!(archive_handoff_event.contains("\"exhaustedAccountId\":\"acct-b\""));
    assert!(archive_handoff_event.contains("no replacement worker restored the context"));

    let unarchive_handoff_event_url = format!("http://{}/v1/events", server.local_addr());
    let unarchive_handoff_event_task = tokio::spawn(async move {
        let mut events_response = reqwest::Client::new()
            .get(unarchive_handoff_event_url)
            .send()
            .await
            .expect("event stream response");
        assert_eq!(events_response.status(), reqwest::StatusCode::OK);

        let mut buffered = String::new();
        loop {
            let chunk = events_response
                .chunk()
                .await
                .expect("event stream chunk")
                .expect("event stream not closed");
            buffered.push_str(std::str::from_utf8(&chunk).expect("utf8"));

            while let Some(event_end) = buffered.find("\n\n") {
                let event = buffered[..event_end].to_string();
                buffered.drain(..event_end + 2);

                if event.contains("event: gateway/accountThreadHandoffFailed")
                    && event.contains("\"method\":\"thread/unarchive\"")
                {
                    return event;
                }
            }
        }
    });
    sleep(Duration::from_millis(100)).await;

    let unarchive_result: Result<ThreadUnarchiveResponse, _> = timeout(
        Duration::from_secs(5),
        client.request_typed(ClientRequest::ThreadUnarchive {
            request_id: RequestId::Integer(9),
            params: ThreadUnarchiveParams {
                thread_id: worker_b_thread.thread.id.clone(),
            },
        }),
    )
    .await
    .expect("thread/unarchive fail-closed response should finish in time");
    let unarchive_error = unarchive_result
        .expect_err("thread/unarchive should fail closed without a replacement account");
    assert!(
            unarchive_error.to_string().contains(
                "thread 00000000-0000-0000-0000-0000000000b2 is pinned to worker 1 with exhausted account capacity for thread/unarchive, and no replacement worker restored the context"
            ),
            "{unarchive_error}"
        );

    let unarchive_handoff_event = timeout(Duration::from_secs(5), unarchive_handoff_event_task)
        .await
        .expect("timed out waiting for thread/unarchive handoff failure event")
        .expect("thread/unarchive handoff event task should finish");
    assert!(unarchive_handoff_event.contains("event: gateway/accountThreadHandoffFailed"));
    assert!(unarchive_handoff_event.contains("\"tenantId\":\"default\""));
    assert!(unarchive_handoff_event.contains("\"projectId\":null"));
    assert!(unarchive_handoff_event.contains("\"method\":\"thread/unarchive\""));
    assert!(
        unarchive_handoff_event.contains("\"threadId\":\"00000000-0000-0000-0000-0000000000b2\"")
    );
    assert!(unarchive_handoff_event.contains("\"exhaustedWorkerId\":1"));
    assert!(unarchive_handoff_event.contains("\"exhaustedAccountId\":\"acct-b\""));
    assert!(unarchive_handoff_event.contains("no replacement worker restored the context"));

    assert_remote_client_shutdown(
        timeout(Duration::from_secs(5), client.shutdown())
            .await
            .expect("client shutdown should finish in time"),
    );
    timeout(Duration::from_secs(5), server.shutdown())
        .await
        .expect("server shutdown should finish in time")
        .expect("shutdown");
}
