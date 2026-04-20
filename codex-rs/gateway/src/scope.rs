use crate::api::GatewayServerRequestKind;
use crate::error::GatewayError;
use crate::event::GatewayEvent;
use axum::extract::FromRequestParts;
use axum::http::HeaderMap;
use axum::http::HeaderName;
use axum::http::request::Parts;
use codex_app_server_protocol::RequestId;
use std::collections::HashMap;
use std::future::ready;
use std::sync::RwLock;
use std::sync::RwLockReadGuard;
use std::sync::RwLockWriteGuard;

const DEFAULT_TENANT_ID: &str = "default";
const TENANT_HEADER: HeaderName = HeaderName::from_static("x-codex-tenant-id");
const PROJECT_HEADER: HeaderName = HeaderName::from_static("x-codex-project-id");

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct GatewayRequestContext {
    pub tenant_id: String,
    pub project_id: Option<String>,
}

impl Default for GatewayRequestContext {
    fn default() -> Self {
        Self {
            tenant_id: DEFAULT_TENANT_ID.to_string(),
            project_id: None,
        }
    }
}

impl GatewayRequestContext {
    pub fn from_headers(headers: &HeaderMap) -> Result<Self, GatewayError> {
        let tenant_id = header_value(headers, &TENANT_HEADER)?
            .filter(|value| !value.is_empty())
            .unwrap_or_else(|| DEFAULT_TENANT_ID.to_string());
        let project_id = header_value(headers, &PROJECT_HEADER)?.filter(|value| !value.is_empty());

        Ok(Self {
            tenant_id,
            project_id,
        })
    }

    pub fn forwarding_headers(&self) -> Vec<(String, String)> {
        let mut headers = vec![(TENANT_HEADER.to_string(), self.tenant_id.clone())];
        if let Some(project_id) = &self.project_id {
            headers.push((PROJECT_HEADER.to_string(), project_id.clone()));
        }
        headers
    }
}

impl<S> FromRequestParts<S> for GatewayRequestContext
where
    S: Send + Sync,
{
    type Rejection = GatewayError;

    fn from_request_parts(
        parts: &mut Parts,
        _state: &S,
    ) -> impl std::future::Future<Output = Result<Self, Self::Rejection>> + Send {
        ready(GatewayRequestContext::from_headers(&parts.headers))
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PendingServerRequest {
    pub kind: GatewayServerRequestKind,
    pub context: GatewayRequestContext,
    pub worker_id: Option<usize>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct ThreadScope {
    context: GatewayRequestContext,
    worker_id: Option<usize>,
}

#[derive(Default)]
pub struct GatewayScopeRegistry {
    thread_contexts: RwLock<HashMap<String, ThreadScope>>,
    pending_server_requests: RwLock<HashMap<RequestId, PendingServerRequest>>,
}

impl GatewayScopeRegistry {
    pub fn register_thread(&self, thread_id: String, context: GatewayRequestContext) {
        self.register_thread_with_worker(thread_id, context, None);
    }

    pub fn register_thread_with_worker(
        &self,
        thread_id: String,
        context: GatewayRequestContext,
        worker_id: Option<usize>,
    ) {
        write_guard(&self.thread_contexts).insert(thread_id, ThreadScope { context, worker_id });
    }

    pub fn thread_context(&self, thread_id: &str) -> Option<GatewayRequestContext> {
        read_guard(&self.thread_contexts)
            .get(thread_id)
            .map(|scope| scope.context.clone())
    }

    pub fn thread_worker_id(&self, thread_id: &str) -> Option<usize> {
        read_guard(&self.thread_contexts)
            .get(thread_id)
            .and_then(|scope| scope.worker_id)
    }

    pub fn register_thread_worker_if_visible(
        &self,
        thread_id: &str,
        context: &GatewayRequestContext,
        worker_id: Option<usize>,
    ) -> bool {
        let mut thread_contexts = write_guard(&self.thread_contexts);
        let Some(scope) = thread_contexts.get_mut(thread_id) else {
            return false;
        };
        if &scope.context != context {
            return false;
        }
        if worker_id.is_some() {
            scope.worker_id = worker_id;
        }
        true
    }

    pub fn thread_visible_to(&self, context: &GatewayRequestContext, thread_id: &str) -> bool {
        self.thread_context(thread_id).as_ref() == Some(context)
    }

    pub fn filter_thread_ids(
        &self,
        context: &GatewayRequestContext,
        thread_ids: impl IntoIterator<Item = String>,
    ) -> Vec<String> {
        thread_ids
            .into_iter()
            .filter(|thread_id| self.thread_visible_to(context, thread_id))
            .collect()
    }

    pub fn register_pending_server_request(
        &self,
        request_id: RequestId,
        kind: GatewayServerRequestKind,
        context: GatewayRequestContext,
    ) {
        self.register_pending_server_request_with_worker(request_id, kind, context, None);
    }

    pub fn register_pending_server_request_with_worker(
        &self,
        request_id: RequestId,
        kind: GatewayServerRequestKind,
        context: GatewayRequestContext,
        worker_id: Option<usize>,
    ) {
        write_guard(&self.pending_server_requests).insert(
            request_id,
            PendingServerRequest {
                kind,
                context,
                worker_id,
            },
        );
    }

    pub fn take_pending_server_request(
        &self,
        request_id: &RequestId,
    ) -> Option<PendingServerRequest> {
        write_guard(&self.pending_server_requests).remove(request_id)
    }

    pub fn clear_pending_server_request(&self, request_id: &RequestId) {
        write_guard(&self.pending_server_requests).remove(request_id);
    }

    pub fn clear_all_pending_server_requests(&self) {
        write_guard(&self.pending_server_requests).clear();
    }

    pub fn clear_pending_server_requests_for_worker(&self, worker_id: usize) {
        write_guard(&self.pending_server_requests)
            .retain(|_, request| request.worker_id != Some(worker_id));
    }

    pub fn event_visible_to(&self, context: &GatewayRequestContext, event: &GatewayEvent) -> bool {
        match &event.thread_id {
            Some(thread_id) => self.thread_visible_to(context, thread_id),
            None => true,
        }
    }
}

fn header_value(headers: &HeaderMap, name: &HeaderName) -> Result<Option<String>, GatewayError> {
    headers
        .get(name)
        .map(|value| {
            value.to_str().map(str::to_owned).map_err(|_| {
                GatewayError::InvalidRequest(format!(
                    "header {} must be valid UTF-8",
                    name.as_str()
                ))
            })
        })
        .transpose()
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

#[cfg(test)]
mod tests {
    use super::GatewayRequestContext;
    use super::GatewayScopeRegistry;
    use crate::api::GatewayServerRequestKind;
    use crate::event::GatewayEvent;
    use axum::http::HeaderMap;
    use codex_app_server_protocol::RequestId;
    use pretty_assertions::assert_eq;

    #[test]
    fn defaults_scope_to_default_tenant() {
        let context = GatewayRequestContext::from_headers(&HeaderMap::new()).expect("context");

        assert_eq!(
            context,
            GatewayRequestContext {
                tenant_id: "default".to_string(),
                project_id: None,
            }
        );
    }

    #[test]
    fn reads_scope_headers() {
        let mut headers = HeaderMap::new();
        headers.insert(
            "x-codex-tenant-id",
            "tenant-a".parse().expect("tenant header"),
        );
        headers.insert(
            "x-codex-project-id",
            "project-a".parse().expect("project header"),
        );

        let context = GatewayRequestContext::from_headers(&headers).expect("context");

        assert_eq!(
            context,
            GatewayRequestContext {
                tenant_id: "tenant-a".to_string(),
                project_id: Some("project-a".to_string()),
            }
        );
    }

    #[test]
    fn registry_hides_threads_and_pending_requests_across_scopes() {
        let registry = GatewayScopeRegistry::default();
        let tenant_a = GatewayRequestContext {
            tenant_id: "tenant-a".to_string(),
            project_id: Some("project-a".to_string()),
        };
        let tenant_b = GatewayRequestContext {
            tenant_id: "tenant-a".to_string(),
            project_id: Some("project-b".to_string()),
        };
        registry.register_thread("thread-a".to_string(), tenant_a.clone());
        registry.register_pending_server_request(
            RequestId::String("req-1".to_string()),
            GatewayServerRequestKind::ToolRequestUserInput,
            tenant_a.clone(),
        );

        assert_eq!(registry.thread_visible_to(&tenant_a, "thread-a"), true);
        assert_eq!(registry.thread_visible_to(&tenant_b, "thread-a"), false);
        assert_eq!(
            registry.filter_thread_ids(
                &tenant_a,
                vec!["thread-a".to_string(), "thread-b".to_string()]
            ),
            vec!["thread-a".to_string()]
        );
        assert_eq!(
            registry.event_visible_to(
                &tenant_b,
                &GatewayEvent {
                    method: "thread/updated".to_string(),
                    thread_id: Some("thread-a".to_string()),
                    data: serde_json::json!({}),
                }
            ),
            false
        );
        assert_eq!(
            registry
                .take_pending_server_request(&RequestId::String("req-1".to_string()))
                .expect("pending request")
                .context,
            tenant_a
        );
    }

    #[test]
    fn clears_pending_requests_for_specific_worker() {
        let registry = GatewayScopeRegistry::default();
        let context = GatewayRequestContext::default();
        registry.register_pending_server_request_with_worker(
            RequestId::String("req-1".to_string()),
            GatewayServerRequestKind::ToolRequestUserInput,
            context.clone(),
            Some(1),
        );
        registry.register_pending_server_request_with_worker(
            RequestId::String("req-2".to_string()),
            GatewayServerRequestKind::ToolRequestUserInput,
            context,
            Some(2),
        );

        registry.clear_pending_server_requests_for_worker(1);

        assert_eq!(
            registry.take_pending_server_request(&RequestId::String("req-1".to_string())),
            None
        );
        assert_eq!(
            registry
                .take_pending_server_request(&RequestId::String("req-2".to_string()))
                .is_some(),
            true
        );
    }

    #[test]
    fn register_thread_worker_if_visible_only_updates_matching_scope() {
        let registry = GatewayScopeRegistry::default();
        let visible_context = GatewayRequestContext {
            tenant_id: "tenant-a".to_string(),
            project_id: Some("project-a".to_string()),
        };
        let other_context = GatewayRequestContext {
            tenant_id: "tenant-a".to_string(),
            project_id: Some("project-b".to_string()),
        };
        registry.register_thread("thread-a".to_string(), visible_context.clone());

        assert_eq!(
            registry.register_thread_worker_if_visible("thread-a", &visible_context, Some(7)),
            true
        );
        assert_eq!(registry.thread_worker_id("thread-a"), Some(7));
        assert_eq!(
            registry.register_thread_worker_if_visible("thread-a", &other_context, Some(8)),
            false
        );
        assert_eq!(
            registry.register_thread_worker_if_visible("thread-missing", &visible_context, Some(9)),
            false
        );
        assert_eq!(registry.thread_worker_id("thread-a"), Some(7));
    }
}
