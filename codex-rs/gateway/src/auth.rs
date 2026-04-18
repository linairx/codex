use axum::body::Body;
use axum::extract::State;
use axum::http::Request;
use axum::http::StatusCode;
use axum::http::header::AUTHORIZATION;
use axum::middleware::Next;
use axum::response::IntoResponse;
use axum::response::Response;

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub enum GatewayAuth {
    #[default]
    Disabled,
    BearerToken {
        token: String,
    },
}

impl GatewayAuth {
    pub fn is_enabled(&self) -> bool {
        !matches!(self, Self::Disabled)
    }
}

#[derive(Debug)]
pub struct GatewayAuthError;

impl IntoResponse for GatewayAuthError {
    fn into_response(self) -> Response {
        (
            StatusCode::UNAUTHORIZED,
            [("www-authenticate", "Bearer")],
            axum::Json(serde_json::json!({
                "error": "authentication required",
            })),
        )
            .into_response()
    }
}

pub async fn require_auth(
    State(auth): State<GatewayAuth>,
    request: Request<Body>,
    next: Next,
) -> Result<Response, GatewayAuthError> {
    match auth {
        GatewayAuth::Disabled => Ok(next.run(request).await),
        GatewayAuth::BearerToken { token } => {
            let expected = format!("Bearer {token}");
            let authorized = request
                .headers()
                .get(AUTHORIZATION)
                .and_then(|value| value.to_str().ok())
                .is_some_and(|value| value == expected);

            if authorized {
                Ok(next.run(request).await)
            } else {
                Err(GatewayAuthError)
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::GatewayAuth;
    use pretty_assertions::assert_eq;

    #[test]
    fn auth_is_disabled_by_default() {
        assert_eq!(GatewayAuth::default().is_enabled(), false);
    }
}
