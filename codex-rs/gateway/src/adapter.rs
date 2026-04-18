use crate::api::CreateThreadRequest;
use crate::api::ListThreadsRequest;
use crate::api::StartTurnRequest;
use codex_app_server_protocol::ClientRequest;
use codex_app_server_protocol::RequestId;
use codex_app_server_protocol::ThreadListParams;
use codex_app_server_protocol::ThreadReadParams;
use codex_app_server_protocol::ThreadStartParams;
use codex_app_server_protocol::TurnInterruptParams;
use codex_app_server_protocol::TurnStartParams;
use codex_app_server_protocol::UserInput;

pub fn thread_start_request(request_id: RequestId, request: CreateThreadRequest) -> ClientRequest {
    ClientRequest::ThreadStart {
        request_id,
        params: ThreadStartParams {
            cwd: request.cwd,
            model: request.model,
            ephemeral: request.ephemeral,
            ..ThreadStartParams::default()
        },
    }
}

pub fn thread_read_request(request_id: RequestId, thread_id: String) -> ClientRequest {
    ClientRequest::ThreadRead {
        request_id,
        params: ThreadReadParams {
            thread_id,
            include_turns: false,
        },
    }
}

pub fn thread_list_request(request_id: RequestId, request: ListThreadsRequest) -> ClientRequest {
    ClientRequest::ThreadList {
        request_id,
        params: ThreadListParams {
            cursor: request.cursor,
            limit: request.limit,
            sort_key: request.sort_key.map(Into::into),
            sort_direction: request.sort_direction.map(Into::into),
            model_providers: None,
            source_kinds: None,
            archived: request.archived,
            cwd: request.cwd,
            search_term: request.search_term,
        },
    }
}

pub fn turn_start_request(
    request_id: RequestId,
    thread_id: String,
    request: StartTurnRequest,
) -> ClientRequest {
    ClientRequest::TurnStart {
        request_id,
        params: TurnStartParams {
            thread_id,
            input: vec![UserInput::Text {
                text: request.input,
                text_elements: Vec::new(),
            }],
            responsesapi_client_metadata: None,
            cwd: None,
            approval_policy: None,
            approvals_reviewer: None,
            sandbox_policy: None,
            model: None,
            service_tier: None,
            effort: None,
            summary: None,
            personality: None,
            output_schema: None,
            collaboration_mode: None,
        },
    }
}

pub fn turn_interrupt_request(
    request_id: RequestId,
    thread_id: String,
    turn_id: String,
) -> ClientRequest {
    ClientRequest::TurnInterrupt {
        request_id,
        params: TurnInterruptParams { thread_id, turn_id },
    }
}

#[cfg(test)]
mod tests {
    use super::thread_list_request;
    use super::thread_read_request;
    use super::thread_start_request;
    use super::turn_interrupt_request;
    use super::turn_start_request;
    use crate::api::CreateThreadRequest;
    use crate::api::GatewaySortDirection;
    use crate::api::GatewayThreadSortKey;
    use crate::api::ListThreadsRequest;
    use crate::api::StartTurnRequest;
    use codex_app_server_protocol::ClientRequest;
    use codex_app_server_protocol::RequestId;
    use codex_app_server_protocol::SortDirection;
    use codex_app_server_protocol::ThreadListParams;
    use codex_app_server_protocol::ThreadReadParams;
    use codex_app_server_protocol::ThreadSortKey;
    use codex_app_server_protocol::ThreadStartParams;
    use codex_app_server_protocol::TurnInterruptParams;
    use codex_app_server_protocol::TurnStartParams;
    use codex_app_server_protocol::UserInput;
    use pretty_assertions::assert_eq;

    #[test]
    fn maps_create_thread_request_to_app_server_request() {
        let request = thread_start_request(
            RequestId::Integer(1),
            CreateThreadRequest {
                cwd: Some("/tmp/project".to_string()),
                model: Some("gpt-5.4".to_string()),
                ephemeral: Some(true),
            },
        );

        assert_eq!(
            request,
            ClientRequest::ThreadStart {
                request_id: RequestId::Integer(1),
                params: ThreadStartParams {
                    cwd: Some("/tmp/project".to_string()),
                    model: Some("gpt-5.4".to_string()),
                    ephemeral: Some(true),
                    ..ThreadStartParams::default()
                },
            }
        );
    }

    #[test]
    fn maps_thread_read_request_to_app_server_request() {
        let request = thread_read_request(RequestId::Integer(2), "thread-123".to_string());

        assert_eq!(
            request,
            ClientRequest::ThreadRead {
                request_id: RequestId::Integer(2),
                params: ThreadReadParams {
                    thread_id: "thread-123".to_string(),
                    include_turns: false,
                },
            }
        );
    }

    #[test]
    fn maps_thread_list_request_to_app_server_request() {
        let request = thread_list_request(
            RequestId::Integer(3),
            ListThreadsRequest {
                cursor: Some("cursor-1".to_string()),
                limit: Some(20),
                sort_key: Some(GatewayThreadSortKey::UpdatedAt),
                sort_direction: Some(GatewaySortDirection::Asc),
                archived: Some(false),
                cwd: Some("/tmp/project".to_string()),
                search_term: Some("gateway".to_string()),
            },
        );

        assert_eq!(
            request,
            ClientRequest::ThreadList {
                request_id: RequestId::Integer(3),
                params: ThreadListParams {
                    cursor: Some("cursor-1".to_string()),
                    limit: Some(20),
                    sort_key: Some(ThreadSortKey::UpdatedAt),
                    sort_direction: Some(SortDirection::Asc),
                    model_providers: None,
                    source_kinds: None,
                    archived: Some(false),
                    cwd: Some("/tmp/project".to_string()),
                    search_term: Some("gateway".to_string()),
                },
            }
        );
    }

    #[test]
    fn maps_start_turn_request_to_app_server_request() {
        let request = turn_start_request(
            RequestId::Integer(4),
            "thread-123".to_string(),
            StartTurnRequest {
                input: "hello".to_string(),
            },
        );

        assert_eq!(
            request,
            ClientRequest::TurnStart {
                request_id: RequestId::Integer(4),
                params: TurnStartParams {
                    thread_id: "thread-123".to_string(),
                    input: vec![UserInput::Text {
                        text: "hello".to_string(),
                        text_elements: Vec::new(),
                    }],
                    responsesapi_client_metadata: None,
                    cwd: None,
                    approval_policy: None,
                    approvals_reviewer: None,
                    sandbox_policy: None,
                    model: None,
                    service_tier: None,
                    effort: None,
                    summary: None,
                    personality: None,
                    output_schema: None,
                    collaboration_mode: None,
                },
            }
        );
    }

    #[test]
    fn maps_turn_interrupt_request_to_app_server_request() {
        let request = turn_interrupt_request(
            RequestId::Integer(5),
            "thread-123".to_string(),
            "turn-456".to_string(),
        );

        assert_eq!(
            request,
            ClientRequest::TurnInterrupt {
                request_id: RequestId::Integer(5),
                params: TurnInterruptParams {
                    thread_id: "thread-123".to_string(),
                    turn_id: "turn-456".to_string(),
                },
            }
        );
    }
}
