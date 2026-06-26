use super::*;
use pretty_assertions::assert_eq;

#[path = "v2_tests_cases_0_initialize.rs"]
mod v2_tests_cases_0_initialize;

#[path = "v2_tests_cases_0_websocket.rs"]
mod v2_tests_cases_0_websocket;

pub(crate) use super::v2_tests_cases_0_late::*;

#[path = "v2_tests_cases_0_downstream_initialize.rs"]
mod v2_tests_cases_0_downstream_initialize;

#[path = "v2_tests_cases_0_multi_worker_initialize.rs"]
mod v2_tests_cases_0_multi_worker_initialize;

#[path = "v2_tests_cases_0_upgrade_policy.rs"]
mod v2_tests_cases_0_upgrade_policy;

#[path = "v2_tests_cases_0_frame_handling.rs"]
mod v2_tests_cases_0_frame_handling;

#[path = "v2_tests_cases_0_frame_handling_tail.rs"]
mod v2_tests_cases_0_frame_handling_tail;

#[tokio::test]
async fn await_io_with_timeout_returns_result_before_timeout() {
    let result = await_io_with_timeout(
        async { Ok::<_, std::io::Error>("ok") },
        Duration::from_millis(50),
        "timed out",
    )
    .await
    .expect("future should complete");

    assert_eq!(result, "ok");
}

#[tokio::test]
async fn await_io_with_timeout_returns_timed_out_error() {
    let error = await_io_with_timeout(
        async {
            sleep(Duration::from_millis(50)).await;
            Ok::<_, std::io::Error>(())
        },
        Duration::from_millis(1),
        "timed out",
    )
    .await
    .expect_err("future should time out");

    assert_eq!(error.kind(), std::io::ErrorKind::TimedOut);
    assert_eq!(error.to_string(), "timed out");
}
