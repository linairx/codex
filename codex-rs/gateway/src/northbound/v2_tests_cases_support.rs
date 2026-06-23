use super::*;
use pretty_assertions::assert_eq;
use std::future::Future;

pub(crate) async fn send_initialize(
    websocket: &mut tokio_tungstenite::WebSocketStream<
        tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>,
    >,
) {
    send_initialize_with_capabilities(websocket, None).await;
}

pub(crate) async fn send_initialized(
    websocket: &mut tokio_tungstenite::WebSocketStream<
        tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>,
    >,
) {
    websocket
        .send(Message::Text(
            serde_json::to_string(&JSONRPCMessage::Notification(JSONRPCNotification {
                method: "initialized".to_string(),
                params: None,
            }))
            .expect("initialized notification should serialize")
            .into(),
        ))
        .await
        .expect("initialized notification should send");
}

pub(crate) async fn send_jsonrpc_request(
    websocket: &mut tokio_tungstenite::WebSocketStream<
        tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>,
    >,
    id: RequestId,
    method: &str,
    params: serde_json::Value,
) {
    websocket
        .send(Message::Text(
            serde_json::to_string(&JSONRPCMessage::Request(JSONRPCRequest {
                id,
                method: method.to_string(),
                params: Some(params),
                trace: None,
            }))
            .expect("json-rpc request should serialize")
            .into(),
        ))
        .await
        .expect("json-rpc request should send");
}

pub(crate) async fn send_initialize_with_capabilities(
    websocket: &mut tokio_tungstenite::WebSocketStream<
        tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>,
    >,
    capabilities: Option<InitializeCapabilities>,
) {
    websocket
        .send(Message::Text(
            serde_json::to_string(&JSONRPCMessage::Request(JSONRPCRequest {
                id: RequestId::String("initialize".to_string()),
                method: "initialize".to_string(),
                params: Some(
                    serde_json::to_value(InitializeParams {
                        client_info: ClientInfo {
                            name: "codex-tui".to_string(),
                            title: None,
                            version: "0.0.0-test".to_string(),
                        },
                        capabilities,
                    })
                    .expect("initialize params should serialize"),
                ),
                trace: None,
            }))
            .expect("request should serialize")
            .into(),
        ))
        .await
        .expect("initialize request should send");

    let JSONRPCMessage::Response(response) = read_websocket_message(websocket).await else {
        panic!("expected initialize response");
    };
    assert_eq!(response.id, RequestId::String("initialize".to_string()));
}

pub(crate) async fn spawn_test_server(
    state: GatewayV2State,
) -> (std::net::SocketAddr, tokio::task::JoinHandle<()>) {
    let listener = TcpListener::bind("127.0.0.1:0")
        .await
        .expect("listener should bind");
    let addr = listener.local_addr().expect("listener address");
    let app = Router::new().route(
        "/",
        any(websocket_upgrade_handler).with_state(state.clone()),
    );
    let server_task = tokio::spawn(async move {
        axum::serve(listener, app).await.expect("server should run");
    });
    (addr, server_task)
}

pub(crate) async fn read_websocket_message<S>(
    websocket: &mut tokio_tungstenite::WebSocketStream<S>,
) -> JSONRPCMessage
where
    S: AsyncRead + AsyncWrite + Unpin,
{
    loop {
        let frame = websocket
            .next()
            .await
            .expect("frame should be available")
            .expect("frame should decode");
        match frame {
            Message::Text(text) => {
                return serde_json::from_str::<JSONRPCMessage>(&text)
                    .expect("text frame should decode");
            }
            Message::Binary(_) | Message::Ping(_) | Message::Pong(_) | Message::Frame(_) => {
                continue;
            }
            Message::Close(_) => panic!("unexpected close frame"),
        }
    }
}

pub(crate) async fn read_websocket_request<S>(
    websocket: &mut tokio_tungstenite::WebSocketStream<S>,
) -> JSONRPCRequest
where
    S: AsyncRead + AsyncWrite + Unpin,
{
    loop {
        let frame = websocket
            .next()
            .await
            .expect("frame should be available")
            .expect("frame should decode");
        match frame {
            Message::Text(text) => {
                match serde_json::from_str::<JSONRPCMessage>(&text)
                    .expect("text frame should decode")
                {
                    JSONRPCMessage::Request(request) => return request,
                    JSONRPCMessage::Notification(notification)
                        if notification.method == "initialized" =>
                    {
                        continue;
                    }
                    other => panic!("expected request, got {other:?}"),
                }
            }
            Message::Binary(_) | Message::Ping(_) | Message::Pong(_) | Message::Frame(_) => {
                continue;
            }
            Message::Close(_) => panic!("unexpected close frame"),
        }
    }
}

pub(crate) async fn wait_for_close_frame(
    websocket: &mut tokio_tungstenite::WebSocketStream<
        tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>,
    >,
) -> Message {
    timeout(Duration::from_secs(2), async {
        loop {
            let frame = websocket
                .next()
                .await
                .expect("websocket should yield frame")
                .expect("frame should decode");
            if matches!(frame, Message::Close(_)) {
                break frame;
            }
        }
    })
    .await
    .expect("close frame should arrive")
}

pub(crate) async fn test_initialize_response() -> InitializeResponse {
    let codex_home = tempdir().expect("tempdir");
    let config = Config::load_default_with_cli_overrides_for_codex_home(
        codex_home.path().to_path_buf(),
        Vec::new(),
    )
    .await
    .expect("config should load");
    gateway_initialize_response(&config)
}

pub(crate) async fn start_test_request_handle() -> (InProcessAppServerClient, AppServerRequestHandle)
{
    let codex_home = tempdir().expect("tempdir");
    let config = Arc::new(
        Config::load_default_with_cli_overrides_for_codex_home(
            codex_home.path().to_path_buf(),
            Vec::new(),
        )
        .await
        .expect("config should load"),
    );
    let client = InProcessAppServerClient::start(InProcessClientStartArgs {
        arg0_paths: Arg0DispatchPaths::default(),
        config,
        cli_overrides: Vec::new(),
        loader_overrides: LoaderOverrides::default(),
        strict_config: false,
        cloud_config_bundle: CloudConfigBundleLoader::default(),
        feedback: CodexFeedback::new(),
        log_db: None,
        state_db: None,
        environment_manager: Arc::new(EnvironmentManager::default_for_tests()),
        config_warnings: Vec::new(),
        session_source: SessionSource::Cli,
        enable_codex_api_key_env: false,
        client_name: "codex-gateway-test".to_string(),
        client_version: "0.0.0-test".to_string(),
        experimental_api: true,
        mcp_server_openai_form_elicitation: false,
        opt_out_notification_methods: Vec::new(),
        channel_capacity: 4,
    })
    .await
    .expect("in-process client should start");
    let request_handle = AppServerRequestHandle::InProcess(client.request_handle());
    (client, request_handle)
}

#[derive(Clone, Default)]
struct SharedWriter {
    buffer: Arc<StdMutex<Vec<u8>>>,
}

struct SharedWriterGuard {
    buffer: Arc<StdMutex<Vec<u8>>>,
}

impl<'a> tracing_subscriber::fmt::MakeWriter<'a> for SharedWriter {
    type Writer = SharedWriterGuard;

    fn make_writer(&'a self) -> Self::Writer {
        SharedWriterGuard {
            buffer: Arc::clone(&self.buffer),
        }
    }
}

impl Write for SharedWriterGuard {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        self.buffer
            .lock()
            .expect("log buffer should lock")
            .extend_from_slice(buf);
        Ok(buf.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

pub(crate) fn capture_logs(f: impl FnOnce()) -> String {
    let _guard = SYNC_LOG_CAPTURE_LOCK
        .lock()
        .expect("sync log capture lock should lock");
    let writer = SharedWriter::default();
    let subscriber = tracing_subscriber::registry().with(
        tracing_subscriber::fmt::layer()
            .with_ansi(false)
            .without_time()
            .with_writer(writer.clone()),
    );
    tracing::subscriber::with_default(subscriber, f);

    let bytes = writer
        .buffer
        .lock()
        .expect("log buffer should lock")
        .clone();
    String::from_utf8(bytes).expect("log output should be utf8")
}

pub(crate) async fn capture_logs_async(f: impl Future<Output = ()>) -> String {
    let _permit = ASYNC_LOG_CAPTURE_LOCK
        .acquire()
        .await
        .expect("async log capture lock should acquire");
    let writer = SharedWriter::default();
    let subscriber = tracing_subscriber::registry().with(
        tracing_subscriber::fmt::layer()
            .with_ansi(false)
            .without_time()
            .with_writer(writer.clone()),
    );
    let dispatch = tracing::Dispatch::new(subscriber);
    let _default_guard = tracing::dispatcher::set_default(&dispatch);
    f.await;

    let bytes = writer
        .buffer
        .lock()
        .expect("log buffer should lock")
        .clone();
    String::from_utf8(bytes).expect("log output should be utf8")
}

pub(crate) async fn start_mock_remote_server_for_initialize() -> String {
    let listener = TcpListener::bind("127.0.0.1:0")
        .await
        .expect("listener should bind");
    let addr = listener.local_addr().expect("listener address");
    tokio::spawn(async move {
        let (stream, _) = listener.accept().await.expect("accept should succeed");
        let mut websocket = tokio_tungstenite::accept_async(stream)
            .await
            .expect("websocket should accept");

        let frame = websocket
            .next()
            .await
            .expect("initialize frame should exist")
            .expect("initialize frame should decode");
        let Message::Text(text) = frame else {
            panic!("expected initialize text frame");
        };
        let JSONRPCMessage::Request(initialize_request) =
            serde_json::from_str(&text).expect("initialize should decode")
        else {
            panic!("expected initialize request");
        };
        assert_eq!(initialize_request.method, "initialize");
        websocket
            .send(Message::Text(
                serde_json::to_string(&JSONRPCMessage::Response(JSONRPCResponse {
                    id: initialize_request.id,
                    result: serde_json::json!({}),
                }))
                .expect("initialize response should serialize")
                .into(),
            ))
            .await
            .expect("initialize response should send");

        let frame = websocket
            .next()
            .await
            .expect("initialized frame should exist")
            .expect("initialized frame should decode");
        let Message::Text(text) = frame else {
            panic!("expected initialized text frame");
        };
        let JSONRPCMessage::Notification(notification) =
            serde_json::from_str(&text).expect("initialized should decode")
        else {
            panic!("expected initialized notification");
        };
        assert_eq!(notification.method, "initialized");

        while let Some(frame) = websocket.next().await {
            match frame.expect("follow-up frame should decode") {
                Message::Close(_) => break,
                Message::Ping(payload) => {
                    websocket
                        .send(Message::Pong(payload))
                        .await
                        .expect("pong should send");
                }
                Message::Text(_) | Message::Binary(_) | Message::Pong(_) | Message::Frame(_) => {}
            }
        }
    });
    format!("ws://{addr}")
}

pub(crate) async fn start_mock_remote_server_recording_initialize(
    recorded_initialize_params: Arc<Mutex<Vec<InitializeParams>>>,
) -> String {
    let listener = TcpListener::bind("127.0.0.1:0")
        .await
        .expect("listener should bind");
    let addr = listener.local_addr().expect("listener address");
    tokio::spawn(async move {
        let (stream, _) = listener.accept().await.expect("accept should succeed");
        let mut websocket = tokio_tungstenite::accept_async(stream)
            .await
            .expect("websocket should accept");

        let frame = websocket
            .next()
            .await
            .expect("initialize frame should exist")
            .expect("initialize frame should decode");
        let Message::Text(text) = frame else {
            panic!("expected initialize text frame");
        };
        let JSONRPCMessage::Request(initialize_request) =
            serde_json::from_str(&text).expect("initialize should decode")
        else {
            panic!("expected initialize request");
        };
        assert_eq!(initialize_request.method, "initialize");
        let initialize_params: InitializeParams = serde_json::from_value(
            initialize_request
                .params
                .clone()
                .expect("initialize params should exist"),
        )
        .expect("initialize params should decode");
        recorded_initialize_params
            .lock()
            .await
            .push(initialize_params);
        websocket
            .send(Message::Text(
                serde_json::to_string(&JSONRPCMessage::Response(JSONRPCResponse {
                    id: initialize_request.id,
                    result: serde_json::json!({}),
                }))
                .expect("initialize response should serialize")
                .into(),
            ))
            .await
            .expect("initialize response should send");

        let frame = websocket
            .next()
            .await
            .expect("initialized frame should exist")
            .expect("initialized frame should decode");
        let Message::Text(text) = frame else {
            panic!("expected initialized text frame");
        };
        let JSONRPCMessage::Notification(notification) =
            serde_json::from_str(&text).expect("initialized should decode")
        else {
            panic!("expected initialized notification");
        };
        assert_eq!(notification.method, "initialized");

        while let Some(frame) = websocket.next().await {
            match frame.expect("follow-up frame should decode") {
                Message::Close(_) => break,
                Message::Ping(payload) => {
                    websocket
                        .send(Message::Pong(payload))
                        .await
                        .expect("pong should send");
                }
                Message::Text(_) | Message::Binary(_) | Message::Pong(_) | Message::Frame(_) => {}
            }
        }
    });
    format!("ws://{addr}")
}
