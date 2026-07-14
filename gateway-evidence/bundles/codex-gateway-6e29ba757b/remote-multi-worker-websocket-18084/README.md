# Gateway Promotion Evidence Bundle

- Gateway build: `codex-gateway-6e29ba757b`
- Topology id: `remote-multi-worker-websocket-18084`
- Captured by: lightweight Python WebSocket client (no local Rust client build)
- Capture window: 2026-07-12T12:36:00Z–2026-07-12T12:36:01Z
- Northbound endpoint: `ws://127.0.0.1:18084/`
- Tenant/project scope: `tenant-evidence-ws` / `project-ws-a`, `project-ws-b`

## Topology

| Worker id | Downstream WebSocket URL | Account id | Health | Capacity |
| --- | --- | --- | --- | --- |
| 0 | `ws://127.0.0.1:19001/v2` | `acct-a` | healthy | available |
| 1 | `ws://127.0.0.1:19002/v2` | `acct-b` | healthy | available |

The gateway ran in `remote` / `workerManaged` / `remoteMultiWorker` mode. The two app-server containers were started by the `remote-multi-worker` Compose profile in an isolated project with host port 18084.

## Evidence Index

| Scenario | Client transcript | Health snapshot | Gateway log | Result |
| --- | --- | --- | --- | --- |
| Empty multi-worker baseline | — | [healthz/01-baseline.json](healthz/01-baseline.json) | — | Two healthy, available workers; zero v2 connections and no project routes. |
| Northbound WebSocket initialize and thread start | [transcripts/02-websocket-thread-start.ndjson](transcripts/02-websocket-thread-start.ndjson) | [healthz/02-websocket-thread-start.json](healthz/02-websocket-thread-start.json), [summary](healthz/02-websocket-thread-start-summary.json) | [logs/02-websocket-thread-start.log](logs/02-websocket-thread-start.log) | Passed: two independent JSON-RPC sessions completed `initialize`, `initialized`, and `thread/start`. |

## Routing Result

The post-traffic health snapshot records both successful routes in the same capture window:

| Project | Thread id | Selected worker | Selected account |
| --- | --- | --- | --- |
| `project-ws-a` | `019f5653-cde7-7892-a3e1-29a0b45fe1ef` | 0 | `acct-a` |
| `project-ws-b` | `019f5653-ce7e-7030-a10e-bdd64994b4fc` | 1 | `acct-b` |

This bundle covers successful northbound WebSocket compatibility and initial project-aware multi-worker assignment only. It does not cover turns, capacity exhaustion, reconnect, restoration, slow-client/backlog, or cleanup behavior.
