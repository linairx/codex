use axum::Json;
use axum::http::StatusCode;
use axum::response::IntoResponse;
use codex_app_server_client::TypedRequestError;
use codex_app_server_protocol::JSONRPCErrorError;
use serde::Serialize;

const INVALID_PARAMS_ERROR_CODE: i64 = -32602;

#[derive(Debug)]
pub enum GatewayError {
    InvalidRequest(String),
    NotFound(String),
    RateLimited {
        message: String,
        retry_after_seconds: u64,
    },
    Upstream(String),
}

impl From<TypedRequestError> for GatewayError {
    fn from(value: TypedRequestError) -> Self {
        match value {
            TypedRequestError::Server { source, .. } => map_server_error(source),
            TypedRequestError::Transport { .. } | TypedRequestError::Deserialize { .. } => {
                Self::Upstream(value.to_string())
            }
        }
    }
}

fn map_server_error(source: JSONRPCErrorError) -> GatewayError {
    if source.code == INVALID_PARAMS_ERROR_CODE {
        if source.message.starts_with("thread not found:")
            || source.message.starts_with("turn not found:")
        {
            return GatewayError::NotFound(source.message);
        }
        return GatewayError::InvalidRequest(source.message);
    }

    GatewayError::Upstream(source.message)
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct ErrorBody {
    error: String,
}

impl IntoResponse for GatewayError {
    fn into_response(self) -> axum::response::Response {
        let (status, message) = match self {
            Self::InvalidRequest(message) => (StatusCode::BAD_REQUEST, message),
            Self::NotFound(message) => (StatusCode::NOT_FOUND, message),
            Self::RateLimited {
                message,
                retry_after_seconds,
            } => {
                return (
                    StatusCode::TOO_MANY_REQUESTS,
                    [("retry-after", retry_after_seconds.to_string())],
                    Json(ErrorBody { error: message }),
                )
                    .into_response();
            }
            Self::Upstream(message) => (StatusCode::BAD_GATEWAY, message),
        };
        (status, Json(ErrorBody { error: message })).into_response()
    }
}

#[cfg(test)]
mod tests {
    use super::GatewayError;
    use axum::response::IntoResponse;
    use codex_app_server_client::TypedRequestError;
    use codex_app_server_protocol::JSONRPCErrorError;
    use pretty_assertions::assert_eq;
    use std::io;

    #[test]
    fn maps_invalid_params_to_bad_request() {
        let error = GatewayError::from(TypedRequestError::Server {
            method: "thread/start".to_string(),
            source: JSONRPCErrorError {
                code: -32602,
                data: None,
                message: "cwd must be absolute".to_string(),
            },
        });

        assert!(matches!(
            error,
            GatewayError::InvalidRequest(message) if message == "cwd must be absolute"
        ));
    }

    #[test]
    fn maps_missing_thread_to_not_found() {
        let error = GatewayError::from(TypedRequestError::Server {
            method: "thread/read".to_string(),
            source: JSONRPCErrorError {
                code: -32602,
                data: None,
                message: "thread not found: thread-123".to_string(),
            },
        });

        assert!(matches!(
            error,
            GatewayError::NotFound(message) if message == "thread not found: thread-123"
        ));
    }

    #[test]
    fn maps_transport_failures_to_bad_gateway() {
        let error = GatewayError::from(TypedRequestError::Transport {
            method: "thread/read".to_string(),
            source: io::Error::other("socket closed"),
        });

        assert_eq!(
            format!("{error:?}"),
            "Upstream(\"thread/read transport error: socket closed\")"
        );
    }

    #[test]
    fn rate_limited_errors_render_retry_after_header() {
        let response = GatewayError::RateLimited {
            message: "rate limit exceeded".to_string(),
            retry_after_seconds: 42,
        }
        .into_response();

        assert_eq!(response.status(), axum::http::StatusCode::TOO_MANY_REQUESTS);
        assert_eq!(
            response
                .headers()
                .get("retry-after")
                .and_then(|value| value.to_str().ok()),
            Some("42")
        );
    }
}
