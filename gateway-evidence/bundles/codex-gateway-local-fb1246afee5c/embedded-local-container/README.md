# Gateway Promotion Evidence Bundle

- Gateway build: codex-gateway-local-fb1246afee5c
- Topology id: embedded-local-container
- Worker builds: embedded
- Tenant/project scope: tenant=local, projects=/tmp/project
- Captured by: Codex local Docker verification
- Capture start: 2026-06-27T15:03:00+08:00
- Capture end: 2026-06-27T15:11:12+08:00
- Decision file: decision.md

## Topology

| Worker id | Build id | WebSocket URL | Account id | Auth mode | Notes |
| --- | --- | --- | --- | --- | --- |
| 0 | embedded | embedded://in-process | local | none | |

## Runtime Configuration

- Gateway listen address: 0.0.0.0:8080 in container, published as 127.0.0.1:8080 on host
- Northbound auth: none
- Downstream auth: not applicable for embedded runtime
- v2 initialize timeout: 30
- v2 client send timeout: 10
- v2 reconnect retry backoff: 1
- v2 max pending server requests: 64
- v2 max pending client requests: 64

## Evidence Index

| Scenario | Transcript | Health | Events | Metrics | Logs | Worksheet row |
| --- | --- | --- | --- | --- | --- | --- |
| Baseline before traffic | n/a | healthz/embedded-baseline-before-traffic.json | n/a | metrics/embedded-local-metrics-not-configured.txt | logs/embedded-container.log | Baseline before traffic |
| Steady-state bootstrap and thread/turn | transcripts/embedded-thread-create.json, transcripts/embedded-thread-read.json, transcripts/embedded-thread-list.json, transcripts/embedded-turn-start.txt | healthz/embedded-after-traffic.json | events/embedded-after-traffic.sse | metrics/embedded-local-metrics-not-configured.txt | logs/embedded-container.log | Steady-state bootstrap and thread/turn |
| Project route selection | | | | | | |
| Reconnect and recovery | | | | | | |
| Degraded-route fail-closed | | | | | | |
| Account-capacity transition | | | | | | |
| Bounded restoration | | | | | | |
| Live active-context no-handoff | | | | | | |
| Slow-client or backlog window | | | | | | |
| Cleanup or delivery failure | | | | | | |

## Notes

- Container: `codex-gateway-verify`
- Image: `codex-gateway:local`
- Image digest: `sha256:fb1246afee5c7bed492f6af78af4f18611cdd781db2e56d9ad4a778dd4501ef3`
- Startup command: `docker run -d --name codex-gateway-verify -p 8080:8080 codex-gateway:local`
- Docker health: healthy
- Verified HTTP flows: `GET /healthz`, `POST /v1/threads`, `GET /v1/threads/{thread_id}`, `GET /v1/threads`, `POST /v1/threads/{thread_id}/turns`
- `POST /v1/threads/{thread_id}/turns` returned HTTP 200 with an in-progress turn id, but container logs show repeated OpenAI Responses WebSocket 401 errors because no valid model credentials were configured for this local container.
- `/v1/events` was captured for five seconds after traffic and produced no event bytes for this embedded HTTP flow.
- `/metrics` returned 404 by design; gateway metrics are exported through the configured `codex_otel` pipeline, and no local metrics backend was configured for this container.
