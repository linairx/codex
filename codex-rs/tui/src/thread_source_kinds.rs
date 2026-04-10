use codex_app_server_protocol::ThreadSourceKind;
use codex_app_server_protocol::all_thread_source_kinds as protocol_all_thread_source_kinds;
use codex_app_server_protocol::interactive_thread_source_kinds as protocol_interactive_thread_source_kinds;

/// Source kinds to use when the caller wants to preserve app-server's default
/// interactive-only behavior (`cli` / `vscode`).
pub(crate) fn interactive_thread_source_kinds() -> Vec<ThreadSourceKind> {
    protocol_interactive_thread_source_kinds()
}

/// Source kinds to use when the caller must opt into both interactive and
/// non-interactive threads instead of relying on app-server's interactive-only
/// default for omitted/empty `sourceKinds`.
pub(crate) fn all_thread_source_kinds() -> Vec<ThreadSourceKind> {
    protocol_all_thread_source_kinds()
}

#[cfg(test)]
mod tests {
    use super::all_thread_source_kinds;
    use super::interactive_thread_source_kinds;
    use codex_app_server_protocol::all_thread_source_kinds as protocol_all_thread_source_kinds;
    use codex_app_server_protocol::interactive_thread_source_kinds as protocol_interactive_thread_source_kinds;
    use pretty_assertions::assert_eq;

    #[test]
    fn interactive_thread_source_kinds_remains_interactive_only() {
        assert_eq!(
            interactive_thread_source_kinds(),
            protocol_interactive_thread_source_kinds()
        );
    }

    #[test]
    fn all_thread_source_kinds_covers_interactive_and_non_interactive_sources() {
        assert_eq!(
            all_thread_source_kinds(),
            protocol_all_thread_source_kinds()
        );
    }
}
