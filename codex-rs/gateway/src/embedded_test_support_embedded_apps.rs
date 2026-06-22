use super::*;

#[derive(Clone)]
pub(crate) struct EmbeddedAppsServerState {
    expected_bearer: String,
    expected_account_id: String,
}

#[derive(Clone, Default)]
pub(crate) struct EmbeddedElicitationAppsMcpServer;

#[derive(Clone, Default)]
pub(crate) struct EmbeddedPluginOauthMcpServer;

impl ServerHandler for EmbeddedElicitationAppsMcpServer {
    fn get_info(&self) -> ServerInfo {
        let mut info = ServerInfo::default();
        info.protocol_version = rmcp::model::ProtocolVersion::V_2025_06_18;
        info.capabilities = ServerCapabilities::builder().enable_tools().build();
        info
    }

    async fn list_tools(
        &self,
        _request: Option<rmcp::model::PaginatedRequestParams>,
        _context: RequestContext<RoleServer>,
    ) -> Result<ListToolsResult, rmcp::ErrorData> {
        let input_schema: JsonObject = serde_json::from_value(serde_json::json!({
            "type": "object",
            "additionalProperties": false
        }))
        .map_err(|err| rmcp::ErrorData::internal_error(err.to_string(), None))?;

        let mut tool = Tool::new(
            Cow::Borrowed(EMBEDDED_TOOL_NAME),
            Cow::Borrowed("Confirm a calendar action."),
            Arc::new(input_schema),
        );
        tool.annotations = Some(ToolAnnotations::new().read_only(true));

        let mut meta = Meta::new();
        meta.0.insert(
            "connector_id".to_string(),
            serde_json::json!(EMBEDDED_CONNECTOR_ID),
        );
        meta.0.insert(
            "connector_name".to_string(),
            serde_json::json!(EMBEDDED_CONNECTOR_NAME),
        );
        tool.meta = Some(meta);

        Ok(ListToolsResult {
            tools: vec![tool],
            next_cursor: None,
            meta: None,
        })
    }

    async fn call_tool(
        &self,
        _request: CallToolRequestParams,
        context: RequestContext<RoleServer>,
    ) -> Result<CallToolResult, rmcp::ErrorData> {
        let requested_schema = ElicitationSchema::builder()
            .required_property("confirmed", PrimitiveSchema::Boolean(BooleanSchema::new()))
            .build()
            .map_err(|err| rmcp::ErrorData::internal_error(err.to_string(), None))?;

        let result = context
            .peer
            .create_elicitation(CreateElicitationRequestParams::FormElicitationParams {
                meta: None,
                message: EMBEDDED_ELICITATION_MESSAGE.to_string(),
                requested_schema,
            })
            .await
            .map_err(|err| rmcp::ErrorData::internal_error(err.to_string(), None))?;

        let output = match result.action {
            ElicitationAction::Accept => {
                pretty_assertions::assert_eq!(
                    result.content,
                    Some(serde_json::json!({
                        "confirmed": true,
                    }))
                );
                "accepted"
            }
            ElicitationAction::Decline => "declined",
            ElicitationAction::Cancel => "cancelled",
        };

        Ok(CallToolResult::success(vec![Content::text(output)]))
    }
}

impl ServerHandler for EmbeddedPluginOauthMcpServer {
    fn get_info(&self) -> ServerInfo {
        let mut info = ServerInfo::default();
        info.protocol_version = rmcp::model::ProtocolVersion::V_2025_06_18;
        info.capabilities = ServerCapabilities::default();
        info
    }
}

pub(crate) async fn start_embedded_gateway_apps_server() -> anyhow::Result<(String, JoinHandle<()>)>
{
    let state = Arc::new(EmbeddedAppsServerState {
        expected_bearer: "Bearer chatgpt-token".to_string(),
        expected_account_id: "account-123".to_string(),
    });

    let listener = TcpListener::bind("127.0.0.1:0").await?;
    let addr = listener.local_addr()?;
    let metadata_base = format!("http://{addr}");

    let mcp_service = StreamableHttpService::new(
        move || Ok(EmbeddedElicitationAppsMcpServer),
        Arc::new(LocalSessionManager::default()),
        StreamableHttpServerConfig::default(),
    );

    let router = Router::new()
        .route(
            "/.well-known/oauth-authorization-server/mcp",
            get({
                move || {
                    let metadata_base = metadata_base.clone();
                    async move {
                        Json(serde_json::json!({
                            "authorization_endpoint": format!(
                                "{metadata_base}/oauth/authorize"
                            ),
                            "token_endpoint": format!("{metadata_base}/oauth/token"),
                            "scopes_supported": [""],
                        }))
                    }
                }
            }),
        )
        .route(
            "/connectors/directory/list",
            get(list_embedded_directory_connectors),
        )
        .route(
            "/connectors/directory/list_workspace",
            get(list_embedded_directory_connectors),
        )
        .route("/api/codex/usage", get(read_embedded_rate_limits))
        .with_state(state)
        .nest_service("/api/codex/ps/mcp", mcp_service)
        .fallback(|method: axum::http::Method, uri: Uri| async move {
            tracing::error!(%method, %uri, "embedded apps mock unmatched request");
            StatusCode::NOT_FOUND
        });

    let handle = tokio::spawn(async move {
        let _ = axum::serve(listener, router).await;
    });

    Ok((format!("http://{addr}"), handle))
}

pub(crate) async fn start_embedded_plugin_mcp_oauth_server()
-> anyhow::Result<(String, JoinHandle<()>)> {
    let listener = TcpListener::bind("127.0.0.1:0").await?;
    let addr = listener.local_addr()?;
    let metadata_base = format!("http://{addr}");

    let mcp_service = StreamableHttpService::new(
        move || Ok(EmbeddedPluginOauthMcpServer),
        Arc::new(LocalSessionManager::default()),
        StreamableHttpServerConfig::default(),
    );

    let router = Router::new()
        .route(
            "/.well-known/oauth-authorization-server/mcp",
            get({
                move || {
                    let metadata_base = metadata_base.clone();
                    async move {
                        Json(serde_json::json!({
                            "authorization_endpoint": format!(
                                "{metadata_base}/oauth/authorize"
                            ),
                            "registration_endpoint": format!(
                                "{metadata_base}/oauth/register"
                            ),
                            "token_endpoint": format!("{metadata_base}/oauth/token"),
                            "scopes_supported": ["calendar.read"],
                        }))
                    }
                }
            }),
        )
        .route("/oauth/authorize", get(embedded_plugin_mcp_oauth_authorize))
        .route("/oauth/register", post(embedded_plugin_mcp_oauth_register))
        .route("/oauth/token", post(embedded_plugin_mcp_oauth_token))
        .nest_service("/mcp", mcp_service);

    let handle = tokio::spawn(async move {
        let _ = axum::serve(listener, router).await;
    });

    Ok((format!("http://{addr}"), handle))
}

pub(crate) async fn embedded_plugin_mcp_oauth_authorize(
    Query(params): Query<HashMap<String, String>>,
) -> Result<Redirect, StatusCode> {
    let redirect_uri = params.get("redirect_uri").ok_or(StatusCode::BAD_REQUEST)?;
    let state = params.get("state").ok_or(StatusCode::BAD_REQUEST)?;
    let separator = if redirect_uri.contains('?') { "&" } else { "?" };
    Ok(Redirect::temporary(&format!(
        "{redirect_uri}{separator}code=embedded-oauth-code&state={state}"
    )))
}

pub(crate) async fn embedded_plugin_mcp_oauth_token() -> Json<serde_json::Value> {
    Json(serde_json::json!({
        "access_token": "embedded-plugin-access-token",
        "token_type": "Bearer",
        "refresh_token": "embedded-plugin-refresh-token",
        "expires_in": 3600,
    }))
}

pub(crate) async fn embedded_plugin_mcp_oauth_register(
    Json(request): Json<serde_json::Value>,
) -> Json<serde_json::Value> {
    let redirect_uris = request
        .get("redirect_uris")
        .cloned()
        .unwrap_or_else(|| serde_json::json!(["http://127.0.0.1/callback"]));
    Json(serde_json::json!({
        "client_id": "embedded-plugin-client-id",
        "client_name": "demo-mcp",
        "redirect_uris": redirect_uris,
        "token_endpoint_auth_method": "none",
        "grant_types": ["authorization_code", "refresh_token"],
        "response_types": ["code"],
    }))
}

pub(crate) async fn list_embedded_directory_connectors(
    State(state): State<Arc<EmbeddedAppsServerState>>,
    headers: HeaderMap,
    uri: Uri,
) -> Result<Json<serde_json::Value>, StatusCode> {
    let auth_ok = embedded_apps_auth_ok(&state, &headers);
    let external_logos_ok = uri
        .query()
        .is_some_and(|query| query.split('&').any(|pair| pair == "external_logos=true"));

    if !auth_ok {
        Err(StatusCode::UNAUTHORIZED)
    } else if !external_logos_ok {
        Err(StatusCode::BAD_REQUEST)
    } else {
        Ok(Json(serde_json::json!({
            "apps": [{
                "id": EMBEDDED_CONNECTOR_ID,
                "name": EMBEDDED_CONNECTOR_NAME,
                "description": "Calendar connector",
                "logo_url": null,
                "logo_url_dark": null,
                "distribution_channel": null,
                "branding": null,
                "app_metadata": null,
                "labels": null,
                "install_url": null,
                "is_accessible": false,
                "is_enabled": true
            }],
            "next_token": null
        })))
    }
}

pub(crate) async fn read_embedded_rate_limits(
    State(state): State<Arc<EmbeddedAppsServerState>>,
    headers: HeaderMap,
) -> Result<Json<serde_json::Value>, StatusCode> {
    if !embedded_apps_auth_ok(&state, &headers) {
        return Err(StatusCode::UNAUTHORIZED);
    }

    Ok(Json(serde_json::json!({
        "plan_type": "pro",
        "rate_limit": {
            "allowed": true,
            "limit_reached": false,
            "primary_window": {
                "used_percent": 42,
                "limit_window_seconds": 3600,
                "reset_after_seconds": 120,
                "reset_at": 1735689720,
            },
            "secondary_window": {
                "used_percent": 5,
                "limit_window_seconds": 86400,
                "reset_after_seconds": 3600,
                "reset_at": 1735693200,
            }
        },
        "rate_limit_reached_type": {
            "type": "workspace_member_usage_limit_reached",
        },
        "additional_rate_limits": [
            {
                "limit_name": "codex_other",
                "metered_feature": "codex_other",
                "rate_limit": {
                    "allowed": true,
                    "limit_reached": false,
                    "primary_window": {
                        "used_percent": 88,
                        "limit_window_seconds": 1800,
                        "reset_after_seconds": 600,
                        "reset_at": 1735693200,
                    }
                }
            }
        ]
    })))
}

pub(crate) fn embedded_apps_auth_ok(state: &EmbeddedAppsServerState, headers: &HeaderMap) -> bool {
    let bearer_ok = headers
        .get(AUTHORIZATION)
        .and_then(|value| value.to_str().ok())
        .is_some_and(|value| value == state.expected_bearer);
    let account_ok = headers
        .get("chatgpt-account-id")
        .and_then(|value| value.to_str().ok())
        .is_some_and(|value| value == state.expected_account_id);
    bearer_ok && account_ok
}

pub(crate) fn write_embedded_mcp_config_toml(
    codex_home: &std::path::Path,
    responses_server_uri: &str,
    apps_server_url: &str,
) -> std::io::Result<()> {
    std::fs::write(
        codex_home.join("config.toml"),
        format!(
            r#"
model = "mock-model"
approval_policy = "untrusted"
sandbox_mode = "read-only"

model_provider = "mock_provider"
chatgpt_base_url = "{apps_server_url}"
mcp_oauth_credentials_store = "file"

[features]
apps = true

[model_providers.mock_provider]
name = "Mock provider for test"
base_url = "{responses_server_uri}/v1"
wire_api = "responses"
request_max_retries = 0
stream_max_retries = 0
"#
        ),
    )
}
