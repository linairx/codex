use crate::api::GatewayRemoteWorkerHealth;
use std::sync::RwLock;
use std::sync::RwLockReadGuard;
use std::sync::RwLockWriteGuard;

#[derive(Debug, Clone, PartialEq, Eq)]
struct RemoteWorkerHealthState {
    websocket_url: String,
    healthy: bool,
    last_error: Option<String>,
}

pub struct RemoteWorkerHealthRegistry {
    workers: RwLock<Vec<RemoteWorkerHealthState>>,
}

impl RemoteWorkerHealthRegistry {
    pub fn new(websocket_urls: Vec<String>) -> Self {
        Self {
            workers: RwLock::new(
                websocket_urls
                    .into_iter()
                    .map(|websocket_url| RemoteWorkerHealthState {
                        websocket_url,
                        healthy: true,
                        last_error: None,
                    })
                    .collect(),
            ),
        }
    }

    pub fn is_healthy(&self, worker_id: usize) -> bool {
        read_guard(&self.workers)
            .get(worker_id)
            .is_some_and(|worker| worker.healthy)
    }

    pub fn mark_unhealthy(&self, worker_id: usize, error: Option<String>) {
        if let Some(worker) = write_guard(&self.workers).get_mut(worker_id) {
            worker.healthy = false;
            worker.last_error = error;
        }
    }

    pub fn mark_healthy(&self, worker_id: usize) {
        if let Some(worker) = write_guard(&self.workers).get_mut(worker_id) {
            worker.healthy = true;
        }
    }

    pub fn snapshot(&self) -> Vec<GatewayRemoteWorkerHealth> {
        read_guard(&self.workers)
            .iter()
            .enumerate()
            .map(|(worker_id, worker)| GatewayRemoteWorkerHealth {
                worker_id,
                websocket_url: worker.websocket_url.clone(),
                healthy: worker.healthy,
                last_error: worker.last_error.clone(),
            })
            .collect()
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

#[cfg(test)]
mod tests {
    use super::RemoteWorkerHealthRegistry;
    use pretty_assertions::assert_eq;

    #[test]
    fn workers_start_healthy_and_capture_last_error() {
        let registry = RemoteWorkerHealthRegistry::new(vec![
            "ws://127.0.0.1:8081".to_string(),
            "ws://127.0.0.1:8082".to_string(),
        ]);

        assert_eq!(registry.is_healthy(0), true);
        assert_eq!(registry.is_healthy(1), true);

        registry.mark_unhealthy(1, Some("socket closed".to_string()));

        assert_eq!(registry.is_healthy(0), true);
        assert_eq!(registry.is_healthy(1), false);
        assert_eq!(
            registry.snapshot(),
            vec![
                crate::api::GatewayRemoteWorkerHealth {
                    worker_id: 0,
                    websocket_url: "ws://127.0.0.1:8081".to_string(),
                    healthy: true,
                    last_error: None,
                },
                crate::api::GatewayRemoteWorkerHealth {
                    worker_id: 1,
                    websocket_url: "ws://127.0.0.1:8082".to_string(),
                    healthy: false,
                    last_error: Some("socket closed".to_string()),
                },
            ]
        );
    }

    #[test]
    fn workers_can_be_marked_healthy_again_without_losing_last_error() {
        let registry = RemoteWorkerHealthRegistry::new(vec!["ws://127.0.0.1:8081".to_string()]);

        registry.mark_unhealthy(0, Some("socket closed".to_string()));
        registry.mark_healthy(0);

        assert_eq!(registry.is_healthy(0), true);
        assert_eq!(
            registry.snapshot(),
            vec![crate::api::GatewayRemoteWorkerHealth {
                worker_id: 0,
                websocket_url: "ws://127.0.0.1:8081".to_string(),
                healthy: true,
                last_error: Some("socket closed".to_string()),
            }]
        );
    }
}
