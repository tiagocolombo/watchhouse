# Synthetic adapter lifecycle v1

Implemented CLI/API protocol conformance trial; **not a real harness adapter or repair execution**. It operates against an existing immutable repair request. No source checkout, arbitrary executable, credential, model call, network request, candidate, regression test or PR is created. Existing repair requests remain blocked and worker quarantine is unchanged.

## Commands

```sh
watchhouse case adapter-trial CASE_ID --file trial.json
watchhouse case adapter-trial-status CASE_ID --trial TRIAL_ID
watchhouse case adapter-trial-events CASE_ID --trial TRIAL_ID --after 0
```

POST `/api/v1/cases/{case}/adapter-trials` uses the same command. Authenticated GET `/api/v1/cases/{case}/adapter-trials/{trial}` returns the current state, frozen envelope and optional result/evidence. GET its `/events?after=N` child returns ordered normalized events after that per-trial sequence, at most 200. Pagination never advances the simulator. Negative cursors are rejected; a cursor beyond the tail returns an empty list. Status lookup works independently of the newest-100 snapshot display cap.

Start from [start.json](start.json). Replace the case version, repair ID and idempotency key:

```json
{
  "command": {
    "meta": {
      "schema_version": 1,
      "idempotency_key": "unique-start-key",
      "actor": "operator",
      "reason": "Exercise synthetic protocol recovery"
    },
    "expected_version": 12
  },
  "action": {
    "type": "start",
    "repair_id": "repair_ID",
    "scenario": "start_unknown",
    "wall_seconds": 120
  }
}
```

Subsequent commands replace `action` with `{"type":"reconcile","trial_id":"trial_ID"}`, `{"type":"advance","trial_id":"trial_ID"}` or `{"type":"cancel","trial_id":"trial_ID"}`. Each new command needs the latest expected case version and a new key. Same-key retries inspect the current trial rather than replaying a start or returning obsolete state. Changed payloads cannot reuse the key. Only the current case owner can issue new commands; fake worker identities cannot. This is local single-operator attribution, not multi-user RBAC.

`start` freezes repair/source/manifest/base, objective/expected behavior/regression plan, evidence references, owner, governing case policy, attempt, fresh fence, and absolute deadline into a digested envelope. These are context references, not fresh source/host validation. The case-pinned policy bounds wall time and total trials **per repair request**. Only one running or unknown trial per request is allowed. This simulation budget is not a global real-execution or monetary budget.

## Scenarios and results

| Scenario | Observation |
| --- | --- |
| `complete` | Explicit `advance` yields a schema/fence-valid synthetic result |
| `invalid_output` | Exit-success-shaped output fails the role schema; raw synthetic output is retained as evidence |
| `quota` | Structured fake quota failure; owned escalation and no automatic retry |
| `stall` | Polls cannot extend the deadline; supervisor cancels the fake session and records timeout |
| `start_unknown` | Backend session exists but start acknowledgment is lost; reconcile the existing handle before advance/cancel |
| `cancel_unknown` | Cancellation occurs but acknowledgment is lost; explicit reconciliation observes cancelled |

States: `running`, `unknown`, `completed`, `failed`, `cancelled`, `timed_out`. Failures expose structured `quota`, `invalid_output`, `timeout`, `cancelled` or `unknown_outcome` codes as applicable. A lost acknowledgment remains unknown. An absent fake backend record does not establish that a start/effect never happened. Reconciliation never repeats start/cancel and does not automatically resolve escalation. Deadline expiry after an unknown start is observed running immediately cancels/times it out. Timers and recovery require no model.

Every trial is `scope: fake-adapter-protocol-v1`, `adapter: deterministic-fake-v1`, `real_execution_admitted: false`. Even a completed result is synthetic, has unknown model/usage (`null`), and establishes no defect reproduction, code fix, independent verification or service recovery. Output must match schema v1, trial ID and fence and must explicitly be synthetic; unknown fields, provider/model claims and stale results fail validation. Case-bound content-addressed evidence uses `adapter-trial-synthetic-v1` provenance. The web panel and ordinary timeline preserve these distinctions; imported text remains inert.

## Persistence boundary and limits

Migration 014 stores coordinator trials/events and a separate **fake** backend journal. Start/status/result/cancel/reconciliation changes, case audit, evidence and command receipt commit atomically. Restart can reload the handle and recover lost-response scenarios. Fake effects are SQLite writes and roll back with the command. This does **not** prove a real process/network effect can participate in a database transaction.

A production adapter still needs reserve-before-I/O dispatch, installed-version capability admission, scoped worker/credential authorization and revocation, independent external journal/reconciliation, bounded streaming/usage, process-tree cancellation and fatal-loss drills. This slice does not register a real runtime or replace the existing investigation FakeAdapter with a production interface. See [ADR 0017](../../../implementation/adrs/0017-durable-adapter-trials.md) and [STATUS](../../../implementation/STATUS.md).
