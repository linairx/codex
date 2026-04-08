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

These commands print the full response plus a compact summary that includes
wire `thread.mode` values (for example `interactive` or
`residentAssistant`) plus the derived `resume`/`reconnect` action label, so
resident assistant reconnect semantics remain visible on read-only lookup,
fork, and metadata-only update paths.
For paginated `thread/list` calls, the compact summary also prints
`next_cursor` when present, so you can continue walking history without
re-reading the full debug struct.

If you need to inspect other recovery paths without reading the full debug
struct, the test client now also exposes resident-aware summaries for loaded
threads, archived-thread restore, and rollback responses, with the same compact
`mode` plus `resume`/`reconnect` action labels for ordinary interactive resume
targets vs resident reconnect targets. These loaded-thread probes also default
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
`mode` or reconnect semantics for those ids, continue with `thread-loaded-read`.

When you use the streaming start/resume/reconnect commands, `thread/started`
notifications also print the same compact resident-aware summary, so the
notification path stays aligned with the direct response summaries instead of
falling back to plain debug output.

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
