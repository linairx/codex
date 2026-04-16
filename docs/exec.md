# Non-interactive mode

`codex exec` is the non-interactive entry point for running Codex from scripts,
CI, or one-off terminal commands without opening the fullscreen TUI.

For the broader product overview, see the external
[non-interactive documentation](https://developers.openai.com/codex/noninteractive).
This page focuses on the local Rust CLI behavior in this repository.

## Common `codex exec` flows

- Start a new non-interactive run:
  `codex exec "summarize the current repository status"`
- Read the prompt from stdin:
  `printf 'review the staged diff' | codex exec -`
- Append stdin after an explicit prompt:
  `git diff --staged | codex exec "review this patch"`
- Resume or reconnect to a saved session by id or name:
  `codex exec resume <SESSION_ID_OR_NAME>`
- Resume or reconnect to the newest recorded session in the current cwd:
  `codex exec resume --last`
- Resume or reconnect to the newest recorded session across all cwds:
  `codex exec resume --last --all`

## Useful flags

- `--json` prints structured JSONL events to stdout instead of the default
  human-readable output.
- `-o, --output-last-message <FILE>` writes the last assistant message to a
  file after completion.
- `--ephemeral` runs without persisting rollout/session files to disk.
- `-C, --cd <DIR>` changes the working root for the run.
- `--image <FILE>` attaches one or more images to the initial prompt, including
  `codex exec resume ...`.

## Resume and reconnect semantics

`codex exec resume` uses the same saved-session model as the interactive CLI.

- Ordinary interactive saved sessions still use the action label `resume`.
- Resident assistant sessions use the same command entry point, but the CLI and
  returned metadata describe that action as `reconnect`.
- When resume lookup already resolves a saved session through `thread/list`,
  exec now reuses the returned `thread.mode` in the outgoing `thread/resume`
  request instead of falling back to server-side inference for known resident
  assistant reconnect targets.
- `SESSION_ID_OR_NAME` accepts either a thread UUID or a saved thread name.
  UUIDs take precedence when a value parses as both.

## Human-readable bootstrap output

In the default mode, bootstrap output is written to stderr and includes
`session mode` plus `session action`.

- Fresh starts and ordinary interactive resume targets show
  `session mode: interactive` and `session action: resume`.
- Resident assistant reconnect targets show
  `session mode: resident assistant` and `session action: reconnect`.

This lets humans distinguish ordinary session resume from resident assistant
reconnect without reading later turn output.

## JSON output

For scripts, use `codex exec --json` or `codex exec resume --json`.

The first `thread.started` event carries the bootstrap `thread_id`, and when
available also includes `thread_mode`.

- Fresh starts emit `thread_mode = interactive`.
- Resident reconnect flows emit `thread_mode = residentAssistant`.
- When the mode is unknown, the field is omitted instead of guessed.

In `--json` mode, this bootstrap metadata is emitted via JSON events, not via
the human-readable stderr summary. Treat that first `thread.started` event as
the authoritative bootstrap summary for the run rather than as a hint that
still requires an extra read to recover reconnect semantics.
