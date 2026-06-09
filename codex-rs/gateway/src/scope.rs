use crate::api::GatewayAccountCapacityStatus;
use crate::api::GatewayPendingServerRequestRouteCounts;
use crate::api::GatewayProjectWorkerRoute;
use crate::api::GatewayServerRequestKind;
use crate::config::normalize_remote_account_id;
use crate::error::GatewayError;
use crate::event::GatewayEvent;
use axum::extract::FromRequestParts;
use axum::http::HeaderMap;
use axum::http::HeaderName;
use axum::http::request::Parts;
use codex_app_server_protocol::RequestId;
use std::collections::BTreeMap;
use std::collections::HashMap;
use std::future::ready;
use std::path::Path;
use std::sync::RwLock;
use std::sync::RwLockReadGuard;
use std::sync::RwLockWriteGuard;
use std::sync::atomic::AtomicUsize;
use std::sync::atomic::Ordering;
use std::time::SystemTime;
use std::time::UNIX_EPOCH;

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
    pub thread_id: String,
    pub worker_id: Option<usize>,
    pub registered_at: i64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct ThreadScope {
    context: GatewayRequestContext,
    worker_id: Option<usize>,
}

#[derive(Default)]
pub struct GatewayScopeRegistry {
    thread_contexts: RwLock<HashMap<String, ThreadScope>>,
    thread_paths: RwLock<HashMap<String, ThreadScope>>,
    project_workers: RwLock<HashMap<GatewayRequestContext, usize>>,
    next_project_worker: AtomicUsize,
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

    pub fn register_project_worker(&self, context: GatewayRequestContext, worker_id: usize) {
        if context.project_id.is_some() {
            write_guard(&self.project_workers).insert(context, worker_id);
        }
    }

    pub fn project_worker_id(&self, context: &GatewayRequestContext) -> Option<usize> {
        context.project_id.as_ref()?;
        read_guard(&self.project_workers).get(context).copied()
    }

    pub fn project_worker_routes(
        &self,
        worker_healthy: impl Fn(usize) -> bool,
        worker_account_id: impl Fn(usize) -> Option<String>,
        worker_account_capacity: impl Fn(usize) -> GatewayAccountCapacityStatus,
    ) -> Vec<GatewayProjectWorkerRoute> {
        let normalized_worker_account_id =
            |worker_id| normalize_remote_account_id(worker_account_id(worker_id));

        let mut routes = read_guard(&self.project_workers)
            .iter()
            .filter_map(|(context, worker_id)| {
                context.project_id.as_ref().map(|project_id| {
                    let account_id = normalized_worker_account_id(*worker_id);
                    let account_capacity = worker_account_capacity(*worker_id);
                    let worker_healthy = worker_healthy(*worker_id);
                    GatewayProjectWorkerRoute {
                        tenant_id: context.tenant_id.clone(),
                        project_id: project_id.clone(),
                        worker_id: *worker_id,
                        account_routing_eligible: worker_healthy
                            && account_id.is_some()
                            && account_capacity == GatewayAccountCapacityStatus::Available,
                        account_id,
                        account_capacity,
                        worker_healthy,
                    }
                })
            })
            .collect::<Vec<_>>();
        routes.sort_by(|left, right| {
            left.tenant_id
                .cmp(&right.tenant_id)
                .then_with(|| left.project_id.cmp(&right.project_id))
                .then_with(|| left.worker_id.cmp(&right.worker_id))
        });
        routes
    }

    pub fn select_project_worker_id(
        &self,
        context: &GatewayRequestContext,
        worker_ids: impl IntoIterator<Item = usize>,
    ) -> Option<usize> {
        self.select_project_worker_id_with_accounts(context, worker_ids, |_| None)
    }

    pub fn select_project_worker_id_with_accounts(
        &self,
        context: &GatewayRequestContext,
        worker_ids: impl IntoIterator<Item = usize>,
        worker_account_id: impl Fn(usize) -> Option<String>,
    ) -> Option<usize> {
        context.project_id.as_ref()?;

        let worker_ids = worker_ids.into_iter().collect::<Vec<_>>();
        if worker_ids.is_empty() {
            return None;
        }

        if let Some(worker_id) = self.project_worker_id(context)
            && worker_ids.contains(&worker_id)
        {
            return Some(worker_id);
        }

        let normalized_worker_account_id =
            |worker_id| normalize_remote_account_id(worker_account_id(worker_id));

        let labeled_worker_ids = worker_ids
            .iter()
            .copied()
            .filter(|worker_id| normalized_worker_account_id(*worker_id).is_some())
            .collect::<Vec<_>>();
        let candidate_worker_ids = if labeled_worker_ids.is_empty() {
            worker_ids
        } else {
            labeled_worker_ids
        };

        let start = self.next_project_worker.fetch_add(1, Ordering::Relaxed);
        let route_loads = {
            let project_workers = read_guard(&self.project_workers);
            project_workers
                .iter()
                .filter(|(project_context, worker_id)| {
                    project_context.project_id.is_some()
                        && project_context.tenant_id == context.tenant_id
                        && candidate_worker_ids.contains(worker_id)
                })
                .fold(
                    HashMap::<String, usize>::new(),
                    |mut loads, (_, worker_id)| {
                        let key =
                            account_route_key(*worker_id, normalized_worker_account_id(*worker_id));
                        *loads.entry(key).or_default() += 1;
                        loads
                    },
                )
        };

        candidate_worker_ids
            .iter()
            .copied()
            .cycle()
            .skip(start % candidate_worker_ids.len())
            .take(candidate_worker_ids.len())
            .min_by_key(|worker_id| {
                let key = account_route_key(*worker_id, normalized_worker_account_id(*worker_id));
                route_loads.get(&key).copied().unwrap_or_default()
            })
    }

    pub fn register_thread_path_with_worker(
        &self,
        thread_path: impl AsRef<Path>,
        context: GatewayRequestContext,
        worker_id: Option<usize>,
    ) {
        let thread_path = thread_path.as_ref().to_string_lossy().to_string();
        write_guard(&self.thread_paths).insert(thread_path, ThreadScope { context, worker_id });
    }

    pub fn thread_path_context(
        &self,
        thread_path: impl AsRef<Path>,
    ) -> Option<GatewayRequestContext> {
        let thread_path = thread_path.as_ref().to_string_lossy().to_string();
        read_guard(&self.thread_paths)
            .get(&thread_path)
            .map(|scope| scope.context.clone())
    }

    pub fn thread_path_worker_id(&self, thread_path: impl AsRef<Path>) -> Option<usize> {
        let thread_path = thread_path.as_ref().to_string_lossy().to_string();
        read_guard(&self.thread_paths)
            .get(&thread_path)
            .and_then(|scope| scope.worker_id)
    }

    pub fn register_thread_path_worker_if_visible(
        &self,
        thread_path: impl AsRef<Path>,
        context: &GatewayRequestContext,
        worker_id: Option<usize>,
    ) -> bool {
        let thread_path = thread_path.as_ref().to_string_lossy().to_string();
        let mut thread_paths = write_guard(&self.thread_paths);
        let Some(scope) = thread_paths.get_mut(&thread_path) else {
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

    pub fn thread_path_visible_to(
        &self,
        context: &GatewayRequestContext,
        thread_path: impl AsRef<Path>,
    ) -> bool {
        self.thread_path_context(thread_path).as_ref() == Some(context)
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
        thread_id: String,
    ) {
        self.register_pending_server_request_with_worker(
            request_id, kind, context, thread_id, None,
        );
    }

    pub fn register_pending_server_request_with_worker(
        &self,
        request_id: RequestId,
        kind: GatewayServerRequestKind,
        context: GatewayRequestContext,
        thread_id: String,
        worker_id: Option<usize>,
    ) {
        let registered_at = unix_timestamp_now();
        write_guard(&self.pending_server_requests).insert(
            request_id,
            PendingServerRequest {
                kind,
                context,
                thread_id,
                worker_id,
                registered_at,
            },
        );
    }

    pub fn pending_server_request(&self, request_id: &RequestId) -> Option<PendingServerRequest> {
        read_guard(&self.pending_server_requests)
            .get(request_id)
            .cloned()
    }

    pub fn pending_server_request_count(&self) -> usize {
        read_guard(&self.pending_server_requests).len()
    }

    pub fn pending_server_request_kind_counts(&self) -> BTreeMap<String, usize> {
        read_guard(&self.pending_server_requests).values().fold(
            BTreeMap::new(),
            |mut counts, request| {
                *counts
                    .entry(request.kind.metric_tag().to_string())
                    .or_default() += 1;
                counts
            },
        )
    }

    pub fn pending_server_request_route_counts(
        &self,
    ) -> Vec<GatewayPendingServerRequestRouteCounts> {
        let grouped = read_guard(&self.pending_server_requests).values().fold(
            BTreeMap::<Option<usize>, GatewayPendingServerRequestRouteCounts>::new(),
            |mut grouped, request| {
                let entry = grouped.entry(request.worker_id).or_insert_with(|| {
                    GatewayPendingServerRequestRouteCounts {
                        worker_id: request.worker_id,
                        count: 0,
                        kind_counts: BTreeMap::new(),
                    }
                });
                entry.count += 1;
                *entry
                    .kind_counts
                    .entry(request.kind.metric_tag().to_string())
                    .or_default() += 1;
                grouped
            },
        );
        grouped.into_values().collect()
    }

    pub fn pending_server_request_oldest_at(&self) -> Option<i64> {
        read_guard(&self.pending_server_requests)
            .values()
            .map(|request| request.registered_at)
            .min()
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

fn account_route_key(worker_id: usize, account_id: Option<String>) -> String {
    account_id
        .map(|account_id| format!("account:{account_id}"))
        .unwrap_or_else(|| format!("worker:{worker_id}"))
}

fn unix_timestamp_now() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_secs() as i64)
        .unwrap_or(0)
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
            "thread-a".to_string(),
        );

        assert_eq!(registry.pending_server_request_count(), 1);
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
        assert_eq!(registry.pending_server_request_count(), 0);
    }

    #[test]
    fn clears_pending_requests_for_specific_worker() {
        let registry = GatewayScopeRegistry::default();
        let context = GatewayRequestContext::default();
        registry.register_pending_server_request_with_worker(
            RequestId::String("req-1".to_string()),
            GatewayServerRequestKind::ToolRequestUserInput,
            context.clone(),
            "thread-1".to_string(),
            Some(1),
        );
        registry.register_pending_server_request_with_worker(
            RequestId::String("req-2".to_string()),
            GatewayServerRequestKind::ToolRequestUserInput,
            context,
            "thread-2".to_string(),
            Some(2),
        );

        registry.clear_pending_server_requests_for_worker(1);

        assert_eq!(registry.pending_server_request_count(), 1);
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
    fn counts_pending_server_requests_by_kind() {
        let registry = GatewayScopeRegistry::default();
        let context = GatewayRequestContext::default();
        assert_eq!(registry.pending_server_request_oldest_at(), None);
        registry.register_pending_server_request(
            RequestId::String("req-1".to_string()),
            GatewayServerRequestKind::ToolRequestUserInput,
            context.clone(),
            "thread-1".to_string(),
        );
        registry.register_pending_server_request(
            RequestId::String("req-2".to_string()),
            GatewayServerRequestKind::ToolRequestUserInput,
            context.clone(),
            "thread-2".to_string(),
        );
        registry.register_pending_server_request(
            RequestId::String("req-3".to_string()),
            GatewayServerRequestKind::CommandExecutionApproval,
            context,
            "thread-3".to_string(),
        );

        assert_eq!(
            registry.pending_server_request_kind_counts(),
            [
                ("commandExecutionApproval".to_string(), 1),
                ("toolRequestUserInput".to_string(), 2),
            ]
            .into()
        );
        assert!(registry.pending_server_request_oldest_at().is_some());
    }

    #[test]
    fn counts_pending_server_requests_by_worker_route() {
        let registry = GatewayScopeRegistry::default();
        let context = GatewayRequestContext::default();
        registry.register_pending_server_request(
            RequestId::String("req-1".to_string()),
            GatewayServerRequestKind::ToolRequestUserInput,
            context.clone(),
            "thread-1".to_string(),
        );
        registry.register_pending_server_request_with_worker(
            RequestId::String("req-2".to_string()),
            GatewayServerRequestKind::ToolRequestUserInput,
            context.clone(),
            "thread-2".to_string(),
            Some(2),
        );
        registry.register_pending_server_request_with_worker(
            RequestId::String("req-3".to_string()),
            GatewayServerRequestKind::CommandExecutionApproval,
            context,
            "thread-3".to_string(),
            Some(2),
        );

        assert_eq!(
            registry.pending_server_request_route_counts(),
            vec![
                crate::api::GatewayPendingServerRequestRouteCounts {
                    worker_id: None,
                    count: 1,
                    kind_counts: [("toolRequestUserInput".to_string(), 1)].into(),
                },
                crate::api::GatewayPendingServerRequestRouteCounts {
                    worker_id: Some(2),
                    count: 2,
                    kind_counts: [
                        ("commandExecutionApproval".to_string(), 1),
                        ("toolRequestUserInput".to_string(), 1),
                    ]
                    .into(),
                },
            ]
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

    #[test]
    fn project_worker_routes_are_scoped_by_tenant_and_project() {
        let registry = GatewayScopeRegistry::default();
        let project_a = GatewayRequestContext {
            tenant_id: "tenant-a".to_string(),
            project_id: Some("project-a".to_string()),
        };
        let project_b = GatewayRequestContext {
            tenant_id: "tenant-a".to_string(),
            project_id: Some("project-b".to_string()),
        };
        let unscoped_project = GatewayRequestContext {
            tenant_id: "tenant-a".to_string(),
            project_id: None,
        };

        registry.register_project_worker(project_a.clone(), 7);
        registry.register_project_worker(unscoped_project.clone(), 9);

        assert_eq!(registry.project_worker_id(&project_a), Some(7));
        assert_eq!(registry.project_worker_id(&project_b), None);
        assert_eq!(registry.project_worker_id(&unscoped_project), None);
        assert_eq!(
            registry.project_worker_routes(
                |_| true,
                |_| Some("acct-a".to_string()),
                |_| crate::api::GatewayAccountCapacityStatus::Available,
            ),
            vec![crate::api::GatewayProjectWorkerRoute {
                tenant_id: "tenant-a".to_string(),
                project_id: "project-a".to_string(),
                worker_id: 7,
                account_id: Some("acct-a".to_string()),
                account_capacity: crate::api::GatewayAccountCapacityStatus::Available,
                worker_healthy: true,
                account_routing_eligible: true,
            }]
        );
    }

    #[test]
    fn project_worker_routes_replace_previous_affinity_for_same_project() {
        let registry = GatewayScopeRegistry::default();
        let project = GatewayRequestContext {
            tenant_id: "tenant-a".to_string(),
            project_id: Some("project-a".to_string()),
        };

        registry.register_project_worker(project.clone(), 7);
        registry.register_project_worker(project.clone(), 9);

        assert_eq!(registry.project_worker_id(&project), Some(9));
        assert_eq!(
            registry.project_worker_routes(
                |_| true,
                |_| Some("acct-a".to_string()),
                |_| crate::api::GatewayAccountCapacityStatus::Available,
            ),
            vec![crate::api::GatewayProjectWorkerRoute {
                tenant_id: "tenant-a".to_string(),
                project_id: "project-a".to_string(),
                worker_id: 9,
                account_id: Some("acct-a".to_string()),
                account_capacity: crate::api::GatewayAccountCapacityStatus::Available,
                worker_healthy: true,
                account_routing_eligible: true,
            }]
        );
    }

    #[test]
    fn project_worker_routes_report_account_routing_eligibility() {
        let registry = GatewayScopeRegistry::default();
        registry.register_project_worker(
            GatewayRequestContext {
                tenant_id: "tenant-a".to_string(),
                project_id: Some("project-a".to_string()),
            },
            /*worker_id*/ 1,
        );
        registry.register_project_worker(
            GatewayRequestContext {
                tenant_id: "tenant-a".to_string(),
                project_id: Some("project-b".to_string()),
            },
            /*worker_id*/ 2,
        );
        registry.register_project_worker(
            GatewayRequestContext {
                tenant_id: "tenant-a".to_string(),
                project_id: Some("project-c".to_string()),
            },
            /*worker_id*/ 3,
        );

        let routes = registry.project_worker_routes(
            |worker_id| worker_id != 2,
            |worker_id| match worker_id {
                1 | 2 => Some(format!("acct-{worker_id}")),
                3 => None,
                _ => unreachable!("unexpected worker id"),
            },
            |worker_id| match worker_id {
                1 | 2 => crate::api::GatewayAccountCapacityStatus::Available,
                3 => crate::api::GatewayAccountCapacityStatus::Exhausted,
                _ => unreachable!("unexpected worker id"),
            },
        );

        assert_eq!(
            routes
                .iter()
                .map(|route| (
                    route.project_id.as_str(),
                    route.worker_healthy,
                    route.account_id.as_deref(),
                    route.account_capacity,
                    route.account_routing_eligible,
                ))
                .collect::<Vec<_>>(),
            vec![
                (
                    "project-a",
                    true,
                    Some("acct-1"),
                    crate::api::GatewayAccountCapacityStatus::Available,
                    true,
                ),
                (
                    "project-b",
                    false,
                    Some("acct-2"),
                    crate::api::GatewayAccountCapacityStatus::Available,
                    false,
                ),
                (
                    "project-c",
                    true,
                    None,
                    crate::api::GatewayAccountCapacityStatus::Exhausted,
                    false,
                ),
            ]
        );
    }

    #[test]
    fn project_worker_routes_treat_blank_account_labels_as_unlabeled() {
        let registry = GatewayScopeRegistry::default();
        registry.register_project_worker(
            GatewayRequestContext {
                tenant_id: "tenant-a".to_string(),
                project_id: Some("project-a".to_string()),
            },
            /*worker_id*/ 1,
        );

        let routes = registry.project_worker_routes(
            |_| true,
            |_| Some("   ".to_string()),
            |_| crate::api::GatewayAccountCapacityStatus::Available,
        );

        assert_eq!(
            routes,
            vec![crate::api::GatewayProjectWorkerRoute {
                tenant_id: "tenant-a".to_string(),
                project_id: "project-a".to_string(),
                worker_id: 1,
                account_id: None,
                account_capacity: crate::api::GatewayAccountCapacityStatus::Available,
                worker_healthy: true,
                account_routing_eligible: false,
            }]
        );
    }

    #[test]
    fn project_worker_selection_distributes_unmapped_projects() {
        let registry = GatewayScopeRegistry::default();
        let project_a = GatewayRequestContext {
            tenant_id: "tenant-a".to_string(),
            project_id: Some("project-a".to_string()),
        };
        let project_b = GatewayRequestContext {
            tenant_id: "tenant-a".to_string(),
            project_id: Some("project-b".to_string()),
        };
        let unscoped_project = GatewayRequestContext {
            tenant_id: "tenant-a".to_string(),
            project_id: None,
        };

        assert_eq!(
            registry.select_project_worker_id(&project_a, [0, 1]),
            Some(0)
        );
        registry.register_project_worker(project_a.clone(), 0);
        assert_eq!(
            registry.select_project_worker_id(&project_a, [0, 1]),
            Some(0)
        );
        assert_eq!(
            registry.select_project_worker_id(&project_b, [0, 1]),
            Some(1)
        );
        assert_eq!(
            registry.select_project_worker_id(&unscoped_project, [0, 1]),
            None
        );
    }

    #[test]
    fn project_worker_selection_prefers_less_loaded_accounts() {
        let registry = GatewayScopeRegistry::default();
        let project_a = GatewayRequestContext {
            tenant_id: "tenant-a".to_string(),
            project_id: Some("project-a".to_string()),
        };
        let project_b = GatewayRequestContext {
            tenant_id: "tenant-a".to_string(),
            project_id: Some("project-b".to_string()),
        };
        let other_tenant_project = GatewayRequestContext {
            tenant_id: "tenant-b".to_string(),
            project_id: Some("project-c".to_string()),
        };

        registry.register_project_worker(project_a.clone(), 0);
        registry.register_project_worker(other_tenant_project, 2);

        assert_eq!(
            registry.select_project_worker_id_with_accounts(&project_a, [0, 1, 2], |worker_id| {
                match worker_id {
                    0 | 1 => Some("acct-a".to_string()),
                    2 => Some("acct-b".to_string()),
                    _ => None,
                }
            }),
            Some(0)
        );
        assert_eq!(
            registry.select_project_worker_id_with_accounts(&project_b, [0, 1, 2], |worker_id| {
                match worker_id {
                    0 | 1 => Some("acct-a".to_string()),
                    2 => Some("acct-b".to_string()),
                    _ => None,
                }
            }),
            Some(2)
        );
    }

    #[test]
    fn project_worker_selection_redistributes_when_affinity_worker_is_not_eligible() {
        let registry = GatewayScopeRegistry::default();
        let project_a = GatewayRequestContext {
            tenant_id: "tenant-a".to_string(),
            project_id: Some("project-a".to_string()),
        };
        let project_b = GatewayRequestContext {
            tenant_id: "tenant-a".to_string(),
            project_id: Some("project-b".to_string()),
        };

        registry.register_project_worker(project_a.clone(), /*worker_id*/ 0);
        registry.register_project_worker(project_b, /*worker_id*/ 1);

        let account_id = |worker_id: usize| {
            ["acct-a", "acct-b", "acct-c"]
                .get(worker_id)
                .map(|account_id| (*account_id).to_string())
        };

        assert_eq!(
            registry.select_project_worker_id_with_accounts(&project_a, [0, 1, 2], account_id),
            Some(0)
        );
        assert_eq!(
            registry.select_project_worker_id_with_accounts(&project_a, [1, 2], account_id),
            Some(2)
        );
    }

    #[test]
    fn project_worker_selection_keeps_existing_affinity_even_when_labeled_workers_exist() {
        let registry = GatewayScopeRegistry::default();
        let project_a = GatewayRequestContext {
            tenant_id: "tenant-a".to_string(),
            project_id: Some("project-a".to_string()),
        };
        let project_b = GatewayRequestContext {
            tenant_id: "tenant-a".to_string(),
            project_id: Some("project-b".to_string()),
        };

        registry.register_project_worker(project_a.clone(), /*worker_id*/ 0);
        registry.register_project_worker(project_b, /*worker_id*/ 1);

        let account_id = |worker_id: usize| match worker_id {
            0 => None,
            1 => Some("acct-a".to_string()),
            2 => Some("acct-b".to_string()),
            _ => None,
        };

        assert_eq!(
            registry.select_project_worker_id_with_accounts(&project_a, [0, 1, 2], account_id),
            Some(0)
        );
    }

    #[test]
    fn project_worker_selection_prefers_labeled_workers_when_no_affinity_exists() {
        let registry = GatewayScopeRegistry::default();
        let project_a = GatewayRequestContext {
            tenant_id: "tenant-a".to_string(),
            project_id: Some("project-a".to_string()),
        };

        let account_id = |worker_id: usize| match worker_id {
            0 => None,
            1 => Some("acct-a".to_string()),
            2 => Some("acct-b".to_string()),
            _ => None,
        };

        assert_eq!(
            registry.select_project_worker_id_with_accounts(&project_a, [0, 1, 2], account_id),
            Some(1)
        );
    }

    #[test]
    fn project_worker_selection_treats_blank_account_labels_as_unlabeled() {
        let registry = GatewayScopeRegistry::default();
        let project_a = GatewayRequestContext {
            tenant_id: "tenant-a".to_string(),
            project_id: Some("project-a".to_string()),
        };

        let account_id = |worker_id| match worker_id {
            0 => Some("   ".to_string()),
            1 => Some("acct-a".to_string()),
            2 => None,
            _ => None,
        };

        assert_eq!(
            registry.select_project_worker_id_with_accounts(&project_a, [0, 1, 2], account_id),
            Some(1)
        );
    }

    #[test]
    fn project_worker_selection_uses_worker_specific_load_for_blank_labels() {
        let registry = GatewayScopeRegistry::default();
        let project_a = GatewayRequestContext {
            tenant_id: "tenant-a".to_string(),
            project_id: Some("project-a".to_string()),
        };

        for index in 0..5 {
            registry.register_project_worker(
                GatewayRequestContext {
                    tenant_id: "tenant-a".to_string(),
                    project_id: Some(format!("project-0-{index}")),
                },
                /*worker_id*/ 0,
            );
        }
        registry.register_project_worker(
            GatewayRequestContext {
                tenant_id: "tenant-a".to_string(),
                project_id: Some("project-1".to_string()),
            },
            /*worker_id*/ 1,
        );

        let account_id = |worker_id| match worker_id {
            0 | 1 => Some("   ".to_string()),
            _ => None,
        };

        assert_eq!(
            registry.select_project_worker_id_with_accounts(&project_a, [0, 1], account_id),
            Some(1)
        );
    }

    #[test]
    fn project_worker_selection_falls_back_to_unlabeled_workers_when_needed() {
        let registry = GatewayScopeRegistry::default();
        let project_a = GatewayRequestContext {
            tenant_id: "tenant-a".to_string(),
            project_id: Some("project-a".to_string()),
        };
        let project_b = GatewayRequestContext {
            tenant_id: "tenant-a".to_string(),
            project_id: Some("project-b".to_string()),
        };

        registry.register_project_worker(project_a.clone(), /*worker_id*/ 0);
        registry.register_project_worker(project_b, /*worker_id*/ 1);

        assert_eq!(
            registry.select_project_worker_id_with_accounts(&project_a, [0, 1], |_| None),
            Some(0)
        );
    }

    #[test]
    fn register_thread_path_worker_if_visible_only_updates_matching_scope() {
        let registry = GatewayScopeRegistry::default();
        let visible_context = GatewayRequestContext {
            tenant_id: "tenant-a".to_string(),
            project_id: Some("project-a".to_string()),
        };
        let other_context = GatewayRequestContext {
            tenant_id: "tenant-a".to_string(),
            project_id: Some("project-b".to_string()),
        };
        registry.register_thread_path_with_worker(
            "/tmp/thread-a.json",
            visible_context.clone(),
            None,
        );

        assert_eq!(
            registry.register_thread_path_worker_if_visible(
                "/tmp/thread-a.json",
                &visible_context,
                Some(7),
            ),
            true
        );
        assert_eq!(
            registry.thread_path_worker_id("/tmp/thread-a.json"),
            Some(7)
        );
        assert_eq!(
            registry.register_thread_path_worker_if_visible(
                "/tmp/thread-a.json",
                &other_context,
                Some(8),
            ),
            false
        );
        assert_eq!(
            registry.register_thread_path_worker_if_visible(
                "/tmp/thread-missing.json",
                &visible_context,
                Some(9),
            ),
            false
        );
        assert_eq!(
            registry.thread_path_worker_id("/tmp/thread-a.json"),
            Some(7)
        );
        assert_eq!(
            registry.thread_path_visible_to(&visible_context, "/tmp/thread-a.json"),
            true
        );
        assert_eq!(
            registry.thread_path_visible_to(&other_context, "/tmp/thread-a.json"),
            false
        );
    }
}
