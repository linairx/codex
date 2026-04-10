use codex_app_server_protocol::ThreadSourceKind;

pub(crate) fn interactive_thread_source_kinds() -> Vec<ThreadSourceKind> {
    vec![ThreadSourceKind::Cli, ThreadSourceKind::VsCode]
}

pub(crate) fn all_thread_source_kinds() -> Vec<ThreadSourceKind> {
    vec![
        ThreadSourceKind::Cli,
        ThreadSourceKind::VsCode,
        ThreadSourceKind::Exec,
        ThreadSourceKind::AppServer,
        ThreadSourceKind::SubAgent,
        ThreadSourceKind::SubAgentReview,
        ThreadSourceKind::SubAgentCompact,
        ThreadSourceKind::SubAgentThreadSpawn,
        ThreadSourceKind::SubAgentOther,
        ThreadSourceKind::Unknown,
    ]
}

#[cfg(test)]
mod tests {
    use super::all_thread_source_kinds;
    use super::interactive_thread_source_kinds;
    use codex_app_server_protocol::ThreadSourceKind;
    use pretty_assertions::assert_eq;

    #[test]
    fn interactive_thread_source_kinds_remains_interactive_only() {
        assert_eq!(
            interactive_thread_source_kinds(),
            vec![ThreadSourceKind::Cli, ThreadSourceKind::VsCode]
        );
    }

    #[test]
    fn all_thread_source_kinds_covers_interactive_and_non_interactive_sources() {
        assert_eq!(
            all_thread_source_kinds(),
            vec![
                ThreadSourceKind::Cli,
                ThreadSourceKind::VsCode,
                ThreadSourceKind::Exec,
                ThreadSourceKind::AppServer,
                ThreadSourceKind::SubAgent,
                ThreadSourceKind::SubAgentReview,
                ThreadSourceKind::SubAgentCompact,
                ThreadSourceKind::SubAgentThreadSpawn,
                ThreadSourceKind::SubAgentOther,
                ThreadSourceKind::Unknown,
            ]
        );
    }
}
