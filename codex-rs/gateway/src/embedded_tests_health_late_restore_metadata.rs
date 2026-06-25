use super::*;
use pretty_assertions::assert_eq;

#[tokio::test]
async fn remote_multi_worker_restores_thread_metadata_update_after_account_exhaustion_over_v2() {
    let (_worker_a, _worker_b, server, client) = legacy_account_handoff_test_setup!();
    let _worker_b_thread: AppServerThreadStartResponse = timeout(
        Duration::from_secs(5),
        client.request_typed(ClientRequest::ThreadStart {
            request_id: RequestId::Integer(1),
            params: ThreadStartParams {
                model: None,
                model_provider: None,
                service_tier: None,
                cwd: Some("/tmp/worker-b".to_string()),
                approval_policy: None,
                approvals_reviewer: None,
                sandbox: None,
                config: None,
                service_name: None,
                base_instructions: None,
                developer_instructions: None,
                personality: None,
                ephemeral: Some(true),
                session_start_source: None,
                dynamic_tools: None,
                mock_experimental_field: None,
                experimental_raw_events: false,
                ..Default::default()
            },
        }),
    )
    .await
    .expect("thread/start should finish in time")
    .expect("thread/start should register worker A scope");
    let worker_a_thread: AppServerThreadStartResponse = timeout(
        Duration::from_secs(5),
        client.request_typed(ClientRequest::ThreadStart {
            request_id: RequestId::Integer(2),
            params: ThreadStartParams {
                model: None,
                model_provider: None,
                service_tier: None,
                cwd: Some("/tmp/worker-a".to_string()),
                approval_policy: None,
                approvals_reviewer: None,
                sandbox: None,
                config: None,
                service_name: None,
                base_instructions: None,
                developer_instructions: None,
                personality: None,
                ephemeral: Some(true),
                session_start_source: None,
                dynamic_tools: None,
                mock_experimental_field: None,
                experimental_raw_events: false,
                ..Default::default()
            },
        }),
    )
    .await
    .expect("thread/start should finish in time")
    .expect("thread/start should register worker B scope");
    let worker_b_thread_id = worker_a_thread.thread.id.clone();

    timeout(
        Duration::from_secs(5),
        client.request_typed::<GetAccountRateLimitsResponse>(ClientRequest::GetAccountRateLimits {
            request_id: RequestId::Integer(3),
            params: None,
        }),
    )
    .await
    .expect("account/rateLimits/read should finish in time")
    .expect("account/rateLimits/read should mark worker B exhausted");

    let handoff_event_url = format!("http://{}/v1/events", server.local_addr());
    let handoff_event_task = tokio::spawn(async move {
        let mut events_response = reqwest::Client::new()
            .get(handoff_event_url)
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

                if event.contains("event: gateway/accountThreadHandoffSucceeded")
                    && event.contains("\"method\":\"thread/metadata/update\"")
                {
                    return event;
                }
            }
        }
    });
    sleep(Duration::from_millis(100)).await;

    let metadata_update_response: ThreadMetadataUpdateResponse = timeout(
        Duration::from_secs(5),
        client.request_typed(ClientRequest::ThreadMetadataUpdate {
            request_id: RequestId::Integer(4),
            params: ThreadMetadataUpdateParams {
                thread_id: worker_b_thread_id.clone(),
                git_info: Some(ThreadMetadataGitInfoUpdateParams {
                    sha: Some(Some("abc123".to_string())),
                    branch: Some(Some("main".to_string())),
                    origin_url: None,
                }),
            },
        }),
    )
    .await
    .expect("thread/metadata/update should finish in time")
    .expect("thread/metadata/update should restore through the replacement worker");
    assert_eq!(metadata_update_response.thread.id, worker_b_thread_id);
    assert_eq!(
        metadata_update_response
            .thread
            .cwd
            .as_ref()
            .to_string_lossy(),
        "/tmp/worker-a-metadata"
    );

    let handoff_event = timeout(Duration::from_secs(5), handoff_event_task)
        .await
        .expect("timed out waiting for thread/metadata/update handoff event")
        .expect("handoff event task should finish");
    assert!(handoff_event.contains("event: gateway/accountThreadHandoffSucceeded"));
    assert!(handoff_event.contains("\"tenantId\":\"default\""));
    assert!(handoff_event.contains("\"projectId\":null"));
    assert!(handoff_event.contains("\"method\":\"thread/metadata/update\""));
    assert!(handoff_event.contains("\"threadId\":\"00000000-0000-0000-0000-0000000000b2\""));
    assert!(handoff_event.contains("\"exhaustedWorkerId\":1"));
    assert!(handoff_event.contains("\"exhaustedAccountId\":\"acct-b\""));
    assert!(handoff_event.contains("\"replacementWorkerId\":0"));
    assert!(handoff_event.contains("\"replacementAccountId\":\"acct-a\""));

    let restored_read: AppServerThreadReadResponse = timeout(
        Duration::from_secs(5),
        client.request_typed(ClientRequest::ThreadRead {
            request_id: RequestId::Integer(5),
            params: ThreadReadParams {
                thread_id: worker_b_thread_id.clone(),
                include_turns: false,
            },
        }),
    )
    .await
    .expect("thread/read should finish in time")
    .expect("thread/read after metadata update should stay on the replacement worker");
    assert_eq!(restored_read.thread.id, worker_b_thread_id);
    assert_eq!(
        restored_read.thread.cwd.as_ref().to_string_lossy(),
        "/tmp/worker-a-read"
    );

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
