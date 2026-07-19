use super::*;
use pretty_assertions::assert_eq;

pub(super) async fn assert_bootstrap_setup_discovery_and_mcp(client: &mut RemoteAppServerClient) {
    let detected: ExternalAgentConfigDetectResponse = client
        .request_typed(ClientRequest::ExternalAgentConfigDetect {
            request_id: RequestId::Integer(1),
            params: ExternalAgentConfigDetectParams {
                include_home: false,
                cwds: Some(vec![PathBuf::from("/tmp/remote-project")]),
                source: None,
                migration_source: None,
            },
        })
        .await
        .expect("externalAgentConfig/detect should succeed through remote gateway");
    assert_eq!(detected.items.is_empty(), true);

    let imported: ExternalAgentConfigImportResponse = client
        .request_typed(ClientRequest::ExternalAgentConfigImport {
            request_id: RequestId::Integer(2),
            params: ExternalAgentConfigImportParams {
                migration_items: Vec::new(),
                source: None,
                migration_source: None,
            },
        })
        .await
        .expect("externalAgentConfig/import should succeed through remote gateway");
    assert!(!imported.import_id.is_empty());

    let import_completed = timeout(Duration::from_secs(5), async {
        loop {
            let event = client
                .next_event()
                .await
                .expect("event stream should stay open");
            if let AppServerEvent::ServerNotification(
                ServerNotification::ExternalAgentConfigImportCompleted(notification),
            ) = event
            {
                break notification;
            }
        }
    })
    .await
    .expect("externalAgentConfig/import/completed notification should arrive");
    assert_eq!(import_completed.item_type_results, Vec::new());
    assert!(!import_completed.import_id.is_empty());

    let config_read: ConfigReadResponse = client
        .request_typed(ClientRequest::ConfigRead {
            request_id: RequestId::Integer(3),
            params: ConfigReadParams {
                include_layers: true,
                cwd: Some("/tmp/remote-project".to_string()),
            },
        })
        .await
        .expect("config/read should succeed through remote gateway");
    assert_eq!(config_read.config.model.as_deref(), Some("gpt-5"));
    assert_eq!(config_read.layers, None);

    let config_requirements: ConfigRequirementsReadResponse = client
        .request_typed(ClientRequest::ConfigRequirementsRead {
            request_id: RequestId::Integer(4),
            params: None,
        })
        .await
        .expect("configRequirements/read should succeed through remote gateway");
    assert_eq!(config_requirements.requirements, None);

    let experimental_features: ExperimentalFeatureListResponse = client
        .request_typed(ClientRequest::ExperimentalFeatureList {
            request_id: RequestId::Integer(5),
            params: ExperimentalFeatureListParams {
                cursor: None,
                limit: Some(20),
                ..Default::default()
            },
        })
        .await
        .expect("experimentalFeature/list should succeed through remote gateway");
    assert_eq!(experimental_features.data.len(), 1);
    assert_eq!(experimental_features.data[0].name, "gateway-test-feature");

    let feature_enablement: ExperimentalFeatureEnablementSetResponse = client
        .request_typed(ClientRequest::ExperimentalFeatureEnablementSet {
            request_id: RequestId::Integer(41),
            params: ExperimentalFeatureEnablementSetParams {
                enablement: std::collections::BTreeMap::from([(
                    "gateway-test-feature".to_string(),
                    true,
                )]),
            },
        })
        .await
        .expect("experimentalFeature/enablement/set should succeed through remote gateway");
    assert_eq!(
        feature_enablement.enablement,
        std::collections::BTreeMap::from([("gateway-test-feature".to_string(), true)])
    );

    let collaboration_modes: CollaborationModeListResponse = client
        .request_typed(ClientRequest::CollaborationModeList {
            request_id: RequestId::Integer(6),
            params: CollaborationModeListParams::default(),
        })
        .await
        .expect("collaborationMode/list should succeed through remote gateway");
    assert_eq!(collaboration_modes.data.len(), 1);
    assert_eq!(collaboration_modes.data[0].name, "default");

    let rate_limits: GetAccountRateLimitsResponse = client
        .request_typed(ClientRequest::GetAccountRateLimits {
            request_id: RequestId::Integer(7),
            params: None,
        })
        .await
        .expect("account/rateLimits/read should succeed through remote gateway");
    assert_eq!(rate_limits.rate_limits.limit_id.as_deref(), Some("codex"));
    assert_eq!(
        rate_limits
            .rate_limits_by_limit_id
            .as_ref()
            .and_then(|rate_limits_by_limit_id| rate_limits_by_limit_id.get("codex"))
            .and_then(|snapshot| snapshot.limit_name.as_deref()),
        Some("Codex")
    );

    let legacy_auth: GetAuthStatusResponse = client
        .request_typed(ClientRequest::GetAuthStatus {
            request_id: RequestId::Integer(49),
            params: GetAuthStatusParams {
                include_token: Some(true),
                refresh_token: Some(false),
            },
        })
        .await
        .expect("getAuthStatus should succeed through remote gateway");
    assert_eq!(
        legacy_auth,
        GetAuthStatusResponse {
            auth_method: None,
            auth_token: Some("remote-auth-token".to_string()),
            requires_openai_auth: Some(false),
        }
    );

    let skills: SkillsListResponse = client
        .request_typed(ClientRequest::SkillsList {
            request_id: RequestId::Integer(8),
            params: SkillsListParams {
                cwds: vec![PathBuf::from("/tmp/remote-project")],
                force_reload: false,
            },
        })
        .await
        .expect("skills/list should succeed through remote gateway");
    assert_eq!(skills.data.len(), 1);
    assert_eq!(skills.data[0].cwd, PathBuf::from("/tmp/remote-project"));
    assert_eq!(skills.data[0].errors, Vec::new());
    assert_eq!(skills.data[0].skills, Vec::new());

    let skills_config: SkillsConfigWriteResponse = client
        .request_typed(ClientRequest::SkillsConfigWrite {
            request_id: RequestId::Integer(42),
            params: SkillsConfigWriteParams {
                path: Some(
                    PathBuf::from("/tmp/remote-project/skills/remote-skill")
                        .try_into()
                        .expect("skills/config/write path should be absolute"),
                ),
                name: None,
                enabled: true,
            },
        })
        .await
        .expect("skills/config/write should succeed through remote gateway");
    assert_eq!(skills_config.effective_enabled, true);

    let apps: AppsListResponse = client
        .request_typed(ClientRequest::AppsList {
            request_id: RequestId::Integer(9),
            params: AppsListParams {
                cursor: None,
                limit: Some(10),
                thread_id: None,
                force_refetch: false,
            },
        })
        .await
        .expect("app/list should succeed through remote gateway");
    assert_eq!(apps.next_cursor, None);
    assert_eq!(apps.data.len(), 1);
    assert_eq!(apps.data[0].id, "remote-app");
    assert_eq!(apps.data[0].name, "Remote App");

    let mcp_statuses: ListMcpServerStatusResponse = client
        .request_typed(ClientRequest::McpServerStatusList {
            request_id: RequestId::Integer(10),
            params: ListMcpServerStatusParams {
                cursor: None,
                limit: Some(10),
                detail: Some(McpServerStatusDetail::ToolsAndAuthOnly),
                thread_id: None,
            },
        })
        .await
        .expect("mcpServerStatus/list should succeed through remote gateway");
    assert_eq!(mcp_statuses.next_cursor, None);
    assert_eq!(mcp_statuses.data.len(), 1);
    assert_eq!(mcp_statuses.data[0].name, "remote-mcp");

    let mcp_refresh: McpServerRefreshResponse = client
        .request_typed(ClientRequest::McpServerRefresh {
            request_id: RequestId::Integer(43),
            params: None,
        })
        .await
        .expect("config/mcpServer/reload should succeed through remote gateway");
    assert_eq!(mcp_refresh, McpServerRefreshResponse {});

    let mcp_oauth_login: McpServerOauthLoginResponse = client
        .request_typed(ClientRequest::McpServerOauthLogin {
            request_id: RequestId::Integer(11),
            params: McpServerOauthLoginParams {
                name: "remote-mcp".to_string(),
                scopes: Some(vec!["calendar.read".to_string()]),
                timeout_secs: Some(30),
                thread_id: None,
            },
        })
        .await
        .expect("mcpServer/oauth/login should succeed through remote gateway");
    assert_eq!(
        mcp_oauth_login.authorization_url,
        "https://example.com/oauth/remote-mcp"
    );

    let mcp_oauth_completed = timeout(Duration::from_secs(5), async {
        loop {
            let event = client
                .next_event()
                .await
                .expect("event stream should stay open");
            if let AppServerEvent::ServerNotification(
                ServerNotification::McpServerOauthLoginCompleted(notification),
            ) = event
            {
                break notification;
            }
        }
    })
    .await
    .expect("mcpServer/oauthLogin/completed notification should arrive");
    assert_eq!(mcp_oauth_completed.name, "remote-mcp");
    assert_eq!(mcp_oauth_completed.success, true);
    assert_eq!(mcp_oauth_completed.error, None);

    let started_for_mcp: AppServerThreadStartResponse = client
        .request_typed(ClientRequest::ThreadStart {
            request_id: RequestId::Integer(48),
            params: ThreadStartParams {
                model: None,
                model_provider: None,
                service_tier: None,
                cwd: Some("/tmp/remote-project".to_string()),
                approval_policy: None,
                approvals_reviewer: None,
                sandbox: None,
                config: None,
                service_name: None,
                base_instructions: None,
                developer_instructions: None,
                personality: None,
                ephemeral: Some(false),
                session_start_source: None,
                dynamic_tools: None,
                mock_experimental_field: None,
                experimental_raw_events: false,
                ..Default::default()
            },
        })
        .await
        .expect("thread/start should make MCP thread-scoped requests visible");
    assert_eq!(started_for_mcp.thread.id, "thread-remote-workflow");

    let mcp_resource: McpResourceReadResponse = client
        .request_typed(ClientRequest::McpResourceRead {
            request_id: RequestId::Integer(44),
            params: McpResourceReadParams {
                thread_id: Some(started_for_mcp.thread.id.clone()),
                server: "remote-mcp".to_string(),
                uri: "file:///tmp/remote-project/context.md".to_string(),
            },
        })
        .await
        .expect("mcpServer/resource/read should succeed through remote gateway");
    assert_eq!(mcp_resource.contents.len(), 1);

    let mcp_tool: McpServerToolCallResponse = client
        .request_typed(ClientRequest::McpServerToolCall {
            request_id: RequestId::Integer(45),
            params: McpServerToolCallParams {
                thread_id: started_for_mcp.thread.id.clone(),
                server: "remote-mcp".to_string(),
                tool: "lookup".to_string(),
                arguments: Some(serde_json::json!({
                    "query": "gateway",
                })),
                meta: None,
            },
        })
        .await
        .expect("mcpServer/tool/call should succeed through remote gateway");
    assert_eq!(mcp_tool.content.len(), 1);
    assert_eq!(mcp_tool.is_error, Some(false));
}
