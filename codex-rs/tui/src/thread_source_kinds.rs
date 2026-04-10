use codex_app_server_protocol::ThreadSourceKind;

/// Source kinds to use when the caller wants to preserve app-server's default
/// interactive-only behavior (`cli` / `vscode`).
pub(crate) fn interactive_thread_source_kinds() -> Vec<ThreadSourceKind> {
    vec![ThreadSourceKind::Cli, ThreadSourceKind::VsCode]
}

/// Source kinds to use when the caller must opt into both interactive and
/// non-interactive threads instead of relying on app-server's interactive-only
/// default for omitted/empty `sourceKinds`.
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
