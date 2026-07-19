use super::*;
use pretty_assertions::assert_eq;

#[tokio::test]
async fn embedded_server_supports_drop_in_v2_client_realtime_request_workflow() {
    let body = mock_responses_sse_body("Done");
    let listener = TcpListener::bind("127.0.0.1:0")
        .await
        .expect("listener should bind");
    let addr = listener.local_addr().expect("listener addr");
    let requests = Arc::new(Mutex::new(Vec::new()));
    let (shutdown_tx, shutdown_rx) = oneshot::channel::<()>();
    let recorded_requests = requests.clone();
    let model_server_task = tokio::spawn(async move {
        let app = axum::Router::new()
            .route(
                "/v1/responses",
                axum::routing::post(move |headers: HeaderMap| {
                    let body = body.clone();
                    let requests = recorded_requests.clone();
                    async move {
                        requests.lock().await.push(MockResponsesRequest { headers });
                        (
                            [
                                (axum::http::header::CONTENT_TYPE, "text/event-stream"),
                                (axum::http::header::CACHE_CONTROL, "no-cache"),
                            ],
                            body,
                        )
                    }
                }),
            )
            .route(
                "/v1/realtime/calls",
                axum::routing::post(|| async move {
                    (
                        StatusCode::OK,
                        [(
                            axum::http::header::LOCATION,
                            "/v1/realtime/calls/rtc_app_test",
                        )],
                        "v=answer\r\n",
                    )
                }),
            );
        let _ = axum::serve(listener, app)
            .with_graceful_shutdown(async move {
                let _ = shutdown_rx.await;
            })
            .await;
    });
    let model_server = MockResponsesServer {
        uri: format!("http://{addr}"),
        requests,
        shutdown: Some(shutdown_tx),
        task: model_server_task,
    };
    let realtime_server = start_websocket_server(vec![vec![
        vec![serde_json::json!({
            "type": "session.updated",
            "session": { "id": "sess-embedded", "instructions": "backend prompt" }
        })],
        vec![],
        vec![
            serde_json::json!({
                "type": "response.output_audio.delta",
                "delta": "AQID",
                "sample_rate": 24_000,
                "channels": 1,
                "samples_per_channel": 512
            }),
            serde_json::json!({
                "type": "conversation.item.added",
                "item": {
                    "type": "message",
                    "role": "assistant",
                    "content": [{ "type": "text", "text": "hi" }]
                }
            }),
            serde_json::json!({
                "type": "conversation.item.input_audio_transcription.delta",
                "delta": "delegate now"
            }),
            serde_json::json!({
                "type": "response.output_text.delta",
                "delta": "working"
            }),
            serde_json::json!({
                "type": "response.output_audio_transcript.done",
                "transcript": "working on it"
            }),
            serde_json::json!({
                "type": "error",
                "message": "upstream boom"
            }),
        ],
        vec![],
    ]])
    .await;

    let codex_home = tempdir().expect("tempdir");
    std::fs::create_dir_all(codex_home.path().join(".git")).expect(".git should be created");
    std::fs::write(
        codex_home.path().join("config.toml"),
        format!(
            r#"
model = "mock-model"
approval_policy = "never"
sandbox_mode = "read-only"
cli_auth_credentials_store = "file"
model_provider = "mock_provider"
experimental_realtime_ws_base_url = "{}"
experimental_realtime_ws_backend_prompt = "backend prompt"

[realtime]
version = "v2"
type = "conversational"

[features]
realtime_conversation = true

[model_providers.mock_provider]
name = "Mock provider for test"
base_url = "{}/v1"
wire_api = "responses"
request_max_retries = 0
stream_max_retries = 0
"#,
            realtime_server.uri(),
            model_server.uri
        ),
    )
    .expect("config.toml should be written");
    std::fs::write(
        codex_home.path().join("auth.json"),
        serde_json::to_string_pretty(&serde_json::json!({
            "OPENAI_API_KEY": "sk-test-key",
            "tokens": serde_json::Value::Null,
            "last_refresh": serde_json::Value::Null,
        }))
        .expect("auth.json should serialize"),
    )
    .expect("auth.json should be written");
    let config = Config::load_default_with_cli_overrides_for_codex_home(
        codex_home.path().to_path_buf(),
        Vec::new(),
    )
    .await
    .expect("config");
    let server = start_embedded_gateway_server(
        GatewayConfig {
            bind_address: "127.0.0.1:0".parse().expect("bind address"),
            enable_codex_api_key_env: false,
            session_source: SessionSource::Cli,
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
    .expect("remote client should connect to embedded gateway");

    let voices: ThreadRealtimeListVoicesResponse = client
        .request_typed(ClientRequest::ThreadRealtimeListVoices {
            request_id: RequestId::Integer(1),
            params: ThreadRealtimeListVoicesParams {},
        })
        .await
        .expect("thread/realtime/listVoices should succeed through embedded gateway");
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
                cwd: Some(codex_home.path().display().to_string()),
                ephemeral: Some(true),
                ..Default::default()
            },
        })
        .await
        .expect("thread/start should succeed through embedded gateway");

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
                codex_response_handoff_mode: None,
                include_startup_context: None,
            },
        })
        .await
        .expect("thread/realtime/start should succeed through embedded gateway");
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

    let session_update = timeout(
        Duration::from_secs(5),
        realtime_server.wait_for_request(0, 0),
    )
    .await
    .expect("session.update should reach realtime sideband");
    assert_eq!(
        session_update.body_json()["type"],
        serde_json::json!("session.update")
    );

    let append_audio: ThreadRealtimeAppendAudioResponse = client
        .request_typed(ClientRequest::ThreadRealtimeAppendAudio {
            request_id: RequestId::Integer(4),
            params: ThreadRealtimeAppendAudioParams {
                thread_id: started.thread.id.clone(),
                audio: ThreadRealtimeAudioChunk {
                    data: "BQYH".to_string(),
                    sample_rate: 24_000,
                    num_channels: 1,
                    samples_per_channel: Some(480),
                    item_id: None,
                },
            },
        })
        .await
        .expect("thread/realtime/appendAudio should succeed through embedded gateway");
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
        .expect("thread/realtime/appendText should succeed through embedded gateway");
    assert_eq!(append_text, ThreadRealtimeAppendTextResponse {});

    let stop_response: ThreadRealtimeStopResponse = client
        .request_typed(ClientRequest::ThreadRealtimeStop {
            request_id: RequestId::Integer(6),
            params: ThreadRealtimeStopParams {
                thread_id: started.thread.id.clone(),
            },
        })
        .await
        .expect("thread/realtime/stop should succeed through embedded gateway");
    assert_eq!(stop_response, ThreadRealtimeStopResponse {});

    let first_append_request = timeout(
        Duration::from_secs(5),
        realtime_server.wait_for_request(0, 1),
    )
    .await
    .expect("first realtime append request should reach sideband");
    let second_append_request = timeout(
        Duration::from_secs(5),
        realtime_server.wait_for_request(0, 2),
    )
    .await
    .expect("second realtime append request should reach sideband");
    let mut append_requests = [
        first_append_request.body_json(),
        second_append_request.body_json(),
    ];
    append_requests.sort_by(|left, right| left["type"].as_str().cmp(&right["type"].as_str()));
    assert_eq!(
        append_requests[0]["type"],
        serde_json::json!("conversation.item.create")
    );
    assert_eq!(
        append_requests[0]["item"]["content"][0]["text"],
        serde_json::json!("hello realtime")
    );
    assert_eq!(
        append_requests[1]["type"],
        serde_json::json!("input_audio_buffer.append")
    );

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
        "embedded realtime notifications should arrive: {coverage:?}"
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
    realtime_server.shutdown().await;
    model_server.shutdown().await;
}
