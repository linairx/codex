use super::*;
use pretty_assertions::assert_eq;

#[tokio::test]
async fn remote_multi_worker_restores_path_resume_after_account_exhaustion_over_v2() {
    let (server, client, worker_b_thread, worker_b_rollout_path) = path_restore_test_setup!();

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

                if event.contains("event: gateway/accountPathHandoffSucceeded")
                    && event.contains("\"method\":\"thread/resume\"")
                {
                    return event;
                }
            }
        }
    });
    sleep(Duration::from_millis(100)).await;

    let resumed: ThreadResumeResponse = timeout(
        Duration::from_secs(5),
        client.request_typed(ClientRequest::ThreadResume {
            request_id: RequestId::Integer(4),
            params: ThreadResumeParams {
                thread_id: worker_b_thread.thread.id.clone(),
                history: None,
                path: Some(worker_b_rollout_path.clone()),
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
    .expect("path-based thread/resume should finish in time")
    .expect("path-based thread/resume should restore through the replacement worker");
    assert_eq!(resumed.thread.id, worker_b_thread.thread.id);
    assert_eq!(resumed.thread.path, Some(worker_b_rollout_path.clone()));
    assert_eq!(resumed.cwd.as_ref().to_string_lossy(), "/tmp/worker-a");

    let handoff_event = timeout(Duration::from_secs(5), handoff_event_task)
        .await
        .expect("timed out waiting for path thread/resume handoff event")
        .expect("handoff event task should finish");
    assert!(handoff_event.contains("event: gateway/accountPathHandoffSucceeded"));
    assert!(handoff_event.contains("\"tenantId\":\"default\""));
    assert!(handoff_event.contains("\"projectId\":null"));
    assert!(handoff_event.contains("\"method\":\"thread/resume\""));
    assert!(handoff_event.contains("\"threadPath\":\"/tmp/worker-b/rollout.jsonl\""));
    assert!(handoff_event.contains("\"exhaustedWorkerId\":1"));
    assert!(handoff_event.contains("\"exhaustedAccountId\":\"acct-b\""));
    assert!(handoff_event.contains("\"replacementWorkerId\":0"));
    assert!(handoff_event.contains("\"replacementAccountId\":\"acct-a\""));

    let restored_read: AppServerThreadReadResponse = timeout(
        Duration::from_secs(5),
        client.request_typed(ClientRequest::ThreadRead {
            request_id: RequestId::Integer(5),
            params: ThreadReadParams {
                thread_id: worker_b_thread.thread.id.clone(),
                include_turns: false,
            },
        }),
    )
    .await
    .expect("thread/read should finish in time")
    .expect("thread/read after path resume should stay on the replacement worker");
    assert_eq!(restored_read.thread.id, worker_b_thread.thread.id);
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
