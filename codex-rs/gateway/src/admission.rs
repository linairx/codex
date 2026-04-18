use crate::error::GatewayError;
use crate::scope::GatewayRequestContext;
use axum::extract::MatchedPath;
use axum::extract::State;
use axum::http::Request;
use axum::middleware::Next;
use axum::response::Response;
use std::collections::HashMap;
use std::sync::Arc;
use std::sync::Mutex;
use std::time::Duration;
use std::time::Instant;

const ONE_MINUTE: Duration = Duration::from_secs(60);

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct GatewayAdmissionConfig {
    pub request_rate_limit_per_minute: Option<u32>,
    pub turn_start_quota_per_minute: Option<u32>,
}

#[derive(Debug, Clone, Default)]
pub struct GatewayAdmissionController {
    limits: GatewayAdmissionConfig,
    windows: Arc<Mutex<HashMap<AdmissionKey, AdmissionWindow>>>,
}

impl GatewayAdmissionController {
    pub fn new(limits: GatewayAdmissionConfig) -> Self {
        Self {
            limits,
            windows: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    pub fn limits(&self) -> &GatewayAdmissionConfig {
        &self.limits
    }

    fn check_request(
        &self,
        context: &GatewayRequestContext,
        route: &str,
    ) -> Result<(), GatewayError> {
        if let Some(limit) = self.limits.request_rate_limit_per_minute
            && let Some(retry_after_seconds) =
                self.check_window(context, AdmissionBucket::Request, limit, ONE_MINUTE)
        {
            return Err(GatewayError::RateLimited {
                message: format!(
                    "request rate limit exceeded for tenant {}",
                    context.tenant_id
                ),
                retry_after_seconds,
            });
        }

        if is_turn_start_route(route)
            && let Some(limit) = self.limits.turn_start_quota_per_minute
            && let Some(retry_after_seconds) =
                self.check_window(context, AdmissionBucket::TurnStart, limit, ONE_MINUTE)
        {
            return Err(GatewayError::RateLimited {
                message: format!("turn start quota exceeded for tenant {}", context.tenant_id),
                retry_after_seconds,
            });
        }

        Ok(())
    }

    fn check_window(
        &self,
        context: &GatewayRequestContext,
        bucket: AdmissionBucket,
        limit: u32,
        window_duration: Duration,
    ) -> Option<u64> {
        let now = Instant::now();
        let mut windows = lock_windows(&self.windows);
        let key = AdmissionKey {
            context: context.clone(),
            bucket,
        };
        let window = windows.entry(key).or_insert_with(|| AdmissionWindow {
            started_at: now,
            count: 0,
        });

        if now.duration_since(window.started_at) >= window_duration {
            window.started_at = now;
            window.count = 0;
        }

        if window.count >= limit {
            let retry_after = window_duration
                .saturating_sub(now.duration_since(window.started_at))
                .as_secs()
                .max(1);
            return Some(retry_after);
        }

        window.count += 1;
        None
    }
}

pub async fn enforce_admission(
    State(admission): State<GatewayAdmissionController>,
    request: Request<axum::body::Body>,
    next: Next,
) -> Result<Response, GatewayError> {
    let context = GatewayRequestContext::from_headers(request.headers())?;
    let route = request
        .extensions()
        .get::<MatchedPath>()
        .map(axum::extract::MatchedPath::as_str)
        .unwrap_or_else(|| request.uri().path());
    admission.check_request(&context, route)?;
    Ok(next.run(request).await)
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
struct AdmissionKey {
    context: GatewayRequestContext,
    bucket: AdmissionBucket,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
enum AdmissionBucket {
    Request,
    TurnStart,
}

#[derive(Debug, Clone, Copy)]
struct AdmissionWindow {
    started_at: Instant,
    count: u32,
}

fn is_turn_start_route(route: &str) -> bool {
    route.ends_with("/threads/{thread_id}/turns")
}

fn lock_windows(
    windows: &Arc<Mutex<HashMap<AdmissionKey, AdmissionWindow>>>,
) -> std::sync::MutexGuard<'_, HashMap<AdmissionKey, AdmissionWindow>> {
    match windows.lock() {
        Ok(guard) => guard,
        Err(poisoned) => poisoned.into_inner(),
    }
}

#[cfg(test)]
mod tests {
    use super::GatewayAdmissionConfig;
    use super::GatewayAdmissionController;
    use crate::scope::GatewayRequestContext;
    use pretty_assertions::assert_eq;

    #[test]
    fn request_limits_are_scoped_by_tenant_and_project() {
        let controller = GatewayAdmissionController::new(GatewayAdmissionConfig {
            request_rate_limit_per_minute: Some(1),
            turn_start_quota_per_minute: None,
        });
        let tenant_a = GatewayRequestContext {
            tenant_id: "tenant-a".to_string(),
            project_id: Some("project-a".to_string()),
        };
        let tenant_b = GatewayRequestContext {
            tenant_id: "tenant-a".to_string(),
            project_id: Some("project-b".to_string()),
        };

        assert_eq!(
            controller.check_request(&tenant_a, "/threads").is_ok(),
            true
        );
        assert_eq!(
            controller.check_request(&tenant_a, "/threads").is_err(),
            true
        );
        assert_eq!(
            controller.check_request(&tenant_b, "/threads").is_ok(),
            true
        );
    }

    #[test]
    fn turn_start_quota_only_applies_to_turn_start_route() {
        let controller = GatewayAdmissionController::new(GatewayAdmissionConfig {
            request_rate_limit_per_minute: None,
            turn_start_quota_per_minute: Some(1),
        });
        let context = GatewayRequestContext::default();

        assert_eq!(
            controller
                .check_request(&context, "/threads/{thread_id}")
                .is_ok(),
            true
        );
        assert_eq!(
            controller
                .check_request(&context, "/threads/{thread_id}/turns")
                .is_ok(),
            true
        );
        assert_eq!(
            controller
                .check_request(&context, "/threads/{thread_id}/turns")
                .is_err(),
            true
        );
    }
}
