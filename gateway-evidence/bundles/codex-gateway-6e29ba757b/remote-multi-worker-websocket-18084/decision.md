# Gateway Multi-Worker Promotion Decision

- Decision: evidence accepted for northbound WebSocket bootstrap and initial project-aware multi-worker routing.
- Scope: the isolated Docker Compose topology and build recorded in [README.md](README.md).
- Evidence: two real WebSocket sessions, two successful `thread/start` responses, and health accounting that maps the two projects to distinct healthy workers/accounts.

This is not a full promotion decision. Reconnect, fail-closed behavior, account-capacity transitions, restoration/handoff, turns, backlog, cleanup, and metrics-backend reconciliation remain outside this capture.
