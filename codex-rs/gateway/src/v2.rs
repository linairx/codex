use codex_app_server_client::AppServerClient;
use codex_app_server_client::InProcessAppServerClient;
use codex_app_server_client::InProcessClientStartArgs;
use codex_app_server_client::RemoteAppServerClient;
use codex_app_server_client::RemoteAppServerConnectArgs;
use codex_app_server_protocol::InitializeParams;
use codex_app_server_protocol::InitializeResponse;
use std::io;
use std::sync::Arc;

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

    pub fn initialize_response(&self) -> InitializeResponse {
        match self {
            Self::Embedded {
                initialize_response,
                ..
            }
            | Self::RemoteSingle {
                initialize_response,
                ..
            } => initialize_response.as_ref().clone(),
        }
    }

    pub async fn connect(&self, initialize: &InitializeParams) -> io::Result<AppServerClient> {
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
                    .map(AppServerClient::InProcess)
            }
            Self::RemoteSingle { connect_args, .. } => {
                let mut connect_args = connect_args.as_ref().clone();
                apply_initialize_params(
                    &mut connect_args.client_name,
                    &mut connect_args.client_version,
                    &mut connect_args.experimental_api,
                    &mut connect_args.opt_out_notification_methods,
                    initialize,
                );
                RemoteAppServerClient::connect(connect_args)
                    .await
                    .map(AppServerClient::Remote)
            }
        }
    }
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
