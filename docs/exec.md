# Non-interactive mode

For information about non-interactive mode, see [this documentation](https://developers.openai.com/codex/noninteractive).

## Common `codex exec` flows

- Start a new non-interactive run: `codex exec "your prompt"`.
- Resume or reconnect to a saved session by id or name:
  `codex exec resume <SESSION_ID_OR_NAME>`.
- Resume or reconnect to the newest recorded session:
  `codex exec resume --last`.

In the default human-readable mode, bootstrap output includes `session mode`
and `session action`. Ordinary interactive resume flows show `resume`, while
resident assistant reconnect flows show `reconnect`.

For scripts, use `codex exec --json` or `codex exec resume --json`. The first
`thread.started` event carries the bootstrap `thread_id`, and when available
also includes `thread_mode` so downstream consumers can distinguish
`interactive` from `residentAssistant`.
