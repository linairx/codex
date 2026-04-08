use crate::bottom_pane::FeedbackAudience;
use crate::status::StatusAccountDisplay;
use crate::status::plan_type_display_name;
use codex_app_server_client::AppServerClient;
use codex_app_server_client::AppServerEvent;
use codex_app_server_client::AppServerRequestHandle;
use codex_app_server_client::TypedRequestError;
use codex_app_server_protocol::Account;
use codex_app_server_protocol::AuthMode;
use codex_app_server_protocol::ClientRequest;
use codex_app_server_protocol::ConfigBatchWriteParams;
use codex_app_server_protocol::ConfigWriteResponse;
use codex_app_server_protocol::GetAccountParams;
use codex_app_server_protocol::GetAccountRateLimitsResponse;
use codex_app_server_protocol::GetAccountResponse;
use codex_app_server_protocol::JSONRPCErrorError;
use codex_app_server_protocol::Model as ApiModel;
use codex_app_server_protocol::ModelListParams;
use codex_app_server_protocol::ModelListResponse;
use codex_app_server_protocol::RequestId;
use codex_app_server_protocol::ReviewDelivery;
use codex_app_server_protocol::ReviewStartParams;
use codex_app_server_protocol::ReviewStartResponse;
use codex_app_server_protocol::SkillsListParams;
use codex_app_server_protocol::SkillsListResponse;
use codex_app_server_protocol::Thread;
use codex_app_server_protocol::ThreadBackgroundTerminalsCleanParams;
use codex_app_server_protocol::ThreadBackgroundTerminalsCleanResponse;
use codex_app_server_protocol::ThreadCompactStartParams;
use codex_app_server_protocol::ThreadCompactStartResponse;
use codex_app_server_protocol::ThreadForkParams;
use codex_app_server_protocol::ThreadForkResponse;
use codex_app_server_protocol::ThreadListParams;
use codex_app_server_protocol::ThreadListResponse;
use codex_app_server_protocol::ThreadLoadedListParams;
use codex_app_server_protocol::ThreadLoadedListResponse;
use codex_app_server_protocol::ThreadLoadedReadParams;
use codex_app_server_protocol::ThreadLoadedReadResponse;
use codex_app_server_protocol::ThreadMode;
use codex_app_server_protocol::ThreadReadParams;
use codex_app_server_protocol::ThreadReadResponse;
use codex_app_server_protocol::ThreadRealtimeAppendAudioParams;
use codex_app_server_protocol::ThreadRealtimeAppendAudioResponse;
use codex_app_server_protocol::ThreadRealtimeAppendTextParams;
use codex_app_server_protocol::ThreadRealtimeAppendTextResponse;
use codex_app_server_protocol::ThreadRealtimeStartParams;
use codex_app_server_protocol::ThreadRealtimeStartResponse;
use codex_app_server_protocol::ThreadRealtimeStopParams;
use codex_app_server_protocol::ThreadRealtimeStopResponse;
use codex_app_server_protocol::ThreadResumeParams;
use codex_app_server_protocol::ThreadResumeResponse;
use codex_app_server_protocol::ThreadRollbackParams;
use codex_app_server_protocol::ThreadRollbackResponse;
use codex_app_server_protocol::ThreadSetNameParams;
use codex_app_server_protocol::ThreadSetNameResponse;
use codex_app_server_protocol::ThreadShellCommandParams;
use codex_app_server_protocol::ThreadShellCommandResponse;
use codex_app_server_protocol::ThreadStartParams;
use codex_app_server_protocol::ThreadStartResponse;
use codex_app_server_protocol::ThreadUnsubscribeParams;
use codex_app_server_protocol::ThreadUnsubscribeResponse;
use codex_app_server_protocol::Turn;
use codex_app_server_protocol::TurnInterruptParams;
use codex_app_server_protocol::TurnInterruptResponse;
use codex_app_server_protocol::TurnStartParams;
use codex_app_server_protocol::TurnStartResponse;
use codex_app_server_protocol::TurnSteerParams;
use codex_app_server_protocol::TurnSteerResponse;
use codex_core::config::Config;
use codex_core::message_history;
use codex_otel::TelemetryAuthMode;
use codex_protocol::ThreadId;
use codex_protocol::openai_models::ModelAvailabilityNux;
use codex_protocol::openai_models::ModelPreset;
use codex_protocol::openai_models::ModelUpgrade;
use codex_protocol::openai_models::ReasoningEffortPreset;
use codex_protocol::protocol::AskForApproval;
use codex_protocol::protocol::ConversationAudioParams;
use codex_protocol::protocol::ConversationStartParams;
use codex_protocol::protocol::ConversationTextParams;
use codex_protocol::protocol::CreditsSnapshot;
use codex_protocol::protocol::RateLimitSnapshot;
use codex_protocol::protocol::RateLimitWindow;
use codex_protocol::protocol::ReviewRequest;
use codex_protocol::protocol::ReviewTarget as CoreReviewTarget;
use codex_protocol::protocol::SandboxPolicy;
use codex_protocol::protocol::SessionNetworkProxyRuntime;
use color_eyre::eyre::ContextCompat;
use color_eyre::eyre::Result;
use color_eyre::eyre::WrapErr;
use std::collections::HashMap;
use std::path::PathBuf;

pub(crate) struct AppServerBootstrap {
    pub(crate) account_auth_mode: Option<AuthMode>,
    pub(crate) account_email: Option<String>,
    pub(crate) auth_mode: Option<TelemetryAuthMode>,
    pub(crate) status_account_display: Option<StatusAccountDisplay>,
    pub(crate) plan_type: Option<codex_protocol::account::PlanType>,
    pub(crate) default_model: String,
    pub(crate) feedback_audience: FeedbackAudience,
    pub(crate) has_chatgpt_account: bool,
    pub(crate) available_models: Vec<ModelPreset>,
    pub(crate) rate_limit_snapshots: Vec<RateLimitSnapshot>,
}

pub(crate) struct AppServerSession {
    client: AppServerClient,
    next_request_id: i64,
    remote_cwd_override: Option<PathBuf>,
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct ThreadSessionState {
    pub(crate) thread_id: ThreadId,
    pub(crate) forked_from_id: Option<ThreadId>,
    pub(crate) thread_name: Option<String>,
    pub(crate) thread_mode: Option<ThreadMode>,
    pub(crate) model: String,
    pub(crate) model_provider_id: String,
    pub(crate) service_tier: Option<codex_protocol::config_types::ServiceTier>,
    pub(crate) approval_policy: AskForApproval,
    pub(crate) approvals_reviewer: codex_protocol::config_types::ApprovalsReviewer,
    pub(crate) sandbox_policy: SandboxPolicy,
    pub(crate) cwd: PathBuf,
    pub(crate) reasoning_effort: Option<codex_protocol::openai_models::ReasoningEffort>,
    pub(crate) history_log_id: u64,
    pub(crate) history_entry_count: u64,
    pub(crate) network_proxy: Option<SessionNetworkProxyRuntime>,
    pub(crate) rollout_path: Option<PathBuf>,
}

#[derive(Clone, Copy)]
enum ThreadParamsMode {
    Embedded,
    Remote,
}

impl ThreadParamsMode {
    fn model_provider_from_config(self, config: &Config) -> Option<String> {
        match self {
            Self::Embedded => Some(config.model_provider_id.clone()),
            Self::Remote => None,
        }
    }
}

pub(crate) struct AppServerStartedThread {
    pub(crate) session: ThreadSessionState,
    pub(crate) turns: Vec<Turn>,
}

impl AppServerSession {
    pub(crate) fn new(client: AppServerClient) -> Self {
        Self {
            client,
            next_request_id: 1,
            remote_cwd_override: None,
        }
    }

    pub(crate) fn with_remote_cwd_override(mut self, remote_cwd_override: Option<PathBuf>) -> Self {
        self.remote_cwd_override = remote_cwd_override;
        self
    }

    pub(crate) fn remote_cwd_override(&self) -> Option<&std::path::Path> {
        self.remote_cwd_override.as_deref()
    }

    pub(crate) fn is_remote(&self) -> bool {
        matches!(self.client, AppServerClient::Remote(_))
    }

    pub(crate) async fn bootstrap(&mut self, config: &Config) -> Result<AppServerBootstrap> {
        let account_request_id = self.next_request_id();
        let account: GetAccountResponse = self
            .client
            .request_typed(ClientRequest::GetAccount {
                request_id: account_request_id,
                params: GetAccountParams {
                    refresh_token: false,
                },
            })
            .await
            .wrap_err("account/read failed during TUI bootstrap")?;
        let model_request_id = self.next_request_id();
        let models: ModelListResponse = self
            .client
            .request_typed(ClientRequest::ModelList {
                request_id: model_request_id,
                params: ModelListParams {
                    cursor: None,
                    limit: None,
                    include_hidden: Some(true),
                },
            })
            .await
            .wrap_err("model/list failed during TUI bootstrap")?;
        let available_models = models
            .data
            .into_iter()
            .map(model_preset_from_api_model)
            .collect::<Vec<_>>();
        let default_model = config
            .model
            .clone()
            .or_else(|| {
                available_models
                    .iter()
                    .find(|model| model.is_default)
                    .map(|model| model.model.clone())
            })
            .or_else(|| available_models.first().map(|model| model.model.clone()))
            .wrap_err("model/list returned no models for TUI bootstrap")?;

        let (
            account_auth_mode,
            account_email,
            auth_mode,
            status_account_display,
            plan_type,
            feedback_audience,
            has_chatgpt_account,
        ) = match account.account {
            Some(Account::ApiKey {}) => (
                Some(AuthMode::ApiKey),
                None,
                Some(TelemetryAuthMode::ApiKey),
                Some(StatusAccountDisplay::ApiKey),
                None,
                FeedbackAudience::External,
                false,
            ),
            Some(Account::Chatgpt { email, plan_type }) => {
                let feedback_audience = if email.ends_with("@openai.com") {
                    FeedbackAudience::OpenAiEmployee
                } else {
                    FeedbackAudience::External
                };
                (
                    Some(AuthMode::Chatgpt),
                    Some(email.clone()),
                    Some(TelemetryAuthMode::Chatgpt),
                    Some(StatusAccountDisplay::ChatGpt {
                        email: Some(email),
                        plan: Some(plan_type_display_name(plan_type)),
                    }),
                    Some(plan_type),
                    feedback_audience,
                    true,
                )
            }
            None => (
                None,
                None,
                None,
                None,
                None,
                FeedbackAudience::External,
                false,
            ),
        };
        let rate_limit_snapshots = if account.requires_openai_auth && has_chatgpt_account {
            let rate_limit_request_id = self.next_request_id();
            match self
                .client
                .request_typed(ClientRequest::GetAccountRateLimits {
                    request_id: rate_limit_request_id,
                    params: None,
                })
                .await
            {
                Ok(rate_limits) => app_server_rate_limit_snapshots_to_core(rate_limits),
                Err(err) => {
                    tracing::warn!("account/rateLimits/read failed during TUI bootstrap: {err}");
                    Vec::new()
                }
            }
        } else {
            Vec::new()
        };

        Ok(AppServerBootstrap {
            account_auth_mode,
            account_email,
            auth_mode,
            status_account_display,
            plan_type,
            default_model,
            feedback_audience,
            has_chatgpt_account,
            available_models,
            rate_limit_snapshots,
        })
    }

    pub(crate) async fn next_event(&mut self) -> Option<AppServerEvent> {
        self.client.next_event().await
    }

    pub(crate) async fn start_thread(&mut self, config: &Config) -> Result<AppServerStartedThread> {
        let request_id = self.next_request_id();
        let response: ThreadStartResponse = self
            .client
            .request_typed(ClientRequest::ThreadStart {
                request_id,
                params: thread_start_params_from_config(
                    config,
                    self.thread_params_mode(),
                    self.remote_cwd_override.as_deref(),
                ),
            })
            .await
            .wrap_err("thread/start failed during TUI bootstrap")?;
        started_thread_from_start_response(response, config).await
    }

    pub(crate) async fn resume_thread(
        &mut self,
        config: Config,
        thread_id: ThreadId,
    ) -> Result<AppServerStartedThread> {
        let request_id = self.next_request_id();
        let response: ThreadResumeResponse = self
            .client
            .request_typed(ClientRequest::ThreadResume {
                request_id,
                params: thread_resume_params_from_config(
                    config.clone(),
                    thread_id,
                    self.thread_params_mode(),
                    self.remote_cwd_override.as_deref(),
                ),
            })
            .await
            .wrap_err("thread/resume failed during TUI bootstrap")?;
        started_thread_from_resume_response(response, &config).await
    }

    pub(crate) async fn fork_thread(
        &mut self,
        config: Config,
        thread_id: ThreadId,
    ) -> Result<AppServerStartedThread> {
        let request_id = self.next_request_id();
        let response: ThreadForkResponse = self
            .client
            .request_typed(ClientRequest::ThreadFork {
                request_id,
                params: thread_fork_params_from_config(
                    config.clone(),
                    thread_id,
                    self.thread_params_mode(),
                    self.remote_cwd_override.as_deref(),
                ),
            })
            .await
            .wrap_err("thread/fork failed during TUI bootstrap")?;
        started_thread_from_fork_response(response, &config).await
    }

    fn thread_params_mode(&self) -> ThreadParamsMode {
        match &self.client {
            AppServerClient::InProcess(_) => ThreadParamsMode::Embedded,
            AppServerClient::Remote(_) => ThreadParamsMode::Remote,
        }
    }

    pub(crate) async fn thread_list(
        &mut self,
        params: ThreadListParams,
    ) -> Result<ThreadListResponse> {
        let request_id = self.next_request_id();
        self.client
            .request_typed(ClientRequest::ThreadList { request_id, params })
            .await
            .wrap_err("thread/list failed during TUI session lookup")
    }

    /// Reads loaded thread summaries that the app server currently holds in memory.
    ///
    /// Used by `App::backfill_loaded_subagent_threads` to discover subagent threads that were
    /// spawned before the TUI connected without issuing an extra `thread/read` for every loaded
    /// thread.
    #[cfg_attr(not(test), allow(dead_code))]
    pub(crate) async fn thread_loaded_list(
        &mut self,
        params: ThreadLoadedListParams,
    ) -> Result<ThreadLoadedListResponse> {
        let request_id = self.next_request_id();
        self.client
            .request_typed(ClientRequest::ThreadLoadedList { request_id, params })
            .await
            .wrap_err("failed to list loaded thread ids from app server")
    }

    /// Reads loaded thread summaries that the app server currently holds in memory.
    ///
    /// Used by `App::backfill_loaded_subagent_threads` to discover subagent threads that were
    /// spawned before the TUI connected without issuing an extra `thread/read` for every loaded
    /// thread.
    pub(crate) async fn thread_loaded_read(
        &mut self,
        params: ThreadLoadedReadParams,
    ) -> Result<ThreadLoadedReadResponse> {
        let request_id = self.next_request_id();
        self.client
            .request_typed(ClientRequest::ThreadLoadedRead { request_id, params })
            .await
            .wrap_err("failed to read loaded threads from app server")
    }

    pub(crate) async fn thread_read(
        &mut self,
        thread_id: ThreadId,
        include_turns: bool,
    ) -> Result<Thread> {
        let request_id = self.next_request_id();
        let response: ThreadReadResponse = self
            .client
            .request_typed(ClientRequest::ThreadRead {
                request_id,
                params: ThreadReadParams {
                    thread_id: thread_id.to_string(),
                    include_turns,
                },
            })
            .await
            .wrap_err("thread/read failed during TUI session lookup")?;
        Ok(response.thread)
    }

    #[allow(clippy::too_many_arguments)]
    pub(crate) async fn turn_start(
        &mut self,
        thread_id: ThreadId,
        items: Vec<codex_protocol::user_input::UserInput>,
        cwd: PathBuf,
        approval_policy: AskForApproval,
        approvals_reviewer: codex_protocol::config_types::ApprovalsReviewer,
        sandbox_policy: SandboxPolicy,
        model: String,
        effort: Option<codex_protocol::openai_models::ReasoningEffort>,
        summary: Option<codex_protocol::config_types::ReasoningSummary>,
        service_tier: Option<Option<codex_protocol::config_types::ServiceTier>>,
        collaboration_mode: Option<codex_protocol::config_types::CollaborationMode>,
        personality: Option<codex_protocol::config_types::Personality>,
        output_schema: Option<serde_json::Value>,
    ) -> Result<TurnStartResponse> {
        let request_id = self.next_request_id();
        self.client
            .request_typed(ClientRequest::TurnStart {
                request_id,
                params: TurnStartParams {
                    thread_id: thread_id.to_string(),
                    input: items.into_iter().map(Into::into).collect(),
                    cwd: Some(cwd),
                    approval_policy: Some(approval_policy.into()),
                    approvals_reviewer: Some(approvals_reviewer.into()),
                    sandbox_policy: Some(sandbox_policy.into()),
                    model: Some(model),
                    service_tier,
                    effort,
                    summary,
                    personality,
                    output_schema,
                    collaboration_mode,
                },
            })
            .await
            .wrap_err("turn/start failed in TUI")
    }

    pub(crate) async fn turn_interrupt(
        &mut self,
        thread_id: ThreadId,
        turn_id: String,
    ) -> Result<()> {
        let request_id = self.next_request_id();
        let _: TurnInterruptResponse = self
            .client
            .request_typed(ClientRequest::TurnInterrupt {
                request_id,
                params: TurnInterruptParams {
                    thread_id: thread_id.to_string(),
                    turn_id,
                },
            })
            .await
            .wrap_err("turn/interrupt failed in TUI")?;
        Ok(())
    }

    pub(crate) async fn turn_steer(
        &mut self,
        thread_id: ThreadId,
        turn_id: String,
        items: Vec<codex_protocol::user_input::UserInput>,
    ) -> std::result::Result<TurnSteerResponse, TypedRequestError> {
        let request_id = self.next_request_id();
        self.client
            .request_typed(ClientRequest::TurnSteer {
                request_id,
                params: TurnSteerParams {
                    thread_id: thread_id.to_string(),
                    input: items.into_iter().map(Into::into).collect(),
                    expected_turn_id: turn_id,
                },
            })
            .await
    }

    pub(crate) async fn thread_set_name(
        &mut self,
        thread_id: ThreadId,
        name: String,
    ) -> Result<()> {
        let request_id = self.next_request_id();
        let _: ThreadSetNameResponse = self
            .client
            .request_typed(ClientRequest::ThreadSetName {
                request_id,
                params: ThreadSetNameParams {
                    thread_id: thread_id.to_string(),
                    name,
                },
            })
            .await
            .wrap_err("thread/name/set failed in TUI")?;
        Ok(())
    }

    pub(crate) async fn thread_unsubscribe(&mut self, thread_id: ThreadId) -> Result<()> {
        let request_id = self.next_request_id();
        let _: ThreadUnsubscribeResponse = self
            .client
            .request_typed(ClientRequest::ThreadUnsubscribe {
                request_id,
                params: ThreadUnsubscribeParams {
                    thread_id: thread_id.to_string(),
                },
            })
            .await
            .wrap_err("thread/unsubscribe failed in TUI")?;
        Ok(())
    }

    pub(crate) async fn thread_compact_start(&mut self, thread_id: ThreadId) -> Result<()> {
        let request_id = self.next_request_id();
        let _: ThreadCompactStartResponse = self
            .client
            .request_typed(ClientRequest::ThreadCompactStart {
                request_id,
                params: ThreadCompactStartParams {
                    thread_id: thread_id.to_string(),
                },
            })
            .await
            .wrap_err("thread/compact/start failed in TUI")?;
        Ok(())
    }

    pub(crate) async fn thread_shell_command(
        &mut self,
        thread_id: ThreadId,
        command: String,
    ) -> Result<()> {
        let request_id = self.next_request_id();
        let _: ThreadShellCommandResponse = self
            .client
            .request_typed(ClientRequest::ThreadShellCommand {
                request_id,
                params: ThreadShellCommandParams {
                    thread_id: thread_id.to_string(),
                    command,
                },
            })
            .await
            .wrap_err("thread/shellCommand failed in TUI")?;
        Ok(())
    }

    pub(crate) async fn thread_background_terminals_clean(
        &mut self,
        thread_id: ThreadId,
    ) -> Result<()> {
        let request_id = self.next_request_id();
        let _: ThreadBackgroundTerminalsCleanResponse = self
            .client
            .request_typed(ClientRequest::ThreadBackgroundTerminalsClean {
                request_id,
                params: ThreadBackgroundTerminalsCleanParams {
                    thread_id: thread_id.to_string(),
                },
            })
            .await
            .wrap_err("thread/backgroundTerminals/clean failed in TUI")?;
        Ok(())
    }

    pub(crate) async fn thread_rollback(
        &mut self,
        thread_id: ThreadId,
        num_turns: u32,
    ) -> Result<ThreadRollbackResponse> {
        let request_id = self.next_request_id();
        self.client
            .request_typed(ClientRequest::ThreadRollback {
                request_id,
                params: ThreadRollbackParams {
                    thread_id: thread_id.to_string(),
                    num_turns,
                },
            })
            .await
            .wrap_err("thread/rollback failed in TUI")
    }

    pub(crate) async fn review_start(
        &mut self,
        thread_id: ThreadId,
        review_request: ReviewRequest,
    ) -> Result<ReviewStartResponse> {
        let request_id = self.next_request_id();
        self.client
            .request_typed(ClientRequest::ReviewStart {
                request_id,
                params: ReviewStartParams {
                    thread_id: thread_id.to_string(),
                    target: review_target_to_app_server(review_request.target),
                    delivery: Some(ReviewDelivery::Inline),
                },
            })
            .await
            .wrap_err("review/start failed in TUI")
    }

    pub(crate) async fn skills_list(
        &mut self,
        params: SkillsListParams,
    ) -> Result<SkillsListResponse> {
        let request_id = self.next_request_id();
        self.client
            .request_typed(ClientRequest::SkillsList { request_id, params })
            .await
            .wrap_err("skills/list failed in TUI")
    }

    pub(crate) async fn reload_user_config(&mut self) -> Result<()> {
        let request_id = self.next_request_id();
        let _: ConfigWriteResponse = self
            .client
            .request_typed(ClientRequest::ConfigBatchWrite {
                request_id,
                params: ConfigBatchWriteParams {
                    edits: Vec::new(),
                    file_path: None,
                    expected_version: None,
                    reload_user_config: true,
                },
            })
            .await
            .wrap_err("config/batchWrite failed while reloading user config in TUI")?;
        Ok(())
    }

    pub(crate) async fn thread_realtime_start(
        &mut self,
        thread_id: ThreadId,
        params: ConversationStartParams,
    ) -> Result<()> {
        let request_id = self.next_request_id();
        let _: ThreadRealtimeStartResponse = self
            .client
            .request_typed(ClientRequest::ThreadRealtimeStart {
                request_id,
                params: ThreadRealtimeStartParams {
                    thread_id: thread_id.to_string(),
                    prompt: params.prompt,
                    session_id: params.session_id,
                },
            })
            .await
            .wrap_err("thread/realtime/start failed in TUI")?;
        Ok(())
    }

    pub(crate) async fn thread_realtime_audio(
        &mut self,
        thread_id: ThreadId,
        params: ConversationAudioParams,
    ) -> Result<()> {
        let request_id = self.next_request_id();
        let _: ThreadRealtimeAppendAudioResponse = self
            .client
            .request_typed(ClientRequest::ThreadRealtimeAppendAudio {
                request_id,
                params: ThreadRealtimeAppendAudioParams {
                    thread_id: thread_id.to_string(),
                    audio: params.frame.into(),
                },
            })
            .await
            .wrap_err("thread/realtime/appendAudio failed in TUI")?;
        Ok(())
    }

    pub(crate) async fn thread_realtime_text(
        &mut self,
        thread_id: ThreadId,
        params: ConversationTextParams,
    ) -> Result<()> {
        let request_id = self.next_request_id();
        let _: ThreadRealtimeAppendTextResponse = self
            .client
            .request_typed(ClientRequest::ThreadRealtimeAppendText {
                request_id,
                params: ThreadRealtimeAppendTextParams {
                    thread_id: thread_id.to_string(),
                    text: params.text,
                },
            })
            .await
            .wrap_err("thread/realtime/appendText failed in TUI")?;
        Ok(())
    }

    pub(crate) async fn thread_realtime_stop(&mut self, thread_id: ThreadId) -> Result<()> {
        let request_id = self.next_request_id();
        let _: ThreadRealtimeStopResponse = self
            .client
            .request_typed(ClientRequest::ThreadRealtimeStop {
                request_id,
                params: ThreadRealtimeStopParams {
                    thread_id: thread_id.to_string(),
                },
            })
            .await
            .wrap_err("thread/realtime/stop failed in TUI")?;
        Ok(())
    }

    pub(crate) async fn reject_server_request(
        &self,
        request_id: RequestId,
        error: JSONRPCErrorError,
    ) -> std::io::Result<()> {
        self.client.reject_server_request(request_id, error).await
    }

    pub(crate) async fn resolve_server_request(
        &self,
        request_id: RequestId,
        result: serde_json::Value,
    ) -> std::io::Result<()> {
        self.client.resolve_server_request(request_id, result).await
    }

    pub(crate) async fn shutdown(self) -> std::io::Result<()> {
        self.client.shutdown().await
    }

    pub(crate) fn request_handle(&self) -> AppServerRequestHandle {
        self.client.request_handle()
    }

    fn next_request_id(&mut self) -> RequestId {
        let request_id = self.next_request_id;
        self.next_request_id += 1;
        RequestId::Integer(request_id)
    }
}

pub(crate) fn status_account_display_from_auth_mode(
    auth_mode: Option<AuthMode>,
    plan_type: Option<codex_protocol::account::PlanType>,
) -> Option<StatusAccountDisplay> {
    match auth_mode {
        Some(AuthMode::ApiKey) => Some(StatusAccountDisplay::ApiKey),
        Some(AuthMode::Chatgpt) | Some(AuthMode::ChatgptAuthTokens) => {
            Some(StatusAccountDisplay::ChatGpt {
                email: None,
                plan: plan_type.map(plan_type_display_name),
            })
        }
        None => None,
    }
}

#[allow(dead_code)]
pub(crate) fn feedback_audience_from_account_email(
    account_email: Option<&str>,
) -> FeedbackAudience {
    match account_email {
        Some(email) if email.ends_with("@openai.com") => FeedbackAudience::OpenAiEmployee,
        Some(_) | None => FeedbackAudience::External,
    }
}

fn model_preset_from_api_model(model: ApiModel) -> ModelPreset {
    let upgrade = model.upgrade.map(|upgrade_id| {
        let upgrade_info = model.upgrade_info.clone();
        ModelUpgrade {
            id: upgrade_id,
            reasoning_effort_mapping: None,
            migration_config_key: model.model.clone(),
            model_link: upgrade_info
                .as_ref()
                .and_then(|info| info.model_link.clone()),
            upgrade_copy: upgrade_info
                .as_ref()
                .and_then(|info| info.upgrade_copy.clone()),
            migration_markdown: upgrade_info.and_then(|info| info.migration_markdown),
        }
    });

    ModelPreset {
        id: model.id,
        model: model.model,
        display_name: model.display_name,
        description: model.description,
        default_reasoning_effort: model.default_reasoning_effort,
        supported_reasoning_efforts: model
            .supported_reasoning_efforts
            .into_iter()
            .map(|effort| ReasoningEffortPreset {
                effort: effort.reasoning_effort,
                description: effort.description,
            })
            .collect(),
        supports_personality: model.supports_personality,
        is_default: model.is_default,
        upgrade,
        show_in_picker: !model.hidden,
        availability_nux: model.availability_nux.map(|nux| ModelAvailabilityNux {
            message: nux.message,
        }),
        // `model/list` already returns models filtered for the active client/auth context.
        supported_in_api: true,
        input_modalities: model.input_modalities,
    }
}

fn approvals_reviewer_override_from_config(
    config: &Config,
) -> Option<codex_app_server_protocol::ApprovalsReviewer> {
    Some(config.approvals_reviewer.into())
}

fn config_request_overrides_from_config(
    config: &Config,
) -> Option<HashMap<String, serde_json::Value>> {
    config.active_profile.as_ref().map(|profile| {
        HashMap::from([(
            "profile".to_string(),
            serde_json::Value::String(profile.clone()),
        )])
    })
}

fn sandbox_mode_from_policy(
    policy: SandboxPolicy,
) -> Option<codex_app_server_protocol::SandboxMode> {
    match policy {
        SandboxPolicy::DangerFullAccess => {
            Some(codex_app_server_protocol::SandboxMode::DangerFullAccess)
        }
        SandboxPolicy::ReadOnly { .. } => Some(codex_app_server_protocol::SandboxMode::ReadOnly),
        SandboxPolicy::WorkspaceWrite { .. } => {
            Some(codex_app_server_protocol::SandboxMode::WorkspaceWrite)
        }
        SandboxPolicy::ExternalSandbox { .. } => None,
    }
}

fn thread_start_params_from_config(
    config: &Config,
    thread_params_mode: ThreadParamsMode,
    remote_cwd_override: Option<&std::path::Path>,
) -> ThreadStartParams {
    ThreadStartParams {
        model: config.model.clone(),
        model_provider: thread_params_mode.model_provider_from_config(config),
        cwd: thread_cwd_from_config(config, thread_params_mode, remote_cwd_override),
        approval_policy: Some(config.permissions.approval_policy.value().into()),
        approvals_reviewer: approvals_reviewer_override_from_config(config),
        sandbox: sandbox_mode_from_policy(config.permissions.sandbox_policy.get().clone()),
        config: config_request_overrides_from_config(config),
        ephemeral: Some(config.ephemeral),
        persist_extended_history: true,
        ..ThreadStartParams::default()
    }
}

fn thread_resume_params_from_config(
    config: Config,
    thread_id: ThreadId,
    thread_params_mode: ThreadParamsMode,
    remote_cwd_override: Option<&std::path::Path>,
) -> ThreadResumeParams {
    ThreadResumeParams {
        thread_id: thread_id.to_string(),
        model: config.model.clone(),
        model_provider: thread_params_mode.model_provider_from_config(&config),
        cwd: thread_cwd_from_config(&config, thread_params_mode, remote_cwd_override),
        approval_policy: Some(config.permissions.approval_policy.value().into()),
        approvals_reviewer: approvals_reviewer_override_from_config(&config),
        sandbox: sandbox_mode_from_policy(config.permissions.sandbox_policy.get().clone()),
        config: config_request_overrides_from_config(&config),
        persist_extended_history: true,
        ..ThreadResumeParams::default()
    }
}

fn thread_fork_params_from_config(
    config: Config,
    thread_id: ThreadId,
    thread_params_mode: ThreadParamsMode,
    remote_cwd_override: Option<&std::path::Path>,
) -> ThreadForkParams {
    ThreadForkParams {
        thread_id: thread_id.to_string(),
        model: config.model.clone(),
        model_provider: thread_params_mode.model_provider_from_config(&config),
        cwd: thread_cwd_from_config(&config, thread_params_mode, remote_cwd_override),
        approval_policy: Some(config.permissions.approval_policy.value().into()),
        approvals_reviewer: approvals_reviewer_override_from_config(&config),
        sandbox: sandbox_mode_from_policy(config.permissions.sandbox_policy.get().clone()),
        config: config_request_overrides_from_config(&config),
        ephemeral: config.ephemeral,
        persist_extended_history: true,
        ..ThreadForkParams::default()
    }
}

fn thread_cwd_from_config(
    config: &Config,
    thread_params_mode: ThreadParamsMode,
    remote_cwd_override: Option<&std::path::Path>,
) -> Option<String> {
    match thread_params_mode {
        ThreadParamsMode::Embedded => Some(config.cwd.to_string_lossy().to_string()),
        ThreadParamsMode::Remote => {
            remote_cwd_override.map(|cwd| cwd.to_string_lossy().to_string())
        }
    }
}

async fn started_thread_from_start_response(
    response: ThreadStartResponse,
    config: &Config,
) -> Result<AppServerStartedThread> {
    let session = thread_session_state_from_thread_start_response(&response, config)
        .await
        .map_err(color_eyre::eyre::Report::msg)?;
    Ok(AppServerStartedThread {
        session,
        turns: response.thread.turns,
    })
}

async fn started_thread_from_resume_response(
    response: ThreadResumeResponse,
    config: &Config,
) -> Result<AppServerStartedThread> {
    let session = thread_session_state_from_thread_resume_response(&response, config)
        .await
        .map_err(color_eyre::eyre::Report::msg)?;
    Ok(AppServerStartedThread {
        session,
        turns: response.thread.turns,
    })
}

async fn started_thread_from_fork_response(
    response: ThreadForkResponse,
    config: &Config,
) -> Result<AppServerStartedThread> {
    let session = thread_session_state_from_thread_fork_response(&response, config)
        .await
        .map_err(color_eyre::eyre::Report::msg)?;
    Ok(AppServerStartedThread {
        session,
        turns: response.thread.turns,
    })
}

async fn thread_session_state_from_thread_start_response(
    response: &ThreadStartResponse,
    config: &Config,
) -> Result<ThreadSessionState, String> {
    thread_session_state_from_thread_response(
        &response.thread.id,
        response.thread.forked_from_id.clone(),
        response.thread.name.clone(),
        Some(response.thread.mode),
        response.thread.path.clone(),
        response.model.clone(),
        response.model_provider.clone(),
        response.service_tier,
        response.approval_policy.to_core(),
        response.approvals_reviewer.to_core(),
        response.sandbox.to_core(),
        response.cwd.clone(),
        response.reasoning_effort,
        config,
    )
    .await
}

async fn thread_session_state_from_thread_resume_response(
    response: &ThreadResumeResponse,
    config: &Config,
) -> Result<ThreadSessionState, String> {
    thread_session_state_from_thread_response(
        &response.thread.id,
        response.thread.forked_from_id.clone(),
        response.thread.name.clone(),
        Some(response.thread.mode),
        response.thread.path.clone(),
        response.model.clone(),
        response.model_provider.clone(),
        response.service_tier,
        response.approval_policy.to_core(),
        response.approvals_reviewer.to_core(),
        response.sandbox.to_core(),
        response.cwd.clone(),
        response.reasoning_effort,
        config,
    )
    .await
}

async fn thread_session_state_from_thread_fork_response(
    response: &ThreadForkResponse,
    config: &Config,
) -> Result<ThreadSessionState, String> {
    thread_session_state_from_thread_response(
        &response.thread.id,
        response.thread.forked_from_id.clone(),
        response.thread.name.clone(),
        Some(response.thread.mode),
        response.thread.path.clone(),
        response.model.clone(),
        response.model_provider.clone(),
        response.service_tier,
        response.approval_policy.to_core(),
        response.approvals_reviewer.to_core(),
        response.sandbox.to_core(),
        response.cwd.clone(),
        response.reasoning_effort,
        config,
    )
    .await
}

fn review_target_to_app_server(
    target: CoreReviewTarget,
) -> codex_app_server_protocol::ReviewTarget {
    match target {
        CoreReviewTarget::UncommittedChanges => {
            codex_app_server_protocol::ReviewTarget::UncommittedChanges
        }
        CoreReviewTarget::BaseBranch { branch } => {
            codex_app_server_protocol::ReviewTarget::BaseBranch { branch }
        }
        CoreReviewTarget::Commit { sha, title } => {
            codex_app_server_protocol::ReviewTarget::Commit { sha, title }
        }
        CoreReviewTarget::Custom { instructions } => {
            codex_app_server_protocol::ReviewTarget::Custom { instructions }
        }
    }
}

#[expect(
    clippy::too_many_arguments,
    reason = "session mapping keeps explicit fields"
)]
async fn thread_session_state_from_thread_response(
    thread_id: &str,
    forked_from_id: Option<String>,
    thread_name: Option<String>,
    thread_mode: Option<ThreadMode>,
    rollout_path: Option<PathBuf>,
    model: String,
    model_provider_id: String,
    service_tier: Option<codex_protocol::config_types::ServiceTier>,
    approval_policy: AskForApproval,
    approvals_reviewer: codex_protocol::config_types::ApprovalsReviewer,
    sandbox_policy: SandboxPolicy,
    cwd: PathBuf,
    reasoning_effort: Option<codex_protocol::openai_models::ReasoningEffort>,
    config: &Config,
) -> Result<ThreadSessionState, String> {
    let thread_id = ThreadId::from_string(thread_id)
        .map_err(|err| format!("thread id `{thread_id}` is invalid: {err}"))?;
    let forked_from_id = forked_from_id
        .as_deref()
        .map(ThreadId::from_string)
        .transpose()
        .map_err(|err| format!("forked_from_id is invalid: {err}"))?;
    let (history_log_id, history_entry_count) = message_history::history_metadata(config).await;
    let history_entry_count = u64::try_from(history_entry_count).unwrap_or(u64::MAX);

    Ok(ThreadSessionState {
        thread_id,
        forked_from_id,
        thread_name,
        thread_mode,
        model,
        model_provider_id,
        service_tier,
        approval_policy,
        approvals_reviewer,
        sandbox_policy,
        cwd,
        reasoning_effort,
        history_log_id,
        history_entry_count,
        network_proxy: None,
        rollout_path,
    })
}

pub(crate) fn app_server_rate_limit_snapshots_to_core(
    response: GetAccountRateLimitsResponse,
) -> Vec<RateLimitSnapshot> {
    let mut snapshots = Vec::new();
    snapshots.push(app_server_rate_limit_snapshot_to_core(response.rate_limits));
    if let Some(by_limit_id) = response.rate_limits_by_limit_id {
        snapshots.extend(
            by_limit_id
                .into_values()
                .map(app_server_rate_limit_snapshot_to_core),
        );
    }
    snapshots
}

pub(crate) fn app_server_rate_limit_snapshot_to_core(
    snapshot: codex_app_server_protocol::RateLimitSnapshot,
) -> RateLimitSnapshot {
    RateLimitSnapshot {
        limit_id: snapshot.limit_id,
        limit_name: snapshot.limit_name,
        primary: snapshot.primary.map(app_server_rate_limit_window_to_core),
        secondary: snapshot.secondary.map(app_server_rate_limit_window_to_core),
        credits: snapshot.credits.map(app_server_credits_snapshot_to_core),
        plan_type: snapshot.plan_type,
    }
}

fn app_server_rate_limit_window_to_core(
    window: codex_app_server_protocol::RateLimitWindow,
) -> RateLimitWindow {
    RateLimitWindow {
        used_percent: window.used_percent as f64,
        window_minutes: window.window_duration_mins,
        resets_at: window.resets_at,
    }
}

fn app_server_credits_snapshot_to_core(
    snapshot: codex_app_server_protocol::CreditsSnapshot,
) -> CreditsSnapshot {
    CreditsSnapshot {
        has_credits: snapshot.has_credits,
        unlimited: snapshot.unlimited,
        balance: snapshot.balance,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use codex_app_server_protocol::ApprovalsReviewer;
    use codex_app_server_protocol::ClientRequest;
    use codex_app_server_protocol::ReadOnlyAccess;
    use codex_app_server_protocol::SessionSource;
    use codex_app_server_protocol::Thread;
    use codex_app_server_protocol::ThreadListParams;
    use codex_app_server_protocol::ThreadLoadedListParams;
    use codex_app_server_protocol::ThreadLoadedReadParams;
    use codex_app_server_protocol::ThreadMode;
    use codex_app_server_protocol::ThreadStartParams;
    use codex_app_server_protocol::ThreadStartResponse;
    use codex_app_server_protocol::ThreadStatus;
    use codex_app_server_protocol::Turn;
    use codex_app_server_protocol::TurnStatus;
    use codex_core::config::ConfigBuilder;
    use codex_protocol::protocol::SessionMeta;
    use codex_protocol::protocol::SessionMetaLine;
    use codex_protocol::protocol::SessionSource as ProtocolSessionSource;
    use pretty_assertions::assert_eq;
    use std::path::Path;
    use tempfile::TempDir;

    async fn build_config(temp_dir: &TempDir) -> Config {
        ConfigBuilder::default()
            .codex_home(temp_dir.path().to_path_buf())
            .build()
            .await
            .expect("config should build")
    }

    fn test_thread(thread_id: ThreadId, mode: ThreadMode, resident: bool) -> Thread {
        Thread {
            id: thread_id.to_string(),
            forked_from_id: None,
            preview: "hello".to_string(),
            ephemeral: false,
            model_provider: "openai".to_string(),
            created_at: 1,
            updated_at: 2,
            status: ThreadStatus::Idle,
            mode,
            resident,
            path: None,
            cwd: PathBuf::from("/tmp/project"),
            cli_version: "0.0.0".to_string(),
            source: SessionSource::Cli,
            agent_nickname: None,
            agent_role: None,
            git_info: None,
            name: None,
            turns: Vec::new(),
        }
    }

    fn test_thread_response_fields() -> (
        String,
        String,
        Option<codex_protocol::config_types::ServiceTier>,
        PathBuf,
        codex_app_server_protocol::AskForApproval,
        ApprovalsReviewer,
        codex_app_server_protocol::SandboxPolicy,
    ) {
        (
            "gpt-5.4".to_string(),
            "openai".to_string(),
            None,
            PathBuf::from("/tmp/project"),
            codex_app_server_protocol::AskForApproval::Never,
            ApprovalsReviewer::User,
            codex_app_server_protocol::SandboxPolicy::ReadOnly {
                access: ReadOnlyAccess::FullAccess,
                network_access: false,
            },
        )
    }

    fn write_minimal_rollout(path: &Path, thread_id: &str, model_provider: &str) {
        let conversation_id = ThreadId::from_string(thread_id).expect("valid thread id");
        let timestamp = "2025-01-05T12:00:00Z";
        let meta = SessionMeta {
            id: conversation_id,
            forked_from_id: None,
            timestamp: timestamp.to_string(),
            cwd: std::env::temp_dir(),
            originator: "codex".to_string(),
            cli_version: "0.0.0-test".to_string(),
            source: ProtocolSessionSource::Cli,
            agent_path: None,
            agent_nickname: None,
            agent_role: None,
            model_provider: Some(model_provider.to_string()),
            base_instructions: None,
            dynamic_tools: None,
            memory_mode: None,
        };
        let payload = serde_json::to_value(SessionMetaLine { meta, git: None })
            .expect("session meta payload should serialize");
        let lines = [
            serde_json::json!({
                "timestamp": timestamp,
                "type": "session_meta",
                "payload": payload,
            })
            .to_string(),
            serde_json::json!({
                "timestamp": timestamp,
                "type": "response_item",
                "payload": {
                    "type": "message",
                    "role": "user",
                    "content": [{"type": "input_text", "text": "resident resume"}]
                }
            })
            .to_string(),
            serde_json::json!({
                "timestamp": timestamp,
                "type": "event_msg",
                "payload": {
                    "type": "user_message",
                    "message": "resident resume",
                    "kind": "plain"
                }
            })
            .to_string(),
        ];

        std::fs::create_dir_all(path.parent().expect("rollout path should have parent"))
            .expect("rollout directory should exist");
        std::fs::write(path, lines.join("\n") + "\n").expect("rollout should be writable");
    }

    #[tokio::test]
    async fn thread_start_params_include_cwd_for_embedded_sessions() {
        let temp_dir = tempfile::tempdir().expect("tempdir");
        let config = build_config(&temp_dir).await;

        let params = thread_start_params_from_config(
            &config,
            ThreadParamsMode::Embedded,
            /*remote_cwd_override*/ None,
        );

        assert_eq!(params.cwd, Some(config.cwd.to_string_lossy().to_string()));
        assert_eq!(params.model_provider, Some(config.model_provider_id));
    }

    #[tokio::test]
    async fn thread_lifecycle_params_omit_cwd_without_remote_override_for_remote_sessions() {
        let temp_dir = tempfile::tempdir().expect("tempdir");
        let config = build_config(&temp_dir).await;
        let thread_id = ThreadId::new();

        let start = thread_start_params_from_config(
            &config,
            ThreadParamsMode::Remote,
            /*remote_cwd_override*/ None,
        );
        let resume = thread_resume_params_from_config(
            config.clone(),
            thread_id,
            ThreadParamsMode::Remote,
            /*remote_cwd_override*/ None,
        );
        let fork = thread_fork_params_from_config(
            config,
            thread_id,
            ThreadParamsMode::Remote,
            /*remote_cwd_override*/ None,
        );

        assert_eq!(start.cwd, None);
        assert_eq!(resume.cwd, None);
        assert_eq!(fork.cwd, None);
        assert_eq!(start.model_provider, None);
        assert_eq!(resume.model_provider, None);
        assert_eq!(fork.model_provider, None);
    }

    #[tokio::test]
    async fn thread_lifecycle_params_forward_explicit_remote_cwd_override_for_remote_sessions() {
        let temp_dir = tempfile::tempdir().expect("tempdir");
        let config = build_config(&temp_dir).await;
        let thread_id = ThreadId::new();
        let remote_cwd = PathBuf::from("repo/on/server");

        let start = thread_start_params_from_config(
            &config,
            ThreadParamsMode::Remote,
            Some(remote_cwd.as_path()),
        );
        let resume = thread_resume_params_from_config(
            config.clone(),
            thread_id,
            ThreadParamsMode::Remote,
            Some(remote_cwd.as_path()),
        );
        let fork = thread_fork_params_from_config(
            config,
            thread_id,
            ThreadParamsMode::Remote,
            Some(remote_cwd.as_path()),
        );

        assert_eq!(start.cwd.as_deref(), Some("repo/on/server"));
        assert_eq!(resume.cwd.as_deref(), Some("repo/on/server"));
        assert_eq!(fork.cwd.as_deref(), Some("repo/on/server"));
        assert_eq!(start.model_provider, None);
        assert_eq!(resume.model_provider, None);
        assert_eq!(fork.model_provider, None);
    }

    #[tokio::test]
    async fn resume_response_restores_turns_from_thread_items() {
        let temp_dir = tempfile::tempdir().expect("tempdir");
        let config = build_config(&temp_dir).await;
        let thread_id = ThreadId::new();
        let forked_from_id = ThreadId::new();
        let response = ThreadResumeResponse {
            thread: codex_app_server_protocol::Thread {
                id: thread_id.to_string(),
                forked_from_id: Some(forked_from_id.to_string()),
                preview: "hello".to_string(),
                ephemeral: false,
                model_provider: "openai".to_string(),
                created_at: 1,
                updated_at: 2,
                status: ThreadStatus::Idle,
                mode: ThreadMode::Interactive,
                resident: false,
                path: None,
                cwd: PathBuf::from("/tmp/project"),
                cli_version: "0.0.0".to_string(),
                source: codex_protocol::protocol::SessionSource::Cli.into(),
                agent_nickname: None,
                agent_role: None,
                git_info: None,
                name: None,
                turns: vec![Turn {
                    id: "turn-1".to_string(),
                    items: vec![
                        codex_app_server_protocol::ThreadItem::UserMessage {
                            id: "user-1".to_string(),
                            content: vec![codex_app_server_protocol::UserInput::Text {
                                text: "hello from history".to_string(),
                                text_elements: Vec::new(),
                            }],
                        },
                        codex_app_server_protocol::ThreadItem::AgentMessage {
                            id: "assistant-1".to_string(),
                            text: "assistant reply".to_string(),
                            phase: None,
                            memory_citation: None,
                        },
                    ],
                    status: TurnStatus::Completed,
                    error: None,
                }],
            },
            model: "gpt-5.4".to_string(),
            model_provider: "openai".to_string(),
            service_tier: None,
            cwd: PathBuf::from("/tmp/project"),
            approval_policy: codex_protocol::protocol::AskForApproval::Never.into(),
            approvals_reviewer: codex_app_server_protocol::ApprovalsReviewer::User,
            sandbox: codex_protocol::protocol::SandboxPolicy::new_read_only_policy().into(),
            reasoning_effort: None,
        };

        let started = started_thread_from_resume_response(response.clone(), &config)
            .await
            .expect("resume response should map");
        assert_eq!(started.session.forked_from_id, Some(forked_from_id));
        assert_eq!(started.session.thread_mode, Some(ThreadMode::Interactive));
        assert_eq!(started.turns.len(), 1);
        assert_eq!(started.turns[0], response.thread.turns[0]);
    }

    #[tokio::test]
    async fn lifecycle_responses_preserve_resident_thread_mode() {
        let temp_dir = tempfile::tempdir().expect("tempdir");
        let config = build_config(&temp_dir).await;
        let thread_id = ThreadId::new();
        let (
            model,
            model_provider,
            service_tier,
            cwd,
            approval_policy,
            approvals_reviewer,
            sandbox,
        ) = test_thread_response_fields();

        let start_response = ThreadStartResponse {
            thread: test_thread(thread_id, ThreadMode::ResidentAssistant, true),
            model: model.clone(),
            model_provider: model_provider.clone(),
            service_tier,
            cwd: cwd.clone(),
            approval_policy,
            approvals_reviewer,
            sandbox: sandbox.clone(),
            reasoning_effort: None,
        };
        let started = started_thread_from_start_response(start_response, &config)
            .await
            .expect("start response should map");
        assert_eq!(
            started.session.thread_mode,
            Some(ThreadMode::ResidentAssistant)
        );

        let resume_response = ThreadResumeResponse {
            thread: test_thread(thread_id, ThreadMode::ResidentAssistant, true),
            model: model.clone(),
            model_provider: model_provider.clone(),
            service_tier,
            cwd: cwd.clone(),
            approval_policy,
            approvals_reviewer,
            sandbox: sandbox.clone(),
            reasoning_effort: None,
        };
        let resumed = started_thread_from_resume_response(resume_response, &config)
            .await
            .expect("resume response should map");
        assert_eq!(
            resumed.session.thread_mode,
            Some(ThreadMode::ResidentAssistant)
        );

        let fork_response = ThreadForkResponse {
            thread: test_thread(thread_id, ThreadMode::ResidentAssistant, true),
            model,
            model_provider,
            service_tier,
            cwd,
            approval_policy,
            approvals_reviewer,
            sandbox,
            reasoning_effort: None,
        };
        let forked = started_thread_from_fork_response(fork_response, &config)
            .await
            .expect("fork response should map");
        assert_eq!(
            forked.session.thread_mode,
            Some(ThreadMode::ResidentAssistant)
        );
    }

    #[tokio::test]
    async fn read_and_list_preserve_resident_thread_mode() {
        let temp_dir = tempfile::tempdir().expect("tempdir");
        let config = build_config(&temp_dir).await;
        let thread_id = ThreadId::new();
        let rollout_path = temp_dir
            .path()
            .join("sessions/2025/02/04")
            .join(format!("rollout-2025-02-04T10-00-00-{thread_id}.jsonl"));
        let rollout_dir = rollout_path.parent().expect("rollout parent");
        std::fs::create_dir_all(rollout_dir).expect("rollout dir should exist");
        std::fs::write(&rollout_path, "").expect("empty rollout should write");

        let state_runtime = codex_state::StateRuntime::init(
            config.codex_home.clone(),
            config.model_provider_id.clone(),
        )
        .await
        .expect("state runtime should init");
        state_runtime
            .mark_backfill_complete(/*last_watermark*/ None)
            .await
            .expect("backfill marker should persist");

        let session_cwd = temp_dir.path().join("project");
        std::fs::create_dir_all(&session_cwd).expect("session cwd should exist");
        let created_at = chrono::DateTime::parse_from_rfc3339("2025-02-04T10:00:00Z")
            .expect("timestamp should parse")
            .with_timezone(&chrono::Utc);
        let mut builder = codex_state::ThreadMetadataBuilder::new(
            thread_id,
            rollout_path.clone(),
            created_at,
            codex_protocol::protocol::SessionSource::Cli,
        );
        builder.cwd = session_cwd;
        let mut metadata = builder.build(config.model_provider_id.as_str());
        metadata.title = "Resident thread in typed reads".to_string();
        metadata.first_user_message = Some("preview text".to_string());
        metadata.mode = "residentAssistant".to_string();
        state_runtime
            .upsert_thread(&metadata)
            .await
            .expect("thread metadata should persist");

        let mut app_server = crate::start_embedded_app_server_for_picker(&config)
            .await
            .expect("embedded app server should start");

        let thread = app_server
            .thread_read(thread_id, /*include_turns*/ false)
            .await
            .expect("thread/read should succeed");
        assert_eq!(thread.mode, ThreadMode::ResidentAssistant);

        let listed = app_server
            .thread_list(ThreadListParams {
                cursor: None,
                limit: Some(20),
                sort_key: None,
                model_providers: None,
                source_kinds: None,
                archived: Some(false),
                cwd: None,
                search_term: None,
            })
            .await
            .expect("thread/list should succeed");
        let listed_thread = listed
            .data
            .into_iter()
            .find(|thread| thread.id == thread_id.to_string())
            .expect("thread/list should include stored thread");
        assert_eq!(listed_thread.mode, ThreadMode::ResidentAssistant);

        app_server
            .shutdown()
            .await
            .expect("shutdown should succeed");
    }

    #[tokio::test]
    async fn archived_read_and_list_preserve_resident_thread_mode() {
        let temp_dir = tempfile::tempdir().expect("tempdir");
        let config = build_config(&temp_dir).await;
        let thread_id = ThreadId::new();
        let rollout_path = temp_dir
            .path()
            .join(codex_core::ARCHIVED_SESSIONS_SUBDIR)
            .join(format!("rollout-2025-02-05T10-00-00-{thread_id}.jsonl"));
        let rollout_dir = rollout_path.parent().expect("rollout parent");
        std::fs::create_dir_all(rollout_dir).expect("rollout dir should exist");
        std::fs::write(&rollout_path, "").expect("empty rollout should write");

        let state_runtime = codex_state::StateRuntime::init(
            config.codex_home.clone(),
            config.model_provider_id.clone(),
        )
        .await
        .expect("state runtime should init");
        state_runtime
            .mark_backfill_complete(/*last_watermark*/ None)
            .await
            .expect("backfill marker should persist");

        let session_cwd = temp_dir.path().join("project");
        std::fs::create_dir_all(&session_cwd).expect("session cwd should exist");
        let created_at = chrono::DateTime::parse_from_rfc3339("2025-02-05T10:00:00Z")
            .expect("timestamp should parse")
            .with_timezone(&chrono::Utc);
        let mut builder = codex_state::ThreadMetadataBuilder::new(
            thread_id,
            rollout_path.clone(),
            created_at,
            codex_protocol::protocol::SessionSource::Cli,
        );
        builder.cwd = session_cwd;
        let mut metadata = builder.build(config.model_provider_id.as_str());
        metadata.title = "Archived resident thread in typed reads".to_string();
        metadata.first_user_message = Some("preview text".to_string());
        metadata.mode = "residentAssistant".to_string();
        state_runtime
            .upsert_thread(&metadata)
            .await
            .expect("thread metadata should persist");
        state_runtime
            .mark_archived(thread_id, rollout_path.as_path(), chrono::Utc::now())
            .await
            .expect("thread should be marked archived");

        let mut app_server = crate::start_embedded_app_server_for_picker(&config)
            .await
            .expect("embedded app server should start");

        let thread = app_server
            .thread_read(thread_id, /*include_turns*/ false)
            .await
            .expect("thread/read should succeed");
        assert_eq!(thread.mode, ThreadMode::ResidentAssistant);

        let listed = app_server
            .thread_list(ThreadListParams {
                cursor: None,
                limit: Some(20),
                sort_key: None,
                model_providers: None,
                source_kinds: None,
                archived: Some(true),
                cwd: None,
                search_term: None,
            })
            .await
            .expect("thread/list should succeed");
        let listed_thread = listed
            .data
            .into_iter()
            .find(|thread| thread.id == thread_id.to_string())
            .expect("thread/list archived=true should include stored thread");
        assert_eq!(listed_thread.mode, ThreadMode::ResidentAssistant);

        app_server
            .shutdown()
            .await
            .expect("shutdown should succeed");
    }

    #[tokio::test]
    async fn loaded_read_preserves_resident_thread_mode() {
        let temp_dir = tempfile::tempdir().expect("tempdir");
        let config = build_config(&temp_dir).await;
        let mut app_server = crate::start_embedded_app_server_for_picker(&config)
            .await
            .expect("embedded app server should start");
        let request_id = app_server.next_request_id();

        let response: ThreadStartResponse = app_server
            .client
            .request_typed(ClientRequest::ThreadStart {
                request_id,
                params: ThreadStartParams {
                    resident: true,
                    ..ThreadStartParams::default()
                },
            })
            .await
            .expect("resident thread/start should succeed");
        assert_eq!(response.thread.mode, ThreadMode::ResidentAssistant);

        let loaded = app_server
            .thread_loaded_read(ThreadLoadedReadParams {
                cursor: None,
                limit: Some(20),
                model_providers: None,
                source_kinds: None,
                cwd: None,
            })
            .await
            .expect("thread/loaded/read should succeed");
        let loaded_thread = loaded
            .data
            .into_iter()
            .find(|thread| thread.id == response.thread.id)
            .expect("thread/loaded/read should include resident thread");
        assert_eq!(loaded_thread.mode, ThreadMode::ResidentAssistant);

        app_server
            .shutdown()
            .await
            .expect("shutdown should succeed");
    }

    #[tokio::test]
    async fn loaded_list_returns_loaded_thread_ids() {
        let temp_dir = tempfile::tempdir().expect("tempdir");
        let config = build_config(&temp_dir).await;
        let mut app_server = crate::start_embedded_app_server_for_picker(&config)
            .await
            .expect("embedded app server should start");
        let request_id = app_server.next_request_id();

        let response: ThreadStartResponse = app_server
            .client
            .request_typed(ClientRequest::ThreadStart {
                request_id,
                params: ThreadStartParams {
                    resident: true,
                    ..ThreadStartParams::default()
                },
            })
            .await
            .expect("resident thread/start should succeed");

        let loaded = app_server
            .thread_loaded_list(ThreadLoadedListParams {
                cursor: None,
                limit: Some(20),
                model_providers: None,
                source_kinds: None,
                cwd: None,
            })
            .await
            .expect("thread/loaded/list should succeed");
        assert!(loaded.data.contains(&response.thread.id));
        assert_eq!(loaded.next_cursor, None);

        app_server
            .shutdown()
            .await
            .expect("shutdown should succeed");
    }

    #[tokio::test]
    async fn loaded_list_preserves_next_cursor_across_pages() {
        let temp_dir = tempfile::tempdir().expect("tempdir");
        let config = build_config(&temp_dir).await;
        let mut app_server = crate::start_embedded_app_server_for_picker(&config)
            .await
            .expect("embedded app server should start");

        let first_request_id = app_server.next_request_id();
        let first_started: ThreadStartResponse = app_server
            .client
            .request_typed(ClientRequest::ThreadStart {
                request_id: first_request_id,
                params: ThreadStartParams {
                    resident: true,
                    ..ThreadStartParams::default()
                },
            })
            .await
            .expect("first resident thread/start should succeed");

        let second_request_id = app_server.next_request_id();
        let second_started: ThreadStartResponse = app_server
            .client
            .request_typed(ClientRequest::ThreadStart {
                request_id: second_request_id,
                params: ThreadStartParams {
                    resident: true,
                    ..ThreadStartParams::default()
                },
            })
            .await
            .expect("second resident thread/start should succeed");

        let first_page = app_server
            .thread_loaded_list(ThreadLoadedListParams {
                cursor: None,
                limit: Some(1),
                model_providers: None,
                source_kinds: None,
                cwd: None,
            })
            .await
            .expect("first thread/loaded/list page should succeed");
        assert_eq!(first_page.data.len(), 1);
        let next_cursor = first_page
            .next_cursor
            .clone()
            .expect("first thread/loaded/list page should include next_cursor");

        let second_page = app_server
            .thread_loaded_list(ThreadLoadedListParams {
                cursor: Some(next_cursor),
                limit: Some(1),
                model_providers: None,
                source_kinds: None,
                cwd: None,
            })
            .await
            .expect("second thread/loaded/list page should succeed");
        assert_eq!(second_page.data.len(), 1);

        let page_ids = [first_page.data[0].clone(), second_page.data[0].clone()];
        assert!(page_ids.contains(&first_started.thread.id));
        assert!(page_ids.contains(&second_started.thread.id));

        app_server
            .shutdown()
            .await
            .expect("shutdown should succeed");
    }

    #[tokio::test]
    async fn resume_thread_preserves_resident_thread_mode() {
        let temp_dir = tempfile::tempdir().expect("tempdir");
        let config = build_config(&temp_dir).await;
        let mut app_server = crate::start_embedded_app_server_for_picker(&config)
            .await
            .expect("embedded app server should start");
        let request_id = app_server.next_request_id();

        let response: ThreadStartResponse = app_server
            .client
            .request_typed(ClientRequest::ThreadStart {
                request_id,
                params: ThreadStartParams {
                    resident: true,
                    ..ThreadStartParams::default()
                },
            })
            .await
            .expect("resident thread/start should succeed");
        let thread_id = ThreadId::from_string(&response.thread.id)
            .expect("thread/start should return thread id");
        assert_eq!(response.thread.mode, ThreadMode::ResidentAssistant);
        let rollout_path = response
            .thread
            .path
            .clone()
            .expect("resident thread/start should expose rollout path");
        write_minimal_rollout(
            rollout_path.as_path(),
            &response.thread.id,
            &response.thread.model_provider,
        );

        let resumed = app_server
            .resume_thread(config.clone(), thread_id)
            .await
            .expect("thread/resume should succeed");
        assert_eq!(resumed.session.thread_id, thread_id);
        assert_eq!(
            resumed.session.thread_mode,
            Some(ThreadMode::ResidentAssistant)
        );

        app_server
            .shutdown()
            .await
            .expect("shutdown should succeed");
    }

    #[tokio::test]
    async fn session_configured_populates_history_metadata() {
        let temp_dir = tempfile::tempdir().expect("tempdir");
        let config = build_config(&temp_dir).await;
        let thread_id = ThreadId::new();

        message_history::append_entry("older", &thread_id, &config)
            .await
            .expect("history append should succeed");
        message_history::append_entry("newer", &thread_id, &config)
            .await
            .expect("history append should succeed");

        let session = thread_session_state_from_thread_response(
            &thread_id.to_string(),
            /*forked_from_id*/ None,
            Some("restore".to_string()),
            /*thread_mode*/ None,
            /*rollout_path*/ None,
            "gpt-5.4".to_string(),
            "openai".to_string(),
            /*service_tier*/ None,
            AskForApproval::Never,
            codex_protocol::config_types::ApprovalsReviewer::User,
            SandboxPolicy::new_read_only_policy(),
            PathBuf::from("/tmp/project"),
            /*reasoning_effort*/ None,
            &config,
        )
        .await
        .expect("session should map");

        assert_ne!(session.history_log_id, 0);
        assert_eq!(session.history_entry_count, 2);
        assert_eq!(session.thread_mode, None);
    }

    #[tokio::test]
    async fn session_configured_preserves_fork_source_thread_id() {
        let temp_dir = tempfile::tempdir().expect("tempdir");
        let config = build_config(&temp_dir).await;
        let thread_id = ThreadId::new();
        let forked_from_id = ThreadId::new();

        let session = thread_session_state_from_thread_response(
            &thread_id.to_string(),
            Some(forked_from_id.to_string()),
            Some("restore".to_string()),
            /*thread_mode*/ None,
            /*rollout_path*/ None,
            "gpt-5.4".to_string(),
            "openai".to_string(),
            /*service_tier*/ None,
            AskForApproval::Never,
            codex_protocol::config_types::ApprovalsReviewer::User,
            SandboxPolicy::new_read_only_policy(),
            PathBuf::from("/tmp/project"),
            /*reasoning_effort*/ None,
            &config,
        )
        .await
        .expect("session should map");

        assert_eq!(session.forked_from_id, Some(forked_from_id));
        assert_eq!(session.thread_mode, None);
    }

    #[test]
    fn status_account_display_from_auth_mode_uses_remapped_plan_labels() {
        let business = status_account_display_from_auth_mode(
            Some(AuthMode::Chatgpt),
            Some(codex_protocol::account::PlanType::EnterpriseCbpUsageBased),
        );
        assert!(matches!(
            business,
            Some(StatusAccountDisplay::ChatGpt {
                email: None,
                plan: Some(ref plan),
            }) if plan == "Enterprise"
        ));

        let team = status_account_display_from_auth_mode(
            Some(AuthMode::Chatgpt),
            Some(codex_protocol::account::PlanType::SelfServeBusinessUsageBased),
        );
        assert!(matches!(
            team,
            Some(StatusAccountDisplay::ChatGpt {
                email: None,
                plan: Some(ref plan),
            }) if plan == "Business"
        ));
    }
}
