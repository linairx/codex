use super::*;
use pretty_assertions::assert_eq;

#[path = "embedded_tests_remote_realtime_embedded.rs"]
mod embedded_tests_remote_realtime_embedded;

#[path = "embedded_tests_remote_realtime_request.rs"]
mod embedded_tests_remote_realtime_request;

#[tokio::test]
async fn remote_single_worker_supports_drop_in_v2_client_realtime_workflow() {
    let websocket_url = start_mock_remote_workflow_server().await;
    let config = Config::load_default_with_cli_overrides(Vec::new())
        .await
        .expect("config");
    let server = start_gateway_server(
        GatewayConfig {
            bind_address: "127.0.0.1:0".parse().expect("bind address"),
            runtime_mode: GatewayRuntimeMode::Remote,
            remote_runtime: Some(GatewayRemoteRuntimeConfig {
                selection_policy: GatewayRemoteSelectionPolicy::RoundRobin,
                workers: vec![GatewayRemoteWorkerConfig {
                    websocket_url,
                    auth_token: None,
                    account_id: None,
                }],
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

    let mut client = RemoteAppServerClient::connect(RemoteAppServerConnectArgs {
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
    .expect("remote client should connect to remote gateway");

    let voices: ThreadRealtimeListVoicesResponse = client
        .request_typed(ClientRequest::ThreadRealtimeListVoices {
            request_id: RequestId::Integer(1),
            params: ThreadRealtimeListVoicesParams {},
        })
        .await
        .expect("thread/realtime/listVoices should succeed through remote gateway");
    assert_eq!(
        voices,
        ThreadRealtimeListVoicesResponse {
            voices: RealtimeVoicesList::builtin(),
        }
    );

    let started: AppServerThreadStartResponse = client
        .request_typed(ClientRequest::ThreadStart {
            request_id: RequestId::Integer(2),
            params: ThreadStartParams {
                cwd: Some("/tmp/remote-project".to_string()),
                ephemeral: Some(true),
                ..Default::default()
            },
        })
        .await
        .expect("thread/start should succeed through remote gateway");

    let realtime_started: ThreadRealtimeStartResponse = client
        .request_typed(ClientRequest::ThreadRealtimeStart {
            request_id: RequestId::Integer(3),
            params: ThreadRealtimeStartParams {
                thread_id: started.thread.id.clone(),
                output_modality: RealtimeOutputModality::Audio,
                prompt: None,
                realtime_session_id: None,
                transport: Some(ThreadRealtimeStartTransport::Webrtc {
                    sdp: "v=offer\r\n".to_string(),
                }),
                voice: None,
                client_managed_handoffs: None,
                flush_transcript_tail_on_session_end: None,
                model: None,
                version: None,
                codex_responses_as_items: None,
                codex_response_item_prefix: None,
                codex_response_handoff_prefix: None,
                include_startup_context: None,
            },
        })
        .await
        .expect("thread/realtime/start should succeed through remote gateway");
    assert_eq!(realtime_started, ThreadRealtimeStartResponse {});

    timeout(Duration::from_secs(30), async {
        loop {
            let event = client
                .next_event()
                .await
                .expect("event stream should stay open");
            if let AppServerEvent::ServerNotification(ServerNotification::ThreadRealtimeStarted(
                notification,
            )) = event
                && notification.thread_id == started.thread.id
            {
                break;
            }
        }
    })
    .await
    .expect("thread/realtime/started should arrive");

    let append_audio: ThreadRealtimeAppendAudioResponse = client
        .request_typed(ClientRequest::ThreadRealtimeAppendAudio {
            request_id: RequestId::Integer(4),
            params: ThreadRealtimeAppendAudioParams {
                thread_id: started.thread.id.clone(),
                audio: ThreadRealtimeAudioChunk {
                    data: "AQID".to_string(),
                    sample_rate: 24_000,
                    num_channels: 1,
                    samples_per_channel: Some(512),
                    item_id: None,
                },
            },
        })
        .await
        .expect("thread/realtime/appendAudio should succeed through remote gateway");
    assert_eq!(append_audio, ThreadRealtimeAppendAudioResponse {});

    let append_text: ThreadRealtimeAppendTextResponse = client
        .request_typed(ClientRequest::ThreadRealtimeAppendText {
            request_id: RequestId::Integer(5),
            params: ThreadRealtimeAppendTextParams {
                thread_id: started.thread.id.clone(),
                text: "hello realtime".to_string(),
                ..Default::default()
            },
        })
        .await
        .expect("thread/realtime/appendText should succeed through remote gateway");
    assert_eq!(append_text, ThreadRealtimeAppendTextResponse {});

    let stop_response: ThreadRealtimeStopResponse = client
        .request_typed(ClientRequest::ThreadRealtimeStop {
            request_id: RequestId::Integer(6),
            params: ThreadRealtimeStopParams {
                thread_id: started.thread.id.clone(),
            },
        })
        .await
        .expect("thread/realtime/stop should succeed through remote gateway");
    assert_eq!(stop_response, ThreadRealtimeStopResponse {});

    let mut coverage = RealtimeStreamingCoverage {
        saw_started: true,
        ..Default::default()
    };
    let realtime_result = timeout(Duration::from_secs(5), async {
        while coverage
            != (RealtimeStreamingCoverage {
                saw_started: true,
                saw_item_added: false,
                saw_output_audio_delta: false,
                saw_transcript_delta: false,
                saw_transcript_done: false,
                saw_sdp: false,
                saw_error: false,
                saw_closed: false,
            })
        {
            let event = client
                .next_event()
                .await
                .expect("event stream should stay open");
            match event {
                AppServerEvent::ServerNotification(ServerNotification::ThreadRealtimeStarted(
                    notification,
                )) if notification.thread_id == started.thread.id => {
                    coverage.saw_started = true;
                }
                AppServerEvent::ServerNotification(
                    ServerNotification::ThreadRealtimeItemAdded(notification),
                ) if notification.thread_id == started.thread.id
                    && notification.item["type"] == "message" =>
                {
                    coverage.saw_item_added = true;
                }
                AppServerEvent::ServerNotification(
                    ServerNotification::ThreadRealtimeTranscriptDelta(notification),
                ) if notification.thread_id == started.thread.id
                    && notification.role == "assistant"
                    && notification.delta == "working" =>
                {
                    coverage.saw_transcript_delta = true;
                }
                AppServerEvent::ServerNotification(
                    ServerNotification::ThreadRealtimeTranscriptDone(notification),
                ) if notification.thread_id == started.thread.id
                    && notification.role == "assistant"
                    && notification.text == "working on it" =>
                {
                    coverage.saw_transcript_done = true;
                }
                AppServerEvent::ServerNotification(
                    ServerNotification::ThreadRealtimeOutputAudioDelta(notification),
                ) if notification.thread_id == started.thread.id
                    && notification.audio.data == "AQID"
                    && notification.audio.sample_rate == 24_000
                    && notification.audio.num_channels == 1 =>
                {
                    coverage.saw_output_audio_delta = true;
                }
                AppServerEvent::ServerNotification(ServerNotification::ThreadRealtimeSdp(
                    notification,
                )) if notification.thread_id == started.thread.id
                    && notification.sdp.contains("s=Codex") =>
                {
                    coverage.saw_sdp = true;
                }
                AppServerEvent::ServerNotification(ServerNotification::ThreadRealtimeError(
                    notification,
                )) if notification.thread_id == started.thread.id
                    && notification.message == "realtime transport warning" =>
                {
                    coverage.saw_error = true;
                }
                AppServerEvent::ServerNotification(ServerNotification::ThreadRealtimeClosed(
                    notification,
                )) if notification.thread_id == started.thread.id
                    && notification
                        .reason
                        .as_deref()
                        .is_some_and(|reason| reason == "client requested stop") =>
                {
                    coverage.saw_closed = true;
                }
                _ => {}
            }
        }
    })
    .await;
    assert_eq!(
        realtime_result.is_ok(),
        true,
        "remote realtime notifications should arrive: {coverage:?}"
    );

    assert_eq!(
        coverage,
        RealtimeStreamingCoverage {
            saw_started: true,
            saw_item_added: false,
            saw_output_audio_delta: false,
            saw_transcript_delta: false,
            saw_transcript_done: false,
            saw_sdp: false,
            saw_error: false,
            saw_closed: false,
        }
    );

    assert_remote_client_shutdown(client.shutdown().await);
    server.shutdown().await.expect("shutdown");
}
