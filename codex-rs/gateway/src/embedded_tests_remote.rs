use super::*;

const EMBEDDED_TOOL_NAMESPACE: &str = "mcp__codex_apps__calendar";
const EMBEDDED_CALLABLE_TOOL_NAME: &str = "_confirm_action";
const EMBEDDED_TOOL_CALL_ID: &str = "call-calendar-confirm";

#[path = "embedded_tests_remote_server_requests.rs"]
mod embedded_tests_remote_server_requests;

#[path = "embedded_tests_remote_tool_requests.rs"]
mod embedded_tests_remote_tool_requests;

#[path = "embedded_tests_remote_plan.rs"]
mod embedded_tests_remote_plan;

#[path = "embedded_tests_remote_realtime.rs"]
mod embedded_tests_remote_realtime;

#[path = "embedded_tests_remote_operational.rs"]
mod embedded_tests_remote_operational;

#[path = "embedded_tests_remote_control.rs"]
mod embedded_tests_remote_control;

#[path = "embedded_tests_remote_bootstrap.rs"]
mod embedded_tests_remote_bootstrap;

#[path = "embedded_tests_remote_embedded_turn.rs"]
mod embedded_tests_remote_embedded_turn;

#[path = "embedded_tests_remote_embedded_thread_control.rs"]
mod embedded_tests_remote_embedded_thread_control;

#[path = "embedded_tests_remote_late.rs"]
mod embedded_tests_remote_late;
