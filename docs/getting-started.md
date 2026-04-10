# Getting started with Codex CLI

For the broader product overview, see the external
[interactive-mode documentation](https://developers.openai.com/codex/cli/features#running-in-interactive-mode).

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

## A minimal first run

1. Run `codex` in the repository or directory you want to work in.
2. Type a prompt describing the task.
3. If you leave and want to come back later, use `codex resume`.
4. If you want the newest saved session immediately, use `codex resume --last`.
5. If you want to branch from an earlier conversation, use `codex fork`.

## When to use `--include-non-interactive`

The saved-session picker defaults to interactive sessions. Add
`--include-non-interactive` when you want session lookup to include resident
assistants and other non-interactive threads.

This matters in three places:

- `codex resume --last`
- `codex fork --last`
- name-based or picker-based lookup via `codex resume` / `codex fork`

## Related entry points

- Use [`exec.md`](./exec.md) for `codex exec` and `codex exec resume`.
- Use [`slash_commands.md`](./slash_commands.md) for in-app TUI commands such
  as `/resume`, `/fork`, and `/status`.
