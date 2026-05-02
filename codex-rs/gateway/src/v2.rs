use crate::scope::GatewayRequestContext;
use codex_app_server_client::AppServerClient;
use codex_app_server_client::InProcessAppServerClient;
use codex_app_server_client::InProcessClientStartArgs;
use codex_app_server_client::RemoteAppServerClient;
use codex_app_server_client::RemoteAppServerConnectArgs;
use codex_app_server_protocol::InitializeParams;
use codex_app_server_protocol::InitializeResponse;
use codex_app_server_protocol::RequestId;
use std::io;
use std::sync::Arc;
use std::sync::atomic::AtomicU64;
use std::sync::atomic::Ordering;

#[derive(Clone)]
pub enum GatewayV2SessionFactory {
    Embedded {
        start_args: Box<InProcessClientStartArgs>,
        initialize_response: Arc<InitializeResponse>,
    },
    RemoteSingle {
        connect_args: Box<RemoteAppServerConnectArgs>,
        initialize_response: Arc<InitializeResponse>,
    },
    RemoteMulti {
        connect_args: Vec<RemoteAppServerConnectArgs>,
        initialize_response: Arc<InitializeResponse>,
        next_server_request_id: Arc<AtomicU64>,
    },
}

pub struct GatewayV2ConnectedSession {
    pub worker_id: Option<usize>,
    pub worker_websocket_url: Option<String>,
    pub app_server: AppServerClient,
}

impl GatewayV2SessionFactory {
    pub fn embedded(
        start_args: InProcessClientStartArgs,
        initialize_response: InitializeResponse,
    ) -> Self {
        Self::Embedded {
            start_args: Box::new(start_args),
            initialize_response: Arc::new(initialize_response),
        }
    }

    pub fn remote_single(
        connect_args: RemoteAppServerConnectArgs,
        initialize_response: InitializeResponse,
    ) -> Self {
        Self::RemoteSingle {
            connect_args: Box::new(connect_args),
            initialize_response: Arc::new(initialize_response),
        }
    }

    pub fn remote_multi(
        connect_args: Vec<RemoteAppServerConnectArgs>,
        initialize_response: InitializeResponse,
    ) -> Self {
        Self::RemoteMulti {
            connect_args,
            initialize_response: Arc::new(initialize_response),
            next_server_request_id: Arc::new(AtomicU64::new(1)),
        }
    }

    pub fn initialize_response(&self) -> InitializeResponse {
        match self {
            Self::Embedded {
                initialize_response,
                ..
            }
            | Self::RemoteSingle {
                initialize_response,
                ..
            }
            | Self::RemoteMulti {
                initialize_response,
                ..
            } => initialize_response.as_ref().clone(),
        }
    }

    pub async fn connect(
        &self,
        initialize: &InitializeParams,
        request_context: &GatewayRequestContext,
    ) -> io::Result<Vec<GatewayV2ConnectedSession>> {
        match self {
            Self::Embedded { start_args, .. } => {
                let mut start_args = start_args.as_ref().clone();
                apply_initialize_params(
                    &mut start_args.client_name,
                    &mut start_args.client_version,
                    &mut start_args.experimental_api,
                    &mut start_args.opt_out_notification_methods,
                    initialize,
                );
                InProcessAppServerClient::start(start_args)
                    .await
                    .map(|app_server| {
                        vec![GatewayV2ConnectedSession {
                            worker_id: None,
                            worker_websocket_url: None,
                            app_server: AppServerClient::InProcess(app_server),
                        }]
                    })
            }
            Self::RemoteSingle { .. } => self
                .connect_worker(0, initialize, request_context)
                .await
                .map(|app_server| vec![app_server]),
            Self::RemoteMulti { connect_args, .. } => {
                let mut sessions = Vec::with_capacity(connect_args.len());
                for worker_id in 0..connect_args.len() {
                    sessions.push(
                        self.connect_worker(worker_id, initialize, request_context)
                            .await?,
                    );
                }
                Ok(sessions)
            }
        }
    }

    pub async fn connect_worker(
        &self,
        worker_id: usize,
        initialize: &InitializeParams,
        request_context: &GatewayRequestContext,
    ) -> io::Result<GatewayV2ConnectedSession> {
        match self {
            Self::RemoteSingle { connect_args, .. } if worker_id == 0 => {
                connect_remote_worker(
                    connect_args.as_ref().clone(),
                    worker_id,
                    initialize,
                    request_context,
                )
                .await
            }
            Self::RemoteMulti { connect_args, .. } => {
                let connect_args = connect_args.get(worker_id).ok_or_else(|| {
                    io::Error::other(format!("gateway v2 worker {worker_id} is not configured"))
                })?;
                connect_remote_worker(connect_args.clone(), worker_id, initialize, request_context)
                    .await
            }
            _ => Err(io::Error::other(format!(
                "gateway v2 worker {worker_id} cannot be connected for this runtime"
            ))),
        }
    }

    pub fn next_server_request_id(&self) -> RequestId {
        match self {
            Self::RemoteMulti {
                next_server_request_id,
                ..
            } => RequestId::String(format!(
                "gateway-srv-{}",
                next_server_request_id.fetch_add(1, Ordering::Relaxed)
            )),
            _ => RequestId::String("gateway-srv-1".to_string()),
        }
    }
}

async fn connect_remote_worker(
    mut connect_args: RemoteAppServerConnectArgs,
    worker_id: usize,
    initialize: &InitializeParams,
    request_context: &GatewayRequestContext,
) -> io::Result<GatewayV2ConnectedSession> {
    let worker_websocket_url = connect_args.websocket_url.clone();
    apply_initialize_params(
        &mut connect_args.client_name,
        &mut connect_args.client_version,
        &mut connect_args.experimental_api,
        &mut connect_args.opt_out_notification_methods,
        initialize,
    );
    let app_server = RemoteAppServerClient::connect_with_headers(
        connect_args,
        request_context.forwarding_headers(),
    )
    .await?;
    Ok(GatewayV2ConnectedSession {
        worker_id: Some(worker_id),
        worker_websocket_url: Some(worker_websocket_url),
        app_server: AppServerClient::Remote(app_server),
    })
}

fn apply_initialize_params(
    client_name: &mut String,
    client_version: &mut String,
    experimental_api: &mut bool,
    opt_out_notification_methods: &mut Vec<String>,
    initialize: &InitializeParams,
) {
    *client_name = initialize.client_info.name.clone();
    *client_version = initialize.client_info.version.clone();

    let capabilities = initialize.capabilities.clone().unwrap_or_default();
    *experimental_api = capabilities.experimental_api;
    *opt_out_notification_methods = capabilities
        .opt_out_notification_methods
        .unwrap_or_default();
}

pub fn gateway_initialize_response(config: &codex_core::config::Config) -> InitializeResponse {
    InitializeResponse {
        user_agent: format!("codex-gateway/{}", env!("CARGO_PKG_VERSION")),
        codex_home: config.codex_home.clone(),
        platform_family: std::env::consts::FAMILY.to_string(),
        platform_os: std::env::consts::OS.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::apply_initialize_params;
    use codex_app_server_protocol::ClientInfo;
    use codex_app_server_protocol::InitializeCapabilities;
    use pretty_assertions::assert_eq;

    #[test]
    fn initialize_params_override_connection_identity_and_capabilities() {
        let mut client_name = "codex-gateway".to_string();
        let mut client_version = "0.0.0".to_string();
        let mut experimental_api = false;
        let mut opt_out_notification_methods = Vec::new();

        apply_initialize_params(
            &mut client_name,
            &mut client_version,
            &mut experimental_api,
            &mut opt_out_notification_methods,
            &codex_app_server_protocol::InitializeParams {
                client_info: ClientInfo {
                    name: "codex-tui".to_string(),
                    title: None,
                    version: "1.2.3".to_string(),
                },
                capabilities: Some(InitializeCapabilities {
                    experimental_api: true,
                    opt_out_notification_methods: Some(vec!["thread/started".to_string()]),
                }),
            },
        );

        assert_eq!(client_name, "codex-tui");
        assert_eq!(client_version, "1.2.3");
        assert_eq!(experimental_api, true);
        assert_eq!(
            opt_out_notification_methods,
            vec!["thread/started".to_string()]
        );
    }
}
