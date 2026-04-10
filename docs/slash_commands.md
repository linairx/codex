# Slash commands

Slash commands are TUI-only commands entered in the composer with a leading
`/`. Typing `/` opens the command picker, and typing a prefix such as `/res`
filters the list before you press `Enter`.

For the broader product overview, see the external
[slash command documentation](https://developers.openai.com/codex/cli/slash-commands).
This page focuses on the commands that matter most for local session
management.

## Common session commands

- `/new` starts a fresh chat without leaving the current TUI session.
- `/resume` opens the saved-session picker inside the running TUI.
- `/fork` forks the current chat into a new thread.
- `/compact` summarizes the conversation to reduce context usage.
- `/status` shows the current session configuration and token usage.

## `/resume` and resident assistants

`/resume` is both:

- the saved-session picker for ordinary interactive sessions
- the reconnect entry point for resident assistants

When the selected saved session has `thread.mode = residentAssistant`, the TUI
keeps the same command entry point but presents the action as `reconnect`
instead of a plain resume. The picker title, hints, and row labels all follow
that resident-aware wording.

## Choosing saved sessions

The in-app picker opened by `/resume` follows the same source-filter rules as
the top-level `codex resume` command.

- By default it shows interactive saved sessions.
- Use the picker toggle to include resident assistants and other
  non-interactive sessions.
- Resident rows render with an assistant badge, and runtime state such as
  system error continues to show alongside the reconnect action.

## Related commands

- `/fork` is for branching from the current chat, not for reconnecting to a
  resident assistant.
- `/rename` renames the current thread.
- `/review` runs a review of the current changes.

If you need command-line session selection instead of the in-app picker, use
`codex resume`, `codex resume --last`, or
`codex resume <SESSION_ID_OR_NAME>`.
