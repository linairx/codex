use super::REMOTE_ACCOUNT_LOGIN_STATE_PATHS_ENV;
use super::REMOTE_ACCOUNT_POOL_ACCOUNT_IDS_ENV;
use super::REMOTE_ACCOUNT_POOL_LOGIN_STATE_PATHS_ENV;
use super::parse_remote_account_login_state_path_values;
use super::parse_remote_account_login_state_paths;
use super::parse_remote_account_pool_account_id_values;
use super::parse_remote_account_pool_account_ids;
use super::parse_remote_account_pool_login_state_path_values;
use super::parse_remote_account_pool_login_state_paths;
use pretty_assertions::assert_eq;
use std::io::ErrorKind;

#[test]
fn account_login_state_paths_default_to_unconfigured_slots() {
    assert_eq!(
        parse_remote_account_login_state_paths(None, 2).expect("missing env should parse"),
        vec![None, None]
    );
    assert_eq!(
        parse_remote_account_login_state_paths(Some("   "), 2).expect("blank env should parse"),
        vec![None, None]
    );
}

#[test]
fn account_login_state_paths_trim_one_path_per_worker() {
    assert_eq!(
        parse_remote_account_login_state_paths(
            Some(" /codex-home/acct-a , /codex-home/acct-b "),
            2
        )
        .expect("configured paths should parse"),
        vec![
            Some("/codex-home/acct-a".to_string()),
            Some("/codex-home/acct-b".to_string()),
        ]
    );
}

#[test]
fn account_login_state_paths_reject_blank_entries() {
    let err = parse_remote_account_login_state_paths(Some("/codex-home/acct-a, "), 2)
        .expect_err("blank path entry should fail");

    assert_eq!(err.kind(), ErrorKind::InvalidInput);
    assert_eq!(
        err.to_string(),
        format!("{REMOTE_ACCOUNT_LOGIN_STATE_PATHS_ENV} must not contain blank paths")
    );
}

#[test]
fn account_login_state_paths_reject_worker_count_mismatch() {
    let err = parse_remote_account_login_state_paths(Some("/codex-home/acct-a"), 2)
        .expect_err("path count mismatch should fail");

    assert_eq!(err.kind(), ErrorKind::InvalidInput);
    assert_eq!(
        err.to_string(),
        format!(
            "{REMOTE_ACCOUNT_LOGIN_STATE_PATHS_ENV} must contain one path per remote worker when configured"
        )
    );
}

#[test]
fn account_login_state_path_values_map_one_path_per_worker() {
    assert_eq!(
        parse_remote_account_login_state_path_values(
            &[
                "/codex-home/acct-a".to_string(),
                "/codex-home/acct-b".to_string(),
            ],
            2
        )
        .expect("configured path values should parse"),
        vec![
            Some("/codex-home/acct-a".to_string()),
            Some("/codex-home/acct-b".to_string()),
        ]
    );
}

#[test]
fn account_login_state_path_values_reject_worker_count_mismatch() {
    let err = parse_remote_account_login_state_path_values(&["/codex-home/acct-a".to_string()], 2)
        .expect_err("path count mismatch should fail");

    assert_eq!(err.kind(), ErrorKind::InvalidInput);
    assert_eq!(
        err.to_string(),
        "--remote-account-login-state-path must be provided once per remote worker when configured"
    );
}

#[test]
fn account_pool_account_ids_default_to_empty_inventory() {
    assert_eq!(
        parse_remote_account_pool_account_ids(None).expect("missing env should parse"),
        Vec::<String>::new()
    );
    assert_eq!(
        parse_remote_account_pool_account_ids(Some("   ")).expect("blank env should parse"),
        Vec::<String>::new()
    );
}

#[test]
fn account_pool_account_ids_trim_entries() {
    assert_eq!(
        parse_remote_account_pool_account_ids(Some(" acct-a , acct-b "))
            .expect("configured account ids should parse"),
        vec!["acct-a".to_string(), "acct-b".to_string()]
    );
}

#[test]
fn account_pool_account_ids_reject_blank_entries() {
    let err = parse_remote_account_pool_account_ids(Some("acct-a, "))
        .expect_err("blank account id entry should fail");

    assert_eq!(err.kind(), ErrorKind::InvalidInput);
    assert_eq!(
        err.to_string(),
        format!("{REMOTE_ACCOUNT_POOL_ACCOUNT_IDS_ENV} must not contain blank account ids")
    );
}

#[test]
fn account_pool_account_id_values_trim_entries() {
    assert_eq!(
        parse_remote_account_pool_account_id_values(&[
            " acct-a ".to_string(),
            "acct-b".to_string()
        ])
        .expect("configured account id values should parse"),
        vec!["acct-a".to_string(), "acct-b".to_string()]
    );
}

#[test]
fn account_pool_account_id_values_reject_blank_entries() {
    let err = parse_remote_account_pool_account_id_values(&["   ".to_string()])
        .expect_err("blank account id value should fail");

    assert_eq!(err.kind(), ErrorKind::InvalidInput);
    assert_eq!(
        err.to_string(),
        "--remote-account-pool-account-id must not be blank"
    );
}

#[test]
fn account_pool_login_state_paths_default_to_unconfigured_accounts() {
    assert_eq!(
        parse_remote_account_pool_login_state_paths(None, 2).expect("missing env should parse"),
        vec![None, None]
    );
    assert_eq!(
        parse_remote_account_pool_login_state_paths(Some("   "), 2)
            .expect("blank env should parse"),
        vec![None, None]
    );
}

#[test]
fn account_pool_login_state_paths_trim_one_path_per_pool_account() {
    assert_eq!(
        parse_remote_account_pool_login_state_paths(
            Some(" /codex-home/acct-a , /codex-home/acct-b "),
            2,
        )
        .expect("configured paths should parse"),
        vec![
            Some("/codex-home/acct-a".to_string()),
            Some("/codex-home/acct-b".to_string()),
        ]
    );
}

#[test]
fn account_pool_login_state_paths_reject_blank_entries() {
    let err = parse_remote_account_pool_login_state_paths(Some("/codex-home/acct-a, "), 2)
        .expect_err("blank path entry should fail");

    assert_eq!(err.kind(), ErrorKind::InvalidInput);
    assert_eq!(
        err.to_string(),
        format!("{REMOTE_ACCOUNT_POOL_LOGIN_STATE_PATHS_ENV} must not contain blank paths")
    );
}

#[test]
fn account_pool_login_state_paths_reject_account_count_mismatch() {
    let err = parse_remote_account_pool_login_state_paths(Some("/codex-home/acct-a"), 2)
        .expect_err("path count mismatch should fail");

    assert_eq!(err.kind(), ErrorKind::InvalidInput);
    assert_eq!(
        err.to_string(),
        format!(
            "{REMOTE_ACCOUNT_POOL_LOGIN_STATE_PATHS_ENV} must contain one path per account-pool account when configured"
        )
    );
}

#[test]
fn account_pool_login_state_path_values_map_one_path_per_pool_account() {
    assert_eq!(
        parse_remote_account_pool_login_state_path_values(
            &[
                "/codex-home/acct-a".to_string(),
                "/codex-home/acct-b".to_string(),
            ],
            2,
        )
        .expect("configured path values should parse"),
        vec![
            Some("/codex-home/acct-a".to_string()),
            Some("/codex-home/acct-b".to_string()),
        ]
    );
}

#[test]
fn account_pool_login_state_path_values_reject_account_count_mismatch() {
    let err =
        parse_remote_account_pool_login_state_path_values(&["/codex-home/acct-a".to_string()], 2)
            .expect_err("path count mismatch should fail");

    assert_eq!(err.kind(), ErrorKind::InvalidInput);
    assert_eq!(
        err.to_string(),
        "--remote-account-pool-login-state-path must be provided once per account-pool account when configured"
    );
}
