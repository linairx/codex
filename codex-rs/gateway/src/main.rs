use clap::Parser;
use clap::ValueEnum;
use codex_arg0::Arg0DispatchPaths;
use codex_arg0::arg0_dispatch_or_else;
use codex_core::config::Config;
use codex_core::config_loader::LoaderOverrides;
use codex_gateway::config::GatewayConfig;
use codex_gateway::config::GatewayRemoteRuntimeConfig;
use codex_gateway::config::GatewayRemoteSelectionPolicy;
use codex_gateway::config::GatewayRemoteWorkerConfig;
use codex_gateway::config::GatewayRuntimeMode;
use codex_gateway::embedded::start_gateway_server;
use codex_protocol::protocol::SessionSource;
use codex_utils_cli::CliConfigOverrides;

#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
enum RuntimeModeArg {
    Embedded,
    Remote,
}

impl From<RuntimeModeArg> for GatewayRuntimeMode {
    fn from(value: RuntimeModeArg) -> Self {
        match value {
            RuntimeModeArg::Embedded => Self::Embedded,
            RuntimeModeArg::Remote => Self::Remote,
        }
    }
}

#[derive(Debug, Parser)]
struct GatewayCli {
    #[arg(long = "listen", default_value = "127.0.0.1:8080", value_name = "ADDR")]
    listen: std::net::SocketAddr,

    #[arg(
        long = "session-source",
        value_name = "SOURCE",
        default_value = "gateway",
        value_parser = SessionSource::from_startup_arg
    )]
    session_source: SessionSource,

    #[arg(long = "client-name", default_value = "codex-gateway")]
    client_name: String,

    #[arg(long = "client-version", default_value = env!("CARGO_PKG_VERSION"))]
    client_version: String,

    #[arg(long = "exec-server-url", value_name = "URL")]
    exec_server_url: Option<String>,

    #[arg(long = "runtime", value_enum, default_value_t = RuntimeModeArg::Embedded)]
    runtime: RuntimeModeArg,

    #[arg(long = "bearer-token", value_name = "TOKEN")]
    bearer_token: Option<String>,

    #[arg(long = "no-audit-logs", default_value_t = false)]
    no_audit_logs: bool,

    #[arg(long = "requests-per-minute", value_name = "COUNT")]
    requests_per_minute: Option<u32>,

    #[arg(long = "turn-starts-per-minute", value_name = "COUNT")]
    turn_starts_per_minute: Option<u32>,

    #[arg(long = "v2-initialize-timeout-seconds", value_name = "SECONDS")]
    v2_initialize_timeout_seconds: Option<u64>,

    #[arg(long = "v2-client-send-timeout-seconds", value_name = "SECONDS")]
    v2_client_send_timeout_seconds: Option<u64>,

    #[arg(long = "v2-max-pending-server-requests", value_name = "COUNT")]
    v2_max_pending_server_requests: Option<usize>,

    #[arg(long = "remote-websocket-url", value_name = "URL")]
    remote_websocket_url: Vec<String>,

    #[arg(long = "remote-auth-token", value_name = "TOKEN")]
    remote_auth_token: Option<String>,
}

#[derive(Debug, Parser)]
struct TopCli {
    #[clap(flatten)]
    config_overrides: CliConfigOverrides,

    #[clap(flatten)]
    inner: GatewayCli,
}

fn main() -> anyhow::Result<()> {
    arg0_dispatch_or_else(|arg0_paths: Arg0DispatchPaths| async move {
        let top_cli = TopCli::parse();
        let cli_overrides = top_cli
            .config_overrides
            .parse_overrides()
            .map_err(anyhow::Error::msg)?;
        let config = Config::load_with_cli_overrides_and_harness_overrides(
            cli_overrides.clone(),
            codex_core::config::ConfigOverrides::default(),
        )
        .await?;
        let default_gateway_config = GatewayConfig::default();
        let server = start_gateway_server(
            GatewayConfig {
                bind_address: top_cli.inner.listen,
                session_source: top_cli.inner.session_source,
                client_name: top_cli.inner.client_name,
                client_version: top_cli.inner.client_version,
                exec_server_url: top_cli.inner.exec_server_url,
                runtime_mode: top_cli.inner.runtime.into(),
                auth: top_cli
                    .inner
                    .bearer_token
                    .map(|token| codex_gateway::auth::GatewayAuth::BearerToken { token })
                    .unwrap_or_default(),
                audit_logs_enabled: !top_cli.inner.no_audit_logs,
                request_rate_limit_per_minute: top_cli.inner.requests_per_minute,
                turn_start_quota_per_minute: top_cli.inner.turn_starts_per_minute,
                v2_initialize_timeout: top_cli
                    .inner
                    .v2_initialize_timeout_seconds
                    .map(std::time::Duration::from_secs)
                    .unwrap_or(default_gateway_config.v2_initialize_timeout),
                v2_client_send_timeout: top_cli
                    .inner
                    .v2_client_send_timeout_seconds
                    .map(std::time::Duration::from_secs)
                    .unwrap_or(default_gateway_config.v2_client_send_timeout),
                v2_max_pending_server_requests: top_cli
                    .inner
                    .v2_max_pending_server_requests
                    .unwrap_or(default_gateway_config.v2_max_pending_server_requests),
                remote_runtime: (!top_cli.inner.remote_websocket_url.is_empty()).then_some(
                    GatewayRemoteRuntimeConfig {
                        selection_policy: GatewayRemoteSelectionPolicy::RoundRobin,
                        workers: top_cli
                            .inner
                            .remote_websocket_url
                            .into_iter()
                            .map(|websocket_url| GatewayRemoteWorkerConfig {
                                websocket_url,
                                auth_token: top_cli.inner.remote_auth_token.clone(),
                            })
                            .collect(),
                    },
                ),
                ..default_gateway_config
            },
            arg0_paths,
            config,
            cli_overrides,
            LoaderOverrides::default(),
        )
        .await?;

        println!("codex-gateway listening on http://{}", server.local_addr());
        tokio::signal::ctrl_c().await?;
        server.shutdown().await?;
        Ok(())
    })
}
