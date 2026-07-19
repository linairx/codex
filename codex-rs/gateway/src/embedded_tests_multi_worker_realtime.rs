use super::*;
use pretty_assertions::assert_eq;

#[tokio::test]
async fn remote_multi_worker_supports_v2_realtime_workflow_after_worker_reconnect() {
    let worker_a = start_reconnecting_v2_multi_connection_realtime_server_with_voices(
        "thread-worker-a-2",
        "/tmp/worker-a-2",
        "session-worker-a-2",
        "delta from recovered worker a",
        "done from recovered worker a",
        RealtimeVoicesList {
            v1: vec![RealtimeVoice::Juniper, RealtimeVoice::Maple],
            v2: vec![RealtimeVoice::Alloy],
            default_v1: RealtimeVoice::Juniper,
            default_v2: RealtimeVoice::Alloy,
        },
    )
    .await;
    let worker_b = start_mock_remote_multi_connection_realtime_server_with_voices(
        "thread-worker-b",
        "/tmp/worker-b",
        "session-worker-b",
        "delta from worker b",
        "done from worker b",
        RealtimeVoicesList {
            v1: vec![RealtimeVoice::Maple, RealtimeVoice::Cove],
            v2: vec![RealtimeVoice::Alloy, RealtimeVoice::Marin],
            default_v1: RealtimeVoice::Cove,
            default_v2: RealtimeVoice::Marin,
        },
    )
    .await;
    let config = Config::load_default_with_cli_overrides(Vec::new())
        .await
        .expect("config");
    let server = start_gateway_server(
        GatewayConfig {
            bind_address: "127.0.0.1:0".parse().expect("bind address"),
            runtime_mode: GatewayRuntimeMode::Remote,
            remote_runtime: Some(GatewayRemoteRuntimeConfig {
                selection_policy: GatewayRemoteSelectionPolicy::RoundRobin,
                workers: vec![
                    GatewayRemoteWorkerConfig {
                        websocket_url: worker_a,

                        auth_token: None,
                        account_id: None,
                    },
                    GatewayRemoteWorkerConfig {
                        websocket_url: worker_b,

                        auth_token: None,
                        account_id: None,
                    },
                ],
            }),
            ..GatewayConfig::default()
        },
        Arg0DispatchPaths::default(),
        config,
        Vec::new(),
        LoaderOverrides::default(),
    )
    .await
    .expect("server");

    let client = reqwest::Client::new();
    timeout(Duration::from_secs(5), async {
        loop {
            let healthz_response = client
                .get(format!("http://{}/healthz", server.local_addr()))
                .send()
                .await
                .expect("healthz response");
            let health: GatewayHealthResponse = healthz_response.json().await.expect("health body");
            let Some(remote_workers) = health.remote_workers.as_ref() else {
                panic!("remote workers should exist");
            };
            if health.status == GatewayHealthStatus::Ok
                && remote_workers[0].healthy
                && !remote_workers[0].reconnecting
                && remote_workers[0].last_error.is_some()
                && remote_workers[1].healthy
                && !remote_workers[1].reconnecting
            {
                break;
            }
            sleep(Duration::from_millis(100)).await;
        }
    })
    .await
    .expect("worker should reconnect before v2 client connects");

    let mut v2_client = RemoteAppServerClient::connect(RemoteAppServerConnectArgs {
        endpoint: codex_app_server_client::RemoteAppServerEndpoint::WebSocket {
            websocket_url: format!("ws://{}/", server.local_addr()),
            auth_token: None,
        },
        client_name: "codex-gateway-test".to_string(),
        client_version: "0.0.0-test".to_string(),
        experimental_api: true,
        mcp_server_openai_form_elicitation: false,
        opt_out_notification_methods: Vec::new(),
        channel_capacity: 64,
    })
    .await
    .expect("v2 client should connect after worker reconnect");

    let voices: ThreadRealtimeListVoicesResponse = v2_client
        .request_typed(ClientRequest::ThreadRealtimeListVoices {
            request_id: RequestId::Integer(0),
            params: ThreadRealtimeListVoicesParams {},
        })
        .await
        .expect("thread/realtime/listVoices should succeed after worker reconnect");
    assert_eq!(
        voices,
        ThreadRealtimeListVoicesResponse {
            voices: RealtimeVoicesList {
                v1: vec![
                    RealtimeVoice::Juniper,
                    RealtimeVoice::Maple,
                    RealtimeVoice::Cove,
                ],
                v2: vec![RealtimeVoice::Alloy, RealtimeVoice::Marin],
                default_v1: RealtimeVoice::Juniper,
                default_v2: RealtimeVoice::Alloy,
            },
        }
    );

    let first_started: AppServerThreadStartResponse = v2_client
        .request_typed(ClientRequest::ThreadStart {
            request_id: RequestId::Integer(1),
            params: ThreadStartParams {
                model: None,
                model_provider: None,
                service_tier: None,
                cwd: Some("/tmp/project-a".to_string()),
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
        })
        .await
        .expect("first thread/start should succeed after worker reconnect");
    assert_eq!(first_started.thread.id, "thread-worker-a-2");

    let second_started: AppServerThreadStartResponse = v2_client
        .request_typed(ClientRequest::ThreadStart {
            request_id: RequestId::Integer(2),
            params: ThreadStartParams {
                model: None,
                model_provider: None,
                service_tier: None,
                cwd: Some("/tmp/project-b".to_string()),
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
        })
        .await
        .expect("second thread/start should succeed after worker reconnect");
    assert_eq!(second_started.thread.id, "thread-worker-b");

    let first_realtime_started: ThreadRealtimeStartResponse = v2_client
        .request_typed(ClientRequest::ThreadRealtimeStart {
            request_id: RequestId::Integer(3),
            params: ThreadRealtimeStartParams {
                thread_id: first_started.thread.id.clone(),
                output_modality: RealtimeOutputModality::Text,
                prompt: None,
                realtime_session_id: None,
                transport: None,
                voice: None,
                client_managed_handoffs: None,
                flush_transcript_tail_on_session_end: None,
                model: None,
                version: None,
                codex_responses_as_items: None,
                codex_response_item_prefix: None,
                codex_response_handoff_mode: None,
                include_startup_context: None,
            },
        })
        .await
        .expect("first realtime start should succeed after worker reconnect");
    assert_eq!(first_realtime_started, ThreadRealtimeStartResponse {});

    let second_realtime_started: ThreadRealtimeStartResponse = v2_client
        .request_typed(ClientRequest::ThreadRealtimeStart {
            request_id: RequestId::Integer(4),
            params: ThreadRealtimeStartParams {
                thread_id: second_started.thread.id.clone(),
                output_modality: RealtimeOutputModality::Text,
                prompt: None,
                realtime_session_id: None,
                transport: None,
                voice: None,
                client_managed_handoffs: None,
                flush_transcript_tail_on_session_end: None,
                model: None,
                version: None,
                codex_responses_as_items: None,
                codex_response_item_prefix: None,
                codex_response_handoff_mode: None,
                include_startup_context: None,
            },
        })
        .await
        .expect("second realtime start should succeed after worker reconnect");
    assert_eq!(second_realtime_started, ThreadRealtimeStartResponse {});

    let first_append_text: ThreadRealtimeAppendTextResponse = v2_client
        .request_typed(ClientRequest::ThreadRealtimeAppendText {
            request_id: RequestId::Integer(5),
            params: ThreadRealtimeAppendTextParams {
                thread_id: first_started.thread.id.clone(),
                text: "hello recovered realtime".to_string(),
                ..Default::default()
            },
        })
        .await
        .expect("first realtime appendText should succeed after worker reconnect");
    assert_eq!(first_append_text, ThreadRealtimeAppendTextResponse {});

    let second_append_text: ThreadRealtimeAppendTextResponse = v2_client
        .request_typed(ClientRequest::ThreadRealtimeAppendText {
            request_id: RequestId::Integer(6),
            params: ThreadRealtimeAppendTextParams {
                thread_id: second_started.thread.id.clone(),
                text: "hello worker b realtime".to_string(),
                ..Default::default()
            },
        })
        .await
        .expect("second realtime appendText should succeed after worker reconnect");
    assert_eq!(second_append_text, ThreadRealtimeAppendTextResponse {});

    let first_append_audio: ThreadRealtimeAppendAudioResponse = v2_client
        .request_typed(ClientRequest::ThreadRealtimeAppendAudio {
            request_id: RequestId::Integer(7),
            params: ThreadRealtimeAppendAudioParams {
                thread_id: first_started.thread.id.clone(),
                audio: ThreadRealtimeAudioChunk {
                    data: "AQID".to_string(),
                    sample_rate: 24_000,
                    num_channels: 1,
                    samples_per_channel: Some(3),
                    item_id: Some("item-audio-a".to_string()),
                },
            },
        })
        .await
        .expect("first realtime appendAudio should succeed after worker reconnect");
    assert_eq!(first_append_audio, ThreadRealtimeAppendAudioResponse {});

    let second_append_audio: ThreadRealtimeAppendAudioResponse = v2_client
        .request_typed(ClientRequest::ThreadRealtimeAppendAudio {
            request_id: RequestId::Integer(8),
            params: ThreadRealtimeAppendAudioParams {
                thread_id: second_started.thread.id.clone(),
                audio: ThreadRealtimeAudioChunk {
                    data: "BAUG".to_string(),
                    sample_rate: 24_000,
                    num_channels: 1,
                    samples_per_channel: Some(3),
                    item_id: Some("item-audio-b".to_string()),
                },
            },
        })
        .await
        .expect("second realtime appendAudio should succeed after worker reconnect");
    assert_eq!(second_append_audio, ThreadRealtimeAppendAudioResponse {});

    let first_stop: ThreadRealtimeStopResponse = v2_client
        .request_typed(ClientRequest::ThreadRealtimeStop {
            request_id: RequestId::Integer(9),
            params: ThreadRealtimeStopParams {
                thread_id: first_started.thread.id.clone(),
            },
        })
        .await
        .expect("first realtime stop should succeed after worker reconnect");
    assert_eq!(first_stop, ThreadRealtimeStopResponse {});

    let second_stop: ThreadRealtimeStopResponse = v2_client
        .request_typed(ClientRequest::ThreadRealtimeStop {
            request_id: RequestId::Integer(10),
            params: ThreadRealtimeStopParams {
                thread_id: second_started.thread.id.clone(),
            },
        })
        .await
        .expect("second realtime stop should succeed after worker reconnect");
    assert_eq!(second_stop, ThreadRealtimeStopResponse {});

    let mut coverage_by_thread = HashMap::from([
        (
            first_started.thread.id.clone(),
            RealtimeStreamingCoverage::default(),
        ),
        (
            second_started.thread.id.clone(),
            RealtimeStreamingCoverage::default(),
        ),
    ]);
    let expected_by_thread = HashMap::from([
        (
            first_started.thread.id.clone(),
            (
                "session-worker-a-2",
                "delta from recovered worker a",
                "done from recovered worker a",
                "AQID",
            ),
        ),
        (
            second_started.thread.id.clone(),
            (
                "session-worker-b",
                "delta from worker b",
                "done from worker b",
                "BAUG",
            ),
        ),
    ]);

    timeout(Duration::from_secs(5), async {
        while coverage_by_thread.values().any(|coverage| {
            *coverage
                != RealtimeStreamingCoverage {
                    saw_started: true,
                    saw_item_added: true,
                    saw_output_audio_delta: true,
                    saw_transcript_delta: true,
                    saw_transcript_done: true,
                    saw_sdp: true,
                    saw_error: true,
                    saw_closed: true,
                }
        }) {
            let event = v2_client
                .next_event()
                .await
                .expect("event stream should stay open after reconnect");
            match event {
                AppServerEvent::ServerNotification(ServerNotification::ThreadRealtimeStarted(
                    notification,
                )) => {
                    if let Some((expected_session_id, _, _, _)) =
                        expected_by_thread.get(&notification.thread_id)
                        && let Some(coverage) = coverage_by_thread.get_mut(&notification.thread_id)
                        && notification.realtime_session_id.as_deref() == Some(*expected_session_id)
                        && notification.version == RealtimeConversationVersion::V2
                    {
                        coverage.saw_started = true;
                    }
                }
                AppServerEvent::ServerNotification(
                    ServerNotification::ThreadRealtimeItemAdded(notification),
                ) => {
                    if let Some(coverage) = coverage_by_thread.get_mut(&notification.thread_id)
                        && notification.item["type"] == "message"
                    {
                        coverage.saw_item_added = true;
                    }
                }
                AppServerEvent::ServerNotification(
                    ServerNotification::ThreadRealtimeTranscriptDelta(notification),
                ) => {
                    if let Some((_, expected_delta, _, _)) =
                        expected_by_thread.get(&notification.thread_id)
                        && let Some(coverage) = coverage_by_thread.get_mut(&notification.thread_id)
                        && notification.role == "assistant"
                        && notification.delta == *expected_delta
                    {
                        coverage.saw_transcript_delta = true;
                    }
                }
                AppServerEvent::ServerNotification(
                    ServerNotification::ThreadRealtimeTranscriptDone(notification),
                ) => {
                    if let Some((_, _, expected_text, _)) =
                        expected_by_thread.get(&notification.thread_id)
                        && let Some(coverage) = coverage_by_thread.get_mut(&notification.thread_id)
                        && notification.role == "assistant"
                        && notification.text == *expected_text
                    {
                        coverage.saw_transcript_done = true;
                    }
                }
                AppServerEvent::ServerNotification(
                    ServerNotification::ThreadRealtimeOutputAudioDelta(notification),
                ) => {
                    if let Some((_, _, _, expected_audio)) =
                        expected_by_thread.get(&notification.thread_id)
                        && let Some(coverage) = coverage_by_thread.get_mut(&notification.thread_id)
                        && notification.audio.data == *expected_audio
                        && notification.audio.sample_rate == 24_000
                        && notification.audio.num_channels == 1
                    {
                        coverage.saw_output_audio_delta = true;
                    }
                }
                AppServerEvent::ServerNotification(ServerNotification::ThreadRealtimeSdp(
                    notification,
                )) => {
                    if let Some(coverage) = coverage_by_thread.get_mut(&notification.thread_id)
                        && notification.sdp.contains("s=Codex")
                    {
                        coverage.saw_sdp = true;
                    }
                }
                AppServerEvent::ServerNotification(ServerNotification::ThreadRealtimeError(
                    notification,
                )) => {
                    if let Some(coverage) = coverage_by_thread.get_mut(&notification.thread_id)
                        && notification.message == "realtime transport warning"
                    {
                        coverage.saw_error = true;
                    }
                }
                AppServerEvent::ServerNotification(ServerNotification::ThreadRealtimeClosed(
                    notification,
                )) => {
                    if let Some(coverage) = coverage_by_thread.get_mut(&notification.thread_id)
                        && notification.reason.as_deref() == Some("client requested stop")
                    {
                        coverage.saw_closed = true;
                    }
                }
                _ => {}
            }
        }
    })
    .await
    .expect("realtime notifications should fan in after reconnect");

    assert_eq!(
        coverage_by_thread.get(&first_started.thread.id),
        Some(&RealtimeStreamingCoverage {
            saw_started: true,
            saw_item_added: true,
            saw_output_audio_delta: true,
            saw_transcript_delta: true,
            saw_transcript_done: true,
            saw_sdp: true,
            saw_error: true,
            saw_closed: true,
        })
    );
    assert_eq!(
        coverage_by_thread.get(&second_started.thread.id),
        Some(&RealtimeStreamingCoverage {
            saw_started: true,
            saw_item_added: true,
            saw_output_audio_delta: true,
            saw_transcript_delta: true,
            saw_transcript_done: true,
            saw_sdp: true,
            saw_error: true,
            saw_closed: true,
        })
    );

    assert_remote_client_shutdown(v2_client.shutdown().await);
    server.shutdown().await.expect("shutdown");
}
