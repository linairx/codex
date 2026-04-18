# codex-app-server-client

Shared in-process app-server client used by conversational CLI surfaces:

- `codex-exec`
- `codex-tui`

## Purpose

This crate centralizes startup and lifecycle management for an in-process
`codex-app-server` runtime, so CLI clients do not need to duplicate:

- app-server bootstrap and initialize handshake
- in-memory request/event transport wiring
- lifecycle orchestration around caller-provided startup identity
- graceful shutdown behavior

## Startup identity

Callers pass both the app-server `SessionSource` and the initialize
`client_info.name` explicitly when starting the facade.

That keeps thread metadata (for example in `thread/list` and `thread/read`,
including `thread.mode` for resident assistant reconnect flows)
aligned with the originating runtime without baking TUI/exec-specific policy
into the shared client layer.

## Transport model

The in-process path uses typed channels:

- client -> server: `ClientRequest` / `ClientNotification`
- server -> client: `InProcessServerEvent`
  - `ServerRequest`
  - `ServerNotification`
  - `LegacyNotification`

JSON serialization is still used at external transport boundaries
(stdio/websocket), but the in-process hot path is typed.

Typed requests still receive app-server responses through the JSON-RPC
result envelope internally. That is intentional: the in-process path is
meant to preserve app-server semantics while removing the process
boundary, not to introduce a second response contract.

## Bootstrap behavior

The client facade starts an already-initialized in-process runtime, but
thread bootstrap still follows normal app-server flow:

- caller sends `thread/start` or `thread/resume` (`resume or reconnect`)
- app-server returns the immediate typed response
- richer session metadata may arrive later as a `SessionConfigured`
  legacy event

For resident assistants, that `thread/resume` bootstrap step is also the
reconnect path. Callers should therefore treat the immediate typed response as
the authoritative `resume or reconnect` bootstrap summary rather than assuming
all reconnect semantics arrive later via legacy events.

Surfaces such as TUI and exec may therefore need a short bootstrap
phase where they reconcile startup response data with later events.
Callers that need to distinguish an ordinary interactive resume target from resident reconnect should
consume `response.thread.mode` from the immediate typed response instead of
waiting for later legacy events.
The same rule applies to metadata-only thread operations such as
`thread/metadata/update`: callers should trust the returned thread identity
metadata directly, including `thread.mode` for reconnect semantics and
`thread.name` for user-visible labels, instead of assuming a follow-up
`thread/read` is required to recover them.
That guarantee now has explicit typed coverage for archived resident threads
too, so the archived metadata-repair path cannot silently fall back to an
ordinary interactive resume target summary.
Archived read-only lookup is also locked down directly: after archive, a plain
`thread/read` typed request must still surface `residentAssistant` without
requiring an extra unarchive step.
The in-process typed request tests now lock this boundary down across more
than bootstrap and metadata repair: `thread/list`, `thread/loaded/read`,
`thread/unsubscribe`, and `thread/unarchive` must continue to surface resident
mode for reconnectable assistants instead of dropping back to generic history,
loaded-thread, detached-reader, or restore summaries.
The same typed test layer now also locks down `thread/loaded/list` as the
id-only loaded probe: typed callers must keep `next_cursor` continuity across
pages without treating loaded ids as a substitute for `thread.mode` or current
status, and should continue with `thread/loaded/read` whenever reconnect
semantics or live loaded-thread status are needed.
That typed pagination surface is now locked down directly too: when
`thread/list` or `thread/loaded/read` returns `next_cursor`, follow-up typed
requests that continue from that cursor must still preserve resident
`thread.mode` instead of degrading on later pages.
That resident continuity now also covers the post-unsubscribe path directly:
after the last subscriber detaches from a resident thread, follow-up
`thread/loaded/read`, `thread/read`, and `thread/resume` typed requests must
still preserve `residentAssistant` instead of degrading to an ordinary
interactive resume target.
The same client-side contract should be applied after the last transport
connection drops unexpectedly: if the server keeps a resident assistant loaded,
typed follow-up `thread/loaded/read`, `thread/read`, and `thread/resume`
responses should still be treated as the authoritative reconnect summary
surface, while non-resident threads may disappear behind the matching
`thread/status/changed -> notLoaded` plus `thread/closed` unload transition.
Callers should not depend on a fixed ordering between those two notifications.
The websocket-backed remote facade now has direct regression coverage for both
arrival orders too.
That reconnect-summary rule now also explicitly covers the resident active
flags that matter for long-lived runtime continuity: follow-up typed reads and
resume calls should preserve `waitingOnApproval`, `waitingOnUserInput`,
`backgroundTerminalRunning`, and `workspaceChanged` when those facts remain
true after unsubscribe or transport disconnect, rather than flattening the
thread back to `idle`. The remote typed layer now also locks this down across a
real follow-up polling sequence: `thread/loaded/read`, `thread/read`, and
`thread/resume` must all preserve the same resident reconnect summary instead
of drifting between “loaded view”, “stored view”, and “reconnect view”. The
remote follow-up sequence coverage now also explicitly includes the resident
approval/input wait states: `thread/read` plus `thread/resume` must preserve
`waitingOnApproval` and `waitingOnUserInput` instead of collapsing those
threads back to generic idle history. The same remote sequence coverage now
also extends the observer/background flags across the full
`thread/loaded/read -> thread/read -> thread/resume` path, so
`backgroundTerminalRunning` and `workspaceChanged` do not drift between
loaded-thread polling, stored lookup, and reconnect.
The same resident continuity is now locked down for `thread/rollback`: after a
resident assistant completes a turn, the rollback response must preserve
`thread.mode = residentAssistant` instead of reconstructing the rollout as an
ordinary interactive resume target summary.
That same rollback boundary now also covers stored-summary repair: if a
resident thread already has a persisted SQLite row but rollout-derived preview
data is missing, typed `thread/rollback` should still return the reconciled
thread snapshot directly instead of preserving only `thread.mode` and forcing a
follow-up `thread/read` just to recover preview.
The same typed boundary now also covers stored-summary repair: if a thread
already has a persisted SQLite row but rollout-derived summary fields such as
preview / first-user-message are still missing, follow-up `thread/read`,
`thread/list`, `thread/resume`, `thread/metadata/update`, and `thread/unarchive`
responses should continue to come back already reconciled. Typed callers should
therefore treat the returned thread snapshot as authoritative instead of
assuming they need an extra `thread/read` round-trip to repair missing preview
data after restore / metadata / reconnect flows.
The same contract now applies to the websocket-backed remote facade too: when
the remote app-server already returns a repaired thread summary, the typed
remote client should preserve `thread.mode`, preview, status, path, and name
exactly as returned instead of reintroducing a client-side “resume, then
re-read” repair step. That boundary is now locked down directly for
`thread/resume`, `thread/read`, `thread/rollback`, `thread/metadata/update`,
`thread/unarchive`, `thread/list`, and `thread/loaded/read`, so remote callers
can continue to treat each of those responses as the authoritative repaired
snapshot when reconnect, rollback, metadata repair, restore, stored lookup,
history listing, or loaded-thread polling returns an already repaired
`Thread`.
The same websocket path now also locks down request-side mode selection:
typed remote callers should prefer `mode` on `thread/start`, `thread/resume`,
and `thread/fork`, and should not expect the remote facade to quietly
reintroduce the legacy `resident` flag when `mode = residentAssistant` is
already explicit at the callsite.
The same remote typed layer now also locks down `thread/loaded/list` as the
id-only loaded probe: websocket callers should preserve loaded ids plus
`next_cursor` exactly as returned, and should continue with
`thread/loaded/read` whenever reconnect semantics or current loaded-thread
status are needed.
That same websocket-backed typed facade now has direct regression coverage for
all four resident continuity flags too: `thread/resume` preserves
`waitingOnApproval` and `waitingOnUserInput`, while `thread/loaded/read`
preserves `backgroundTerminalRunning` and `workspaceChanged` exactly as the
remote app-server returned them.
The event stream is now locked down at the same boundary too: in-process
`next_event()` consumers can rely on `thread/started` as the resident-aware
snapshot that still carries `thread.mode`, while later
`thread/status/changed` notifications continue to update only runtime
`status`. Callers that need reconnect semantics across both events should
therefore retain the previously observed `mode` instead of expecting the
status-only increment to repeat it. The websocket-backed remote facade now has
the same regression coverage, so remote `next_event()` consumers can keep the
same “mode from started, status from changed” contract. The same event-stream
boundary now also explicitly covers the other lifecycle increments: remote
`next_event()` consumers should continue to treat `thread/closed`,
`thread/archived`, and `thread/unarchived` as lifecycle edges rather than
replacement thread snapshots, and should treat `thread/name/updated` as a
name-only increment rather than as a summary refresh that repeats
`thread.mode`, `status`, or repaired preview/path fields. In other words, the
typed event stream should still be consumed as:

- `thread/started` introduces the authoritative thread snapshot
- `thread/status/changed` updates only runtime `status`
- `thread/name/updated` updates only `name`
- `thread/closed` / `thread/archived` / `thread/unarchived` describe lifecycle
  edges on that retained thread summary

If a caller wants that retained-summary contract as reusable code instead of
re-implementing it per surface, this crate now also exposes
`ThreadSummaryTracker`. It bootstraps from paginated `thread/list` /
`thread/loaded/read`, retains the last authoritative `Thread` per id, applies
incremental lifecycle notifications on top of that cache, and reports when a
bridge-style consumer should explicitly refresh via `thread/read`. The helper
also exposes single-page `fetch_thread_list_page(...)` /
`fetch_thread_loaded_read_page(...)` helpers, a `bootstrap_all_pages(...)`
entrypoint, `apply_event` for direct consumption of `AppServerEvent`, plus an
async `track_event(...)` helper that applies lifecycle edges and performs the
follow-up `thread/read` refresh when the edge is identity-only, so
remote-control or bridge-style surfaces can keep one retained-summary state
machine instead of re-deriving the same
`thread/list + thread/loaded/read + lifecycle notification` contract per
client. That single-page API is intended for consumers that still want to
render or log each paginated response as it arrives while keeping the retained
cache update in the shared helper layer rather than hand-rolling per-page
`upsert_thread(...)` loops. `track_event(...)` returns a structured
`ThreadSummaryTrackedEvent` so bridge-style consumers can keep presentation or
logging policy outside the shared state machine while still reusing the common
refresh semantics. Approval and elicitation server requests now also reuse that
same retained-summary context: the tracked server-request event includes the
request thread id plus the cached authoritative `Thread` summary when the
consumer already knows that thread, so bridge-style control surfaces can keep
approval prompts aligned with reconnect/action semantics instead of treating
them as detached raw requests. For callers that want the next layer up, the crate also now
exposes `ThreadSummaryClient`: it wraps an `AppServerClient`, keeps one shared
`ThreadSummaryTracker`, forwards bootstrap reads through the same retained-
summary cache, and turns `next_event()` into `next_tracked_event()` so remote
or bridge-style control surfaces do not need to wire `request_handle()` +
`track_event(...)` loops themselves. That wrapper now also forwards
`resolve_server_request(...)`, `resolve_server_request_typed(...)`, and
`reject_server_request(...)` through the same shared request handle, so a
bridge-style consumer can bootstrap summaries, consume tracked approval /
elicitation requests, and send the matching remote resolution without dropping
down to the raw transport client. For the current “minimal bridge” policy, it
also exposes `resolve_default_bridge_server_request(...)`, which auto-accepts
the existing approval request set, auto-answers `request_user_input` with the
first option (or empty fallback), auto-cancels MCP elicitation, and explicitly
rejects unsupported server requests. For callers that want that policy wired
directly into the event loop, the crate now also exposes
`ThreadSummaryBridgeClient`: it wraps `ThreadSummaryClient`, keeps the same
retained-summary bootstrap surface, and turns `next_tracked_event()` plus the
default bridge request resolver into one `next_bridge_event()` stream so a
bridge-style control surface can consume tracked thread events and approval
resolutions from a single facade.

## Backpressure and shutdown

- Queues are bounded and use `DEFAULT_IN_PROCESS_CHANNEL_CAPACITY` by default.
- Full queues return explicit overload behavior instead of unbounded growth.
- `shutdown()` performs a bounded graceful shutdown and then aborts if timeout
  is exceeded.

If the client falls behind on event consumption, the worker emits
`InProcessServerEvent::Lagged` and may reject pending server requests so
approval flows do not hang indefinitely behind a saturated queue.
