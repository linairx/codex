use crate::auth::GatewayAuth;
use codex_app_server_client::DEFAULT_IN_PROCESS_CHANNEL_CAPACITY;
use codex_protocol::protocol::SessionSource;
use std::net::Ipv4Addr;
use std::net::SocketAddr;
use std::net::SocketAddrV4;
use std::time::Duration;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GatewayConfig {
    pub bind_address: SocketAddr,
    pub session_source: SessionSource,
    pub client_name: String,
    pub client_version: String,
    pub exec_server_url: Option<String>,
    pub runtime_mode: GatewayRuntimeMode,
    pub auth: GatewayAuth,
    pub audit_logs_enabled: bool,
    pub enable_codex_api_key_env: bool,
    pub experimental_api: bool,
    pub channel_capacity: usize,
    pub event_buffer_capacity: usize,
    pub request_rate_limit_per_minute: Option<u32>,
    pub turn_start_quota_per_minute: Option<u32>,
    pub v2_initialize_timeout: Duration,
    pub v2_client_send_timeout: Duration,
    pub v2_reconnect_retry_backoff: Duration,
    pub v2_max_pending_server_requests: usize,
    pub remote_runtime: Option<GatewayRemoteRuntimeConfig>,
}

impl Default for GatewayConfig {
    fn default() -> Self {
        Self {
            bind_address: SocketAddr::V4(SocketAddrV4::new(Ipv4Addr::LOCALHOST, 8080)),
            session_source: SessionSource::Custom("gateway".to_string()),
            client_name: "codex-gateway".to_string(),
            client_version: env!("CARGO_PKG_VERSION").to_string(),
            exec_server_url: None,
            runtime_mode: GatewayRuntimeMode::Embedded,
            auth: GatewayAuth::Disabled,
            audit_logs_enabled: true,
            enable_codex_api_key_env: true,
            experimental_api: true,
            channel_capacity: DEFAULT_IN_PROCESS_CHANNEL_CAPACITY,
            event_buffer_capacity: DEFAULT_IN_PROCESS_CHANNEL_CAPACITY,
            request_rate_limit_per_minute: None,
            turn_start_quota_per_minute: None,
            v2_initialize_timeout: Duration::from_secs(30),
            v2_client_send_timeout: Duration::from_secs(10),
            v2_reconnect_retry_backoff: Duration::from_secs(1),
            v2_max_pending_server_requests: 64,
            remote_runtime: None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum GatewayRuntimeMode {
    Embedded,
    Remote,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GatewayRemoteRuntimeConfig {
    pub selection_policy: GatewayRemoteSelectionPolicy,
    pub workers: Vec<GatewayRemoteWorkerConfig>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum GatewayRemoteSelectionPolicy {
    #[default]
    RoundRobin,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GatewayRemoteWorkerConfig {
    pub websocket_url: String,
    pub auth_token: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::GatewayConfig;
    use super::GatewayRemoteRuntimeConfig;
    use super::GatewayRemoteSelectionPolicy;
    use super::GatewayRemoteWorkerConfig;
    use super::GatewayRuntimeMode;
    use crate::auth::GatewayAuth;
    use codex_app_server_client::DEFAULT_IN_PROCESS_CHANNEL_CAPACITY;
    use codex_protocol::protocol::SessionSource;
    use pretty_assertions::assert_eq;
    use std::net::Ipv4Addr;
    use std::time::Duration;

    #[test]
    fn default_config_targets_local_embedded_runtime() {
        let config = GatewayConfig::default();

        assert_eq!(
            config.bind_address.ip().to_string(),
            Ipv4Addr::LOCALHOST.to_string()
        );
        assert_eq!(config.bind_address.port(), 8080);
        assert_eq!(
            config.session_source,
            SessionSource::Custom("gateway".to_string())
        );
        assert_eq!(config.client_name, "codex-gateway");
        assert_eq!(config.exec_server_url, None);
        assert_eq!(config.runtime_mode, GatewayRuntimeMode::Embedded);
        assert_eq!(config.auth, GatewayAuth::Disabled);
        assert_eq!(config.audit_logs_enabled, true);
        assert!(config.enable_codex_api_key_env);
        assert!(config.experimental_api);
        assert_eq!(
            config.event_buffer_capacity,
            DEFAULT_IN_PROCESS_CHANNEL_CAPACITY
        );
        assert_eq!(config.request_rate_limit_per_minute, None);
        assert_eq!(config.turn_start_quota_per_minute, None);
        assert_eq!(config.v2_initialize_timeout, Duration::from_secs(30));
        assert_eq!(config.v2_client_send_timeout, Duration::from_secs(10));
        assert_eq!(config.v2_reconnect_retry_backoff, Duration::from_secs(1));
        assert_eq!(config.v2_max_pending_server_requests, 64);
        assert_eq!(config.remote_runtime, None);
    }

    #[test]
    fn remote_runtime_config_is_cloneable() {
        let remote = GatewayRemoteRuntimeConfig {
            selection_policy: GatewayRemoteSelectionPolicy::RoundRobin,
            workers: vec![GatewayRemoteWorkerConfig {
                websocket_url: "ws://127.0.0.1:8081".to_string(),
                auth_token: Some("secret".to_string()),
            }],
        };

        assert_eq!(remote.clone(), remote);
    }
}
