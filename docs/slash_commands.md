# Slash commands

For an overview of Codex CLI slash commands, see [this documentation](https://developers.openai.com/codex/cli/slash-commands).

## Common session commands

- `/new` starts a fresh chat without leaving the current TUI session.
- `/resume` opens the saved-session resume/reconnect picker.
- `/fork` forks the current chat into a new thread.

`/resume` is also the reconnect entry point for resident assistants. When the
selected saved session has `thread.mode = residentAssistant`, the TUI presents
that action as `reconnect` instead of a plain resume.
