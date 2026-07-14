# Gateway Multi-Worker Evidence Worksheet

| Scenario | Expected evidence | Observed evidence | Result |
| --- | --- | --- | --- |
| Empty baseline | Remote multi-worker health with no previous northbound traffic | `totalConnectionCount=0`, empty request counts and routes, workers 0/1 healthy and available | Pass |
| Protocol bootstrap | A non-Rust client can complete the actual northbound JSON-RPC handshake | Each session received an `initialize` response, then sent `initialized` | Pass |
| Project-aware initial routing | Each new project receives a stable worker/account assignment | `project-ws-a` mapped to 0/`acct-a`; `project-ws-b` mapped to 1/`acct-b` in the post-traffic health snapshot | Pass |
| Thread admission | The routed worker accepts `thread/start` | Both responses contain idle app-server threads with the recorded IDs | Pass |

## Limits

The capture intentionally used only `thread/start`, so it establishes admission and initial routing rather than model execution. No conclusion is made about turn completion or fault paths.
