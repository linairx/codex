# Getting started with Codex CLI

For an overview of Codex CLI features, see [this documentation](https://developers.openai.com/codex/cli/features#running-in-interactive-mode).

## Common session flows

- Start a new interactive session: run `codex`.
- Resume or reconnect to a previous session: run `codex resume`.
- For `codex resume` or `codex fork`, add `--include-non-interactive` when you
  need the picker, `--last`, or name-based lookup to include resident
  assistants and other non-interactive sessions.
- Fork from a previous session: run `codex fork`.

Useful shortcuts:

- `codex resume --last` resumes or reconnects to the newest recorded session.
- `codex fork --last` forks the newest recorded session.
- `codex resume <SESSION_ID_OR_NAME>` targets a specific saved session.
- `codex fork <SESSION_ID_OR_NAME>` forks a specific saved session.

Resident assistant sessions use the same `codex resume` entry point, but the UI
and help text describe that action as `reconnect` when the saved session's
`thread.mode` is `residentAssistant`.
