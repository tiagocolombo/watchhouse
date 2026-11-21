# Local native worker routing v1

Install immutable, consecutive routing revisions from the OS operator CLI:

```sh
watchhouse worker-routing --file routes.json --operator operator
watchhouse worker-routes
watchhouse model-worker --worker-id text-author --once
watchhouse container-worker --worker-id regression --once
```

```json
{"schema_version":1,"revision":1,"workers":[{"id":"text-author","platform":"linux","architecture":"x86_64","paused":false,"targets":[{"kind":"model","policy_revision":1}]},{"id":"regression","platform":"linux","architecture":"x86_64","paused":false,"targets":[{"kind":"container","policy_revision":1}]}]}
```

Targets reference installed immutable execution profiles. Separate profiles can select different harnesses without transferring qualification or private transcripts. Every selected job still passes the existing exact policy/current-source/native admission checks. Routing has no credential fields and cannot grant production, publication or merge authority.

Without a routing policy, the existing unnamed local worker remains compatible. Once installed, only named matching routes admit new worker effects; an empty revision pauses all routes. Selection filters before the queue limit, so unrelated queued profiles cannot starve a matching worker. Each first effect atomically records an immutable worker/platform/architecture binding in the same transaction as its reservation. Another route cannot take over uncertain resources. Original deadlines, owners, attempts, evidence and cleanup budgets are retained.

Pausing/removing a route stops new create/start reservations, including a created resource's start. Read-only observation of an already running model remains available to its original worker. It does not assert an already running process stopped or an external effect failed. Existing deadline supervision remains active and schedules bounded cleanup on the original worker. Cleanup selection uses retained bindings even when current routing is removed or corrupt; its resource identity/protocol checks still apply. OS operator reconciliation remains an explicit inspection/recovery command, not automatic reassignment.

`GET /api/v1/worker-routes` and the web inspector expose current policy/provenance, host compatibility and the latest 100 dispatch bindings. Status is authenticated; configuration remains OS-only. `dispatch_supported_here` describes routing compatibility, not native admission or live model availability.

This is a local shared-database scheduler. It does not authenticate remote workers or make network SQLite sharing safe. Current model/candidate boundaries are implemented for Linux; macOS/Windows targets remain visibly unsupported and cannot dispatch through a Linux substitute. Native macOS/Windows isolation/cancellation adapters and authenticated cross-host transport remain unfinished development, separately tracked in COMPLETION.md. Native coordinator packaging on those OSes is not worker qualification.
