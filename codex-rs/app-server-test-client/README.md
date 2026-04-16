# App Server Test Client
Quickstart for running and hitting `codex app-server`.

## Quickstart

Run from `<reporoot>/codex-rs`.

```bash
# 1) Build debug codex binary
cargo build -p codex-cli --bin codex

# 2) Start websocket app-server in background
cargo run -p codex-app-server-test-client -- \
  --codex-bin ./target/debug/codex \
  serve --listen ws://127.0.0.1:4222 --kill

# 3) Call app-server (defaults to ws://127.0.0.1:4222)
cargo run -p codex-app-server-test-client -- model-list
```

## Watching Raw Inbound Traffic

Initialize a connection, then print every inbound JSON-RPC message until you stop it with
`Ctrl+C`:

```bash
cargo run -p codex-app-server-test-client -- watch
```

If you want a closer approximation of a minimal bridge consumer, use
`watch-summary` instead. It bootstraps with one `thread/list` pass plus one
`thread-loaded/read` pass, prints the compact summary view for both, then keeps
streaming resident-aware `thread/started`, `thread/status/changed`, and
`thread/name/updated` summaries until interrupted:

```bash
cargo run -p codex-app-server-test-client -- watch-summary --limit 20
```

## Testing Thread Resume/Reconnect Behavior

Build and start an app server using commands above. The app-server log is written to `/tmp/codex-app-server-test-client/app-server.log`

### 1) Get a thread id

Create at least one thread, then list threads. The list commands now default to
both interactive and non-interactive sources, so newer `Exec` or other
resident non-interactive threads are visible without extra filters:

```bash
cargo run -p codex-app-server-test-client -- send-message-v2 "seed thread for reconnect test"
cargo run -p codex-app-server-test-client -- thread-list --limit 5
cargo run -p codex-app-server-test-client -- thread-list --cursor <NEXT_CURSOR> --limit 5
```

Copy a thread id from the `thread-list` output.

You can also inspect or patch a stored thread directly:

```bash
cargo run -p codex-app-server-test-client -- thread-read <THREAD_ID>
cargo run -p codex-app-server-test-client -- thread-fork <THREAD_ID> --resident
cargo run -p codex-app-server-test-client -- \
  thread-metadata-update <THREAD_ID> --branch feature/resident-mode
```

`thread-fork --resident` now maps to request-side `mode = residentAssistant`
while keeping the old CLI flag for compatibility.
The same explicit request-side mode path now also exists on reconnect flows:
`resume-message-v2 --resident` and `thread-resume --resident` send
`thread/resume` with `mode = residentAssistant` instead of falling back to the
legacy `resident` flag.

These commands print the full response plus a compact summary that includes
wire `thread.mode` values (for example `interactive` or
`residentAssistant`), the current `status`, plus the derived
`resume`/`reconnect` action label. When a thread already has a stored
`thread.name`, that compact summary now prints it too, so resident assistant
reconnect semantics stay visible alongside the user-facing thread label on
read-only lookup, fork, metadata-only update, unarchive, and rollback paths.
For paginated `thread/list` calls, the compact summary also prints
`next_cursor` when present, so you can continue walking history without
re-reading the full debug struct.

If you need to inspect other recovery paths without reading the full debug
struct, the test client now also exposes resident-aware summaries for loaded
threads, archived-thread restore, and rollback responses, with the same compact
`mode + status + action` summary for ordinary interactive resume targets vs
resident reconnect targets. These loaded-thread probes also default
to both interactive and non-interactive sources:

```bash
cargo run -p codex-app-server-test-client -- thread-loaded-list --limit 5
cargo run -p codex-app-server-test-client -- thread-loaded-list --cursor <NEXT_CURSOR> --limit 5
cargo run -p codex-app-server-test-client -- thread-loaded-read --limit 5
cargo run -p codex-app-server-test-client -- thread-loaded-read --cursor <NEXT_CURSOR> --limit 5
cargo run -p codex-app-server-test-client -- thread-unarchive <THREAD_ID>
cargo run -p codex-app-server-test-client -- thread-rollback <THREAD_ID> --num-turns 1
```

`thread-loaded-list` is intentionally an id-only probe. Its compact summary
prints loaded thread ids plus `next_cursor` when present; if you need resident
`mode`, status, or reconnect semantics for those ids, continue with
`thread-loaded-read`.

When you use the streaming start/resume/reconnect commands, `thread/started`
notifications also print the same compact `mode + status + action` summary, so
the notification path stays aligned with the direct response summaries instead
of falling back to plain debug output. Follow-up `thread/status/changed`
notifications now reuse the locally cached thread summary as well: when the
client already knows that thread's `mode`, the status change line keeps the
same compact `mode + status + action` shape; when the thread id is still
unknown, it deliberately stays status-only instead of guessing reconnect
semantics, then immediately follows up with a `thread/read` refresh so later
notifications and summaries can recover the authoritative `mode + status +
action` snapshot. The only exception is `thread/status/changed ->
notLoaded` for an unknown thread id: that unload edge stays status-only so the
client does not issue a noisy refresh read after a matching `thread/closed`
transition. `thread/name/updated` now follows the same rule: known threads
reuse cached mode/status/action context while unknown thread ids stay
identity-only, and `thread/closed` evicts any cached summary after printing the
final compact unload line. That shared notification handling now covers both
turn streaming and long-running observation modes such as `thread-resume` /
`watch`, so the resident-aware summaries stay consistent even when you keep the
client attached just to watch later runtime state changes.

When the test client receives server-side approval or elicitation requests, it
now auto-responds across the full current app-server v2 set instead of only the
legacy command/file subset:

- command execution approvals: accept
- file change approvals: accept
- permission approvals: grant the requested profile with turn scope
- request_user_input: answer each question with its first option when present,
  otherwise an empty string
- MCP elicitation requests: cancel

That lifecycle model also applies if the underlying connection drops instead of
issuing an explicit `thread-unsubscribe`: non-resident threads unload through
the same `thread/status/changed -> notLoaded` plus `thread/closed` transition,
while resident assistants remain available through later `thread-loaded/read`,
`thread-read`, or `thread-resume` probes. When watching those unload
notifications, do not assume `thread/status/changed` and `thread/closed` will
arrive in a fixed order.

### 2) Resume or reconnect while a turn is in progress (two terminals)

Terminal A:

```bash
cargo run --bin codex-app-server-test-client -- \
  resume-message-v2 <THREAD_ID> "respond with thorough docs on the rust core"
```

Terminal B (while Terminal A is still streaming):

```bash
cargo run --bin codex-app-server-test-client -- thread-resume <THREAD_ID>
```
