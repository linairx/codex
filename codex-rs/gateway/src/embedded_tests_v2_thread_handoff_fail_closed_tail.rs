use super::*;
use pretty_assertions::assert_eq;

#[tokio::test]
async fn remote_multi_worker_fails_closed_when_thread_id_handoff_has_no_account_replacement_for_tail_methods()
 {
    let (server, client, worker_b_thread) = thread_handoff_fail_closed_test_setup!();

    let metadata_handoff_event_url = format!("http://{}/v1/events", server.local_addr());
    let metadata_handoff_event_task = tokio::spawn(async move {
        let mut events_response = reqwest::Client::new()
            .get(metadata_handoff_event_url)
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
                    && event.contains("\"method\":\"thread/metadata/update\"")
                {
                    return event;
                }
            }
        }
    });
    sleep(Duration::from_millis(100)).await;

    let metadata_result: Result<ThreadMetadataUpdateResponse, _> = timeout(
        Duration::from_secs(5),
        client.request_typed(ClientRequest::ThreadMetadataUpdate {
            request_id: RequestId::Integer(10),
            params: ThreadMetadataUpdateParams {
                thread_id: worker_b_thread.thread.id.clone(),
                git_info: Some(ThreadMetadataGitInfoUpdateParams {
                    sha: Some(Some("abc123".to_string())),
                    branch: Some(Some("main".to_string())),
                    origin_url: None,
                }),
            },
        }),
    )
    .await
    .expect("thread/metadata/update fail-closed response should finish in time");
    let metadata_error = metadata_result
        .expect_err("thread/metadata/update should fail closed without a replacement account");
    assert!(
            metadata_error.to_string().contains(
                "thread 00000000-0000-0000-0000-0000000000b2 is pinned to worker 1 with exhausted account capacity for thread/metadata/update, and no replacement worker restored the context"
            ),
            "{metadata_error}"
        );

    let metadata_handoff_event = timeout(Duration::from_secs(5), metadata_handoff_event_task)
        .await
        .expect("timed out waiting for thread/metadata/update handoff failure event")
        .expect("thread/metadata/update handoff event task should finish");
    assert!(metadata_handoff_event.contains("event: gateway/accountThreadHandoffFailed"));
    assert!(metadata_handoff_event.contains("\"tenantId\":\"default\""));
    assert!(metadata_handoff_event.contains("\"projectId\":null"));
    assert!(metadata_handoff_event.contains("\"method\":\"thread/metadata/update\""));
    assert!(
        metadata_handoff_event.contains("\"threadId\":\"00000000-0000-0000-0000-0000000000b2\"")
    );
    assert!(metadata_handoff_event.contains("\"exhaustedWorkerId\":1"));
    assert!(metadata_handoff_event.contains("\"exhaustedAccountId\":\"acct-b\""));
    assert!(metadata_handoff_event.contains("no replacement worker restored the context"));

    let turns_list_handoff_event_url = format!("http://{}/v1/events", server.local_addr());
    let turns_list_handoff_event_task = tokio::spawn(async move {
        let mut events_response = reqwest::Client::new()
            .get(turns_list_handoff_event_url)
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
                    && event.contains("\"method\":\"thread/turns/list\"")
                {
                    return event;
                }
            }
        }
    });
    sleep(Duration::from_millis(100)).await;

    let turns_list_result: Result<ThreadTurnsListResponse, _> = timeout(
        Duration::from_secs(5),
        client.request_typed(ClientRequest::ThreadTurnsList {
            request_id: RequestId::Integer(11),
            params: ThreadTurnsListParams {
                thread_id: worker_b_thread.thread.id.clone(),
                cursor: None,
                limit: Some(10),
                sort_direction: None,
                items_view: None,
            },
        }),
    )
    .await
    .expect("thread/turns/list fail-closed response should finish in time");
    let turns_list_error = turns_list_result
        .expect_err("thread/turns/list should fail closed without a replacement account");
    assert!(
            turns_list_error.to_string().contains(
                "thread 00000000-0000-0000-0000-0000000000b2 is pinned to worker 1 with exhausted account capacity for thread/turns/list, and no replacement worker restored the context"
            ),
            "{turns_list_error}"
        );

    let turns_list_handoff_event = timeout(Duration::from_secs(5), turns_list_handoff_event_task)
        .await
        .expect("timed out waiting for thread/turns/list handoff failure event")
        .expect("thread/turns/list handoff event task should finish");
    assert!(turns_list_handoff_event.contains("event: gateway/accountThreadHandoffFailed"));
    assert!(turns_list_handoff_event.contains("\"tenantId\":\"default\""));
    assert!(turns_list_handoff_event.contains("\"projectId\":null"));
    assert!(turns_list_handoff_event.contains("\"method\":\"thread/turns/list\""));
    assert!(
        turns_list_handoff_event.contains("\"threadId\":\"00000000-0000-0000-0000-0000000000b2\"")
    );
    assert!(turns_list_handoff_event.contains("\"exhaustedWorkerId\":1"));
    assert!(turns_list_handoff_event.contains("\"exhaustedAccountId\":\"acct-b\""));
    assert!(turns_list_handoff_event.contains("no replacement worker restored the context"));

    let increment_handoff_event_url = format!("http://{}/v1/events", server.local_addr());
    let increment_handoff_event_task = tokio::spawn(async move {
        let mut events_response = reqwest::Client::new()
            .get(increment_handoff_event_url)
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
                    && event.contains("\"method\":\"thread/increment_elicitation\"")
                {
                    return event;
                }
            }
        }
    });
    sleep(Duration::from_millis(100)).await;

    let increment_result: Result<ThreadIncrementElicitationResponse, _> = timeout(
        Duration::from_secs(5),
        client.request_typed(ClientRequest::ThreadIncrementElicitation {
            request_id: RequestId::Integer(12),
            params: ThreadIncrementElicitationParams {
                thread_id: worker_b_thread.thread.id.clone(),
            },
        }),
    )
    .await
    .expect("thread/increment_elicitation fail-closed response should finish in time");
    let increment_error = increment_result.expect_err(
        "thread/increment_elicitation should fail closed without a replacement account",
    );
    assert!(
            increment_error.to_string().contains(
                "thread 00000000-0000-0000-0000-0000000000b2 is pinned to worker 1 with exhausted account capacity for thread/increment_elicitation, and no replacement worker restored the context"
            ),
            "{increment_error}"
        );

    let increment_handoff_event = timeout(Duration::from_secs(5), increment_handoff_event_task)
        .await
        .expect("timed out waiting for thread/increment_elicitation handoff failure event")
        .expect("thread/increment_elicitation handoff event task should finish");
    assert!(increment_handoff_event.contains("event: gateway/accountThreadHandoffFailed"));
    assert!(increment_handoff_event.contains("\"tenantId\":\"default\""));
    assert!(increment_handoff_event.contains("\"projectId\":null"));
    assert!(increment_handoff_event.contains("\"method\":\"thread/increment_elicitation\""));
    assert!(
        increment_handoff_event.contains("\"threadId\":\"00000000-0000-0000-0000-0000000000b2\"")
    );
    assert!(increment_handoff_event.contains("\"exhaustedWorkerId\":1"));
    assert!(increment_handoff_event.contains("\"exhaustedAccountId\":\"acct-b\""));
    assert!(increment_handoff_event.contains("no replacement worker restored the context"));

    let decrement_handoff_event_url = format!("http://{}/v1/events", server.local_addr());
    let decrement_handoff_event_task = tokio::spawn(async move {
        let mut events_response = reqwest::Client::new()
            .get(decrement_handoff_event_url)
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
                    && event.contains("\"method\":\"thread/decrement_elicitation\"")
                {
                    return event;
                }
            }
        }
    });
    sleep(Duration::from_millis(100)).await;

    let decrement_result: Result<ThreadDecrementElicitationResponse, _> = timeout(
        Duration::from_secs(5),
        client.request_typed(ClientRequest::ThreadDecrementElicitation {
            request_id: RequestId::Integer(13),
            params: ThreadDecrementElicitationParams {
                thread_id: worker_b_thread.thread.id.clone(),
            },
        }),
    )
    .await
    .expect("thread/decrement_elicitation fail-closed response should finish in time");
    let decrement_error = decrement_result.expect_err(
        "thread/decrement_elicitation should fail closed without a replacement account",
    );
    assert!(
            decrement_error.to_string().contains(
                "thread 00000000-0000-0000-0000-0000000000b2 is pinned to worker 1 with exhausted account capacity for thread/decrement_elicitation, and no replacement worker restored the context"
            ),
            "{decrement_error}"
        );

    let decrement_handoff_event = timeout(Duration::from_secs(5), decrement_handoff_event_task)
        .await
        .expect("timed out waiting for thread/decrement_elicitation handoff failure event")
        .expect("thread/decrement_elicitation handoff event task should finish");
    assert!(decrement_handoff_event.contains("event: gateway/accountThreadHandoffFailed"));
    assert!(decrement_handoff_event.contains("\"tenantId\":\"default\""));
    assert!(decrement_handoff_event.contains("\"projectId\":null"));
    assert!(decrement_handoff_event.contains("\"method\":\"thread/decrement_elicitation\""));
    assert!(
        decrement_handoff_event.contains("\"threadId\":\"00000000-0000-0000-0000-0000000000b2\"")
    );
    assert!(decrement_handoff_event.contains("\"exhaustedWorkerId\":1"));
    assert!(decrement_handoff_event.contains("\"exhaustedAccountId\":\"acct-b\""));
    assert!(decrement_handoff_event.contains("no replacement worker restored the context"));

    let inject_items_handoff_event_url = format!("http://{}/v1/events", server.local_addr());
    let inject_items_handoff_event_task = tokio::spawn(async move {
        let mut events_response = reqwest::Client::new()
            .get(inject_items_handoff_event_url)
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
                    && event.contains("\"method\":\"thread/inject_items\"")
                {
                    return event;
                }
            }
        }
    });
    sleep(Duration::from_millis(100)).await;

    let inject_items_result: Result<ThreadInjectItemsResponse, _> = timeout(
        Duration::from_secs(5),
        client.request_typed(ClientRequest::ThreadInjectItems {
            request_id: RequestId::Integer(14),
            params: ThreadInjectItemsParams {
                thread_id: worker_b_thread.thread.id.clone(),
                items: vec![serde_json::json!({
                    "type": "message",
                    "role": "assistant",
                    "content": [{
                        "type": "output_text",
                        "text": "Injected reply",
                        "annotations": [],
                    }],
                })],
            },
        }),
    )
    .await
    .expect("thread/inject_items fail-closed response should finish in time");
    let inject_items_error = inject_items_result
        .expect_err("thread/inject_items should fail closed without a replacement account");
    assert!(
            inject_items_error.to_string().contains(
                "thread 00000000-0000-0000-0000-0000000000b2 is pinned to worker 1 with exhausted account capacity for thread/inject_items, and no replacement worker restored the context"
            ),
            "{inject_items_error}"
        );

    let inject_items_handoff_event =
        timeout(Duration::from_secs(5), inject_items_handoff_event_task)
            .await
            .expect("timed out waiting for thread/inject_items handoff failure event")
            .expect("thread/inject_items handoff event task should finish");
    assert!(inject_items_handoff_event.contains("event: gateway/accountThreadHandoffFailed"));
    assert!(inject_items_handoff_event.contains("\"tenantId\":\"default\""));
    assert!(inject_items_handoff_event.contains("\"projectId\":null"));
    assert!(inject_items_handoff_event.contains("\"method\":\"thread/inject_items\""));
    assert!(
        inject_items_handoff_event
            .contains("\"threadId\":\"00000000-0000-0000-0000-0000000000b2\"")
    );
    assert!(inject_items_handoff_event.contains("\"exhaustedWorkerId\":1"));
    assert!(inject_items_handoff_event.contains("\"exhaustedAccountId\":\"acct-b\""));
    assert!(inject_items_handoff_event.contains("no replacement worker restored the context"));

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
