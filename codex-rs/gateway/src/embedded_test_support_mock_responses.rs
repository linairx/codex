use super::*;

pub(crate) struct MockResponsesServer {
    pub(crate) uri: String,
    pub(crate) requests: Arc<Mutex<Vec<MockResponsesRequest>>>,
    pub(crate) shutdown: Option<oneshot::Sender<()>>,
    pub(crate) task: tokio::task::JoinHandle<()>,
}

#[derive(Clone)]
pub(crate) struct MockResponsesRequest {
    pub(crate) headers: HeaderMap,
}

pub(crate) struct MockResponsesHttpResponse {
    status: StatusCode,
    content_type: &'static str,
    body: String,
}

impl MockResponsesServer {
    pub(crate) async fn wait_for_request(&self, index: usize) -> MockResponsesRequest {
        loop {
            if let Some(request) = self.requests.lock().await.get(index).cloned() {
                return request;
            }
            sleep(Duration::from_millis(10)).await;
        }
    }

    pub(crate) async fn shutdown(mut self) {
        if let Some(shutdown) = self.shutdown.take() {
            let _ = shutdown.send(());
        }
        let _ = self.task.await;
    }
}

impl MockResponsesRequest {
    pub(crate) fn header(&self, name: &str) -> Option<String> {
        self.headers
            .get(name)
            .and_then(|value| value.to_str().ok())
            .map(str::to_string)
    }
}

pub(crate) async fn start_mock_responses_server_repeating_assistant(
    message: &str,
) -> MockResponsesServer {
    let body = mock_responses_sse_body(message);
    let listener = TcpListener::bind("127.0.0.1:0")
        .await
        .expect("listener should bind");
    let addr = listener.local_addr().expect("listener addr");
    let requests = Arc::new(Mutex::new(Vec::new()));
    let (shutdown_tx, shutdown_rx) = oneshot::channel::<()>();
    let recorded_requests = requests.clone();
    let task = tokio::spawn(async move {
        let app = axum::Router::new().route(
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
        );
        let _ = axum::serve(listener, app)
            .with_graceful_shutdown(async move {
                let _ = shutdown_rx.await;
            })
            .await;
    });

    MockResponsesServer {
        uri: format!("http://{addr}"),
        requests,
        shutdown: Some(shutdown_tx),
        task,
    }
}

pub(crate) async fn start_mock_responses_server_sequence(
    bodies: Vec<String>,
) -> MockResponsesServer {
    let responses = bodies
        .into_iter()
        .map(|body| MockResponsesHttpResponse {
            status: StatusCode::OK,
            content_type: "text/event-stream",
            body,
        })
        .collect();
    start_mock_responses_server_http_sequence(responses).await
}

pub(crate) async fn start_mock_responses_server_http_sequence(
    responses: Vec<MockResponsesHttpResponse>,
) -> MockResponsesServer {
    let listener = TcpListener::bind("127.0.0.1:0")
        .await
        .expect("listener should bind");
    let addr = listener.local_addr().expect("listener addr");
    let responses = Arc::new(Mutex::new(VecDeque::from(responses)));
    let requests = Arc::new(Mutex::new(Vec::new()));
    let (shutdown_tx, shutdown_rx) = oneshot::channel::<()>();
    let recorded_requests = requests.clone();
    let task = tokio::spawn(async move {
        let app = axum::Router::new().route(
            "/v1/responses",
            axum::routing::post(move |headers: HeaderMap| {
                let responses = responses.clone();
                let requests = recorded_requests.clone();
                async move {
                    requests.lock().await.push(MockResponsesRequest { headers });
                    let response = responses
                        .lock()
                        .await
                        .pop_front()
                        .expect("mock responses sequence should have another response");
                    (
                        response.status,
                        [
                            (axum::http::header::CONTENT_TYPE, response.content_type),
                            (axum::http::header::CACHE_CONTROL, "no-cache"),
                        ],
                        response.body,
                    )
                }
            }),
        );
        let _ = axum::serve(listener, app)
            .with_graceful_shutdown(async move {
                let _ = shutdown_rx.await;
            })
            .await;
    });

    MockResponsesServer {
        uri: format!("http://{addr}"),
        requests,
        shutdown: Some(shutdown_tx),
        task,
    }
}

pub(crate) fn mock_responses_sse_body(message: &str) -> String {
    format!(
        "event: response.created\n\
data: {{\"type\":\"response.created\",\"response\":{{\"id\":\"resp-1\"}}}}\n\n\
event: response.output_item.added\n\
data: {{\"type\":\"response.output_item.added\",\"item\":{{\"type\":\"message\",\"role\":\"assistant\",\"id\":\"msg-1\",\"content\":[{{\"type\":\"output_text\",\"text\":\"\"}}]}}}}\n\n\
event: response.output_text.delta\n\
data: {{\"type\":\"response.output_text.delta\",\"delta\":{message:?}}}\n\n\
event: response.output_item.done\n\
data: {{\"type\":\"response.output_item.done\",\"item\":{{\"type\":\"message\",\"role\":\"assistant\",\"id\":\"msg-1\",\"content\":[{{\"type\":\"output_text\",\"text\":{message:?}}}]}}}}\n\n\
event: response.completed\n\
data: {{\"type\":\"response.completed\",\"response\":{{\"id\":\"resp-1\",\"usage\":{{\"input_tokens\":0,\"input_tokens_details\":null,\"output_tokens\":0,\"output_tokens_details\":null,\"total_tokens\":0}}}}}}\n\n"
    )
}

pub(crate) fn mock_responses_request_user_input_sse_body(call_id: &str) -> String {
    format!(
        "event: response.created\n\
data: {{\"type\":\"response.created\",\"response\":{{\"id\":\"resp-1\"}}}}\n\n\
event: response.output_item.added\n\
data: {{\"type\":\"response.output_item.added\",\"item\":{{\"type\":\"function_call\",\"id\":{call_id:?},\"call_id\":{call_id:?},\"name\":\"request_user_input\",\"arguments\":\"{{\\\"questions\\\":[{{\\\"id\\\":\\\"confirm_path\\\",\\\"header\\\":\\\"Confirm\\\",\\\"question\\\":\\\"Proceed with the plan?\\\",\\\"options\\\":[{{\\\"label\\\":\\\"Yes (Recommended)\\\",\\\"description\\\":\\\"Continue the current plan.\\\"}},{{\\\"label\\\":\\\"No\\\",\\\"description\\\":\\\"Stop and revisit the approach.\\\"}}]}}]}}\"}}}}\n\n\
event: response.output_item.done\n\
data: {{\"type\":\"response.output_item.done\",\"item\":{{\"type\":\"function_call\",\"id\":{call_id:?},\"call_id\":{call_id:?},\"name\":\"request_user_input\",\"arguments\":\"{{\\\"questions\\\":[{{\\\"id\\\":\\\"confirm_path\\\",\\\"header\\\":\\\"Confirm\\\",\\\"question\\\":\\\"Proceed with the plan?\\\",\\\"options\\\":[{{\\\"label\\\":\\\"Yes (Recommended)\\\",\\\"description\\\":\\\"Continue the current plan.\\\"}},{{\\\"label\\\":\\\"No\\\",\\\"description\\\":\\\"Stop and revisit the approach.\\\"}}]}}]}}\"}}}}\n\n\
event: response.completed\n\
data: {{\"type\":\"response.completed\",\"response\":{{\"id\":\"resp-1\",\"usage\":{{\"input_tokens\":0,\"input_tokens_details\":null,\"output_tokens\":0,\"output_tokens_details\":null,\"total_tokens\":0}}}}}}\n\n"
    )
}

pub(crate) fn mock_responses_namespaced_function_call_sse_body(
    call_id: &str,
    namespace: &str,
    name: &str,
    arguments: &str,
) -> String {
    let created = serde_json::json!({
        "type": "response.created",
        "response": {
            "id": "resp-1",
        },
    });
    let function_call = serde_json::json!({
        "type": "response.output_item.done",
        "item": {
            "type": "function_call",
            "call_id": call_id,
            "namespace": namespace,
            "name": name,
            "arguments": arguments,
        },
    });
    let completed = serde_json::json!({
        "type": "response.completed",
        "response": {
            "id": "resp-1",
            "usage": {
                "input_tokens": 0,
                "input_tokens_details": null,
                "output_tokens": 0,
                "output_tokens_details": null,
                "total_tokens": 0,
            },
        },
    });

    format!(
        "event: response.created\n\
data: {}\n\n\
event: response.output_item.done\n\
data: {}\n\n\
event: response.completed\n\
data: {}\n\n",
        serde_json::to_string(&created).expect("created event should serialize"),
        serde_json::to_string(&function_call).expect("function call should serialize"),
        serde_json::to_string(&completed).expect("completed event should serialize"),
    )
}

pub(crate) fn write_mock_responses_config_toml(
    codex_home: &std::path::Path,
    server_uri: &str,
) -> std::io::Result<()> {
    let config_toml = codex_home.join("config.toml");
    std::fs::write(
        config_toml,
        format!(
            r#"
model = "mock-model"
approval_policy = "never"
sandbox_mode = "read-only"

model_provider = "mock_provider"

[model_providers.mock_provider]
name = "Mock provider for test"
base_url = "{server_uri}/v1"
wire_api = "responses"
request_max_retries = 0
stream_max_retries = 0
"#
        ),
    )
}
