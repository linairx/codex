use codex_app_server_client::AppServerRequestHandle;
use codex_app_server_client::RemoteAppServerClient;
use codex_app_server_client::RemoteAppServerConnectArgs;
use std::io;
use std::sync::Arc;
use std::sync::RwLock;
use std::sync::RwLockReadGuard;
use std::sync::RwLockWriteGuard;

#[derive(Clone)]
pub(crate) struct GatewayRemoteWorker {
    id: usize,
    connect_args: RemoteAppServerConnectArgs,
    request_handle: Arc<RwLock<AppServerRequestHandle>>,
}

impl GatewayRemoteWorker {
    pub(crate) async fn connect(
        id: usize,
        connect_args: RemoteAppServerConnectArgs,
    ) -> io::Result<(Self, RemoteAppServerClient)> {
        let client = RemoteAppServerClient::connect(connect_args.clone()).await?;
        let request_handle = Arc::new(RwLock::new(AppServerRequestHandle::Remote(
            client.request_handle(),
        )));
        Ok((
            Self {
                id,
                connect_args,
                request_handle,
            },
            client,
        ))
    }

    pub(crate) fn id(&self) -> usize {
        self.id
    }

    pub(crate) fn request_handle(&self) -> AppServerRequestHandle {
        read_guard(&self.request_handle).clone()
    }

    pub(crate) fn websocket_url(&self) -> &str {
        &self.connect_args.websocket_url
    }

    pub(crate) async fn reconnect(&self) -> io::Result<RemoteAppServerClient> {
        let client = RemoteAppServerClient::connect(self.connect_args.clone()).await?;
        *write_guard(&self.request_handle) =
            AppServerRequestHandle::Remote(client.request_handle());
        Ok(client)
    }
}

fn read_guard<T>(lock: &RwLock<T>) -> RwLockReadGuard<'_, T> {
    match lock.read() {
        Ok(guard) => guard,
        Err(poisoned) => poisoned.into_inner(),
    }
}

fn write_guard<T>(lock: &RwLock<T>) -> RwLockWriteGuard<'_, T> {
    match lock.write() {
        Ok(guard) => guard,
        Err(poisoned) => poisoned.into_inner(),
    }
}
