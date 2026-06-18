use codex_app_server_protocol::JSONRPCErrorError;

pub(crate) const RATE_LIMITED_ERROR_CODE: i64 = -32001;
pub(crate) const TOO_MANY_PENDING_SERVER_REQUESTS_MESSAGE: &str =
    "too many pending server requests for websocket connection";
pub(crate) const TOO_MANY_PENDING_CLIENT_REQUESTS_MESSAGE: &str =
    "too many pending client requests for websocket connection";

pub(crate) fn pending_server_request_limit_error(
    pending_server_request_count: usize,
    max_pending_server_requests: usize,
) -> Option<JSONRPCErrorError> {
    if pending_server_request_count < max_pending_server_requests {
        None
    } else {
        Some(JSONRPCErrorError {
            code: RATE_LIMITED_ERROR_CODE,
            message: TOO_MANY_PENDING_SERVER_REQUESTS_MESSAGE.to_string(),
            data: None,
        })
    }
}

pub(crate) fn pending_client_request_limit_error(
    pending_client_request_count: usize,
    max_pending_client_requests: usize,
) -> Option<JSONRPCErrorError> {
    if pending_client_request_count < max_pending_client_requests {
        None
    } else {
        Some(JSONRPCErrorError {
            code: RATE_LIMITED_ERROR_CODE,
            message: TOO_MANY_PENDING_CLIENT_REQUESTS_MESSAGE.to_string(),
            data: None,
        })
    }
}

#[cfg(test)]
mod tests {
    use pretty_assertions::assert_eq;

    #[test]
    fn pending_server_request_limit_error_allows_counts_below_limit() {
        assert_eq!(super::pending_server_request_limit_error(0, 1), None);
        assert_eq!(super::pending_server_request_limit_error(3, 4), None);
    }

    #[test]
    fn pending_server_request_limit_error_rejects_counts_at_or_above_limit() {
        let error = super::pending_server_request_limit_error(1, 1).expect("error");
        assert_eq!(error.code, super::RATE_LIMITED_ERROR_CODE);
        assert_eq!(
            error.message,
            super::TOO_MANY_PENDING_SERVER_REQUESTS_MESSAGE
        );

        let error = super::pending_server_request_limit_error(2, 1).expect("error");
        assert_eq!(error.code, super::RATE_LIMITED_ERROR_CODE);
        assert_eq!(
            error.message,
            super::TOO_MANY_PENDING_SERVER_REQUESTS_MESSAGE
        );
    }

    #[test]
    fn pending_client_request_limit_error_allows_counts_below_limit() {
        assert_eq!(super::pending_client_request_limit_error(0, 1), None);
        assert_eq!(super::pending_client_request_limit_error(3, 4), None);
    }

    #[test]
    fn pending_client_request_limit_error_rejects_counts_at_or_above_limit() {
        let error = super::pending_client_request_limit_error(1, 1).expect("error");
        assert_eq!(error.code, super::RATE_LIMITED_ERROR_CODE);
        assert_eq!(
            error.message,
            super::TOO_MANY_PENDING_CLIENT_REQUESTS_MESSAGE
        );

        let error = super::pending_client_request_limit_error(2, 1).expect("error");
        assert_eq!(error.code, super::RATE_LIMITED_ERROR_CODE);
        assert_eq!(
            error.message,
            super::TOO_MANY_PENDING_CLIENT_REQUESTS_MESSAGE
        );
    }
}
