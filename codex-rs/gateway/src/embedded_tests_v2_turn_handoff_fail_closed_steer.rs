use super::*;
use pretty_assertions::assert_eq;

#[tokio::test]
async fn remote_multi_worker_fails_closed_when_turn_steer_has_no_account_replacement_over_v2() {
    let (server, client, worker_b_thread, turn_id) = turn_handoff_fail_closed_test_setup!();

    let thread_id = worker_b_thread.thread.id.clone();
    let handoff_event_url = format!("http://{}/v1/events", server.local_addr());
    let handoff_event_task = tokio::spawn({
        async move {
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

                    if event.contains("event: gateway/accountActiveThreadHandoffFailed")
                        && event.contains("\"method\":\"turn/steer\"")
                    {
                        return event;
                    }
                }
            }
        }
    });
    sleep(Duration::from_millis(100)).await;

    let result: Result<TurnSteerResponse, _> = timeout(
        Duration::from_secs(5),
        client.request_typed(ClientRequest::TurnSteer {
            request_id: RequestId::Integer(5),
            params: TurnSteerParams {
                thread_id: thread_id.clone(),
                input: vec![UserInput::Text {
                    text: "keep steering".to_string(),
                    text_elements: Vec::new(),
                }],
                responsesapi_client_metadata: None,
                expected_turn_id: turn_id.clone(),
                ..Default::default()
            },
        }),
    )
    .await
    .expect("turn/steer fail-closed response should finish in time");
    let request_error =
        result.expect_err("turn/steer should fail closed without a replacement account");

    assert!(
        request_error
            .to_string()
            .contains(
                format!(
                    "thread {thread_id} is pinned to worker 1 with exhausted account capacity for turn/steer"
                )
                .as_str()
            ),
        "{request_error}"
    );

    let handoff_event = timeout(Duration::from_secs(5), handoff_event_task)
        .await
        .expect("timed out waiting for turn control handoff failure event")
        .expect("handoff event task should finish");
    assert!(handoff_event.contains("event: gateway/accountActiveThreadHandoffFailed"));
    assert!(handoff_event.contains("\"tenantId\":\"default\""));
    assert!(handoff_event.contains("\"projectId\":null"));
    assert!(handoff_event.contains("\"method\":\"turn/steer\""));
    assert!(handoff_event.contains(&format!("\"threadId\":\"{thread_id}\"")));
    assert!(handoff_event.contains("\"exhaustedWorkerId\":1"));
    assert!(handoff_event.contains("\"exhaustedAccountId\":\"acct-b\""));

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
