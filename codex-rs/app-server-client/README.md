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
`thread/metadata/update`: callers should trust the returned `thread.mode`
directly instead of assuming a follow-up `thread/read` is required to recover
resident assistant semantics.
That guarantee now has explicit typed coverage for archived resident threads
too, so the archived metadata-repair path cannot silently fall back to an
ordinary interactive thread summary.
Archived read-only lookup is also locked down directly: after archive, a plain
`thread/read` typed request must still surface `residentAssistant` without
requiring an extra unarchive step.
The in-process typed request tests now lock this boundary down across more
than bootstrap and metadata repair: `thread/list`, `thread/loaded/read`,
`thread/unsubscribe`, and `thread/unarchive` must continue to surface resident
mode for reconnectable assistants instead of dropping back to generic history,
loaded-thread, detached-reader, or restore summaries.
That typed pagination surface is now locked down directly too: when
`thread/list` or `thread/loaded/read` returns `next_cursor`, follow-up typed
requests that continue from that cursor must still preserve resident
`thread.mode` instead of degrading on later pages.
That resident continuity now also covers the post-unsubscribe path directly:
after the last subscriber detaches from a resident thread, follow-up
`thread/loaded/read`, `thread/read`, and `thread/resume` typed requests must
still preserve `residentAssistant` instead of degrading to an ordinary
interactive session.
The same resident continuity is now locked down for `thread/rollback`: after a
resident assistant completes a turn, the rollback response must preserve
`thread.mode = residentAssistant` instead of reconstructing a generic
interactive history summary from the rollout.

## Backpressure and shutdown

- Queues are bounded and use `DEFAULT_IN_PROCESS_CHANNEL_CAPACITY` by default.
- Full queues return explicit overload behavior instead of unbounded growth.
- `shutdown()` performs a bounded graceful shutdown and then aborts if timeout
  is exceeded.

If the client falls behind on event consumption, the worker emits
`InProcessServerEvent::Lagged` and may reject pending server requests so
approval flows do not hang indefinitely behind a saturated queue.
