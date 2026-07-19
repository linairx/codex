use super::*;
use pretty_assertions::assert_eq;

pub(super) async fn assert_bootstrap_refresh_discovery_requests(
    v2_client: &mut RemoteAppServerClient,
) {
    let account: GetAccountResponse = timeout(
        Duration::from_secs(5),
        v2_client.request_typed(ClientRequest::GetAccount {
            request_id: RequestId::Integer(1),
            params: GetAccountParams {
                refresh_token: false,
            },
        }),
    )
    .await
    .expect("account/read should finish in time after worker reconnect")
    .expect("account/read should succeed after worker reconnect");
    assert_eq!(
        account,
        GetAccountResponse {
            account: Some(Account::Chatgpt {
                email: Some("gateway@example.com".to_string()),
                plan_type: AccountPlanType::Pro,
            }),
            requires_openai_auth: false,
        }
    );

    let codex_rate_limit = RateLimitSnapshot {
        limit_id: Some("codex".to_string()),
        limit_name: Some("Codex".to_string()),
        primary: Some(RateLimitWindow {
            used_percent: 42,
            window_duration_mins: Some(300),
            resets_at: Some(1_700_000_000),
        }),
        secondary: None,
        credits: None,
        plan_type: Some(AccountPlanType::Pro),
        rate_limit_reached_type: None,
        individual_limit: None,
        spend_control_reached: None,
    };
    let rate_limits: GetAccountRateLimitsResponse = timeout(
        Duration::from_secs(5),
        v2_client.request_typed(ClientRequest::GetAccountRateLimits {
            request_id: RequestId::Integer(2),
            params: None,
        }),
    )
    .await
    .expect("account/rateLimits/read should finish in time after worker reconnect")
    .expect("account/rateLimits/read should succeed after worker reconnect");
    assert_eq!(
        rate_limits,
        GetAccountRateLimitsResponse {
            rate_limits: codex_rate_limit.clone(),
            rate_limits_by_limit_id: Some(
                [("codex".to_string(), codex_rate_limit)]
                    .into_iter()
                    .collect(),
            ),
            rate_limit_reset_credits: None,
        }
    );

    let models: ModelListResponse = timeout(
        Duration::from_secs(5),
        v2_client.request_typed(ClientRequest::ModelList {
            request_id: RequestId::Integer(3),
            params: ModelListParams {
                cursor: None,
                limit: None,
                include_hidden: Some(true),
            },
        }),
    )
    .await
    .expect("model/list should finish in time after worker reconnect")
    .expect("model/list should succeed after worker reconnect");
    assert_eq!(models.next_cursor, None);
    assert_eq!(models.data.len(), 1);
    assert_eq!(models.data[0].id, "openai/gpt-5");

    let external_agent_config: ExternalAgentConfigDetectResponse = timeout(
        Duration::from_secs(5),
        v2_client.request_typed(ClientRequest::ExternalAgentConfigDetect {
            request_id: RequestId::Integer(4),
            params: ExternalAgentConfigDetectParams {
                include_home: true,
                cwds: Some(vec![PathBuf::from("/tmp/reconnected-project")]),
                source: None,
                migration_source: None,
            },
        }),
    )
    .await
    .expect("externalAgentConfig/detect should finish in time after worker reconnect")
    .expect("externalAgentConfig/detect should succeed after worker reconnect");
    assert_eq!(external_agent_config.items.len(), 1);
    assert_eq!(
        external_agent_config.items[0].description,
        "reconnected repo config"
    );

    let apps: AppsListResponse = timeout(
        Duration::from_secs(5),
        v2_client.request_typed(ClientRequest::AppsList {
            request_id: RequestId::Integer(5),
            params: AppsListParams {
                cursor: None,
                limit: Some(10),
                thread_id: None,
                force_refetch: false,
            },
        }),
    )
    .await
    .expect("app/list should finish in time after worker reconnect")
    .expect("app/list should succeed after worker reconnect");
    assert_eq!(apps.next_cursor, None);
    assert_eq!(apps.data.len(), 1);
    assert_eq!(apps.data[0].id, "reconnected-app");

    let skills: SkillsListResponse = timeout(
        Duration::from_secs(5),
        v2_client.request_typed(ClientRequest::SkillsList {
            request_id: RequestId::Integer(6),
            params: SkillsListParams {
                cwds: vec![PathBuf::from("/tmp/reconnected-project")],
                force_reload: false,
            },
        }),
    )
    .await
    .expect("skills/list should finish in time after worker reconnect")
    .expect("skills/list should succeed after worker reconnect");
    assert_eq!(skills.data.len(), 1);
    assert_eq!(
        skills.data[0].cwd,
        PathBuf::from("/tmp/reconnected-project")
    );
    assert_eq!(skills.data[0].skills.len(), 1);
    assert_eq!(skills.data[0].skills[0].name, "reconnected-skill");
}
