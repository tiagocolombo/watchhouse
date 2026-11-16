# Worker qualification v1

This is a controlled **probe-only** profile, not admission of a real harness or arbitrary repository test. Every record has `scope: hermetic-probe-v1` and `real_execution_admitted: false`, including successful measured probes. Do not reuse a probe pass as Codex/authentication/production authority. Model execution, candidate tests, credentials, publishing, deployment and recovery remain separate work.

## Operator setup

Use a Linux worker with the necessary namespace/mount support and trusted compatible Bubblewrap/prlimit installations. Unsupported/restricted hosts fail closed; there is no root/sudo fallback, shared-network mode, arbitrary-command option or unsafe flag override. macOS/Windows coordinators can retain and supervise records but this adapter reports unsupported there.

Build the fixed trusted payload from [tools/isolation_probe.rs](../../../tools/isolation_probe.rs) on Linux:

```sh
rustc --edition=2024 -C target-feature=+crt-static tools/isolation_probe.rs -o /trusted/isolation-probe
```

The operator creates a private scratch root outside source/frozen-base roots and pins the exact boundary, limiter and probe SHA-256 values in [policy.json](policy.json). Tool paths must be absolute, outside scratch and outside the selected source/base roots. A probe must be static ELF64 little-endian matching the native x86-64/aarch64 architecture; a dynamic/oversized/unrecognized payload is not invoked. These are operator-trusted installed tools, not candidate-selected binaries. The operator must review and own them; a malicious host administrator/tool installation remains outside the boundary.

```sh
watchhouse worker-policy --file /trusted/worker-policy-v1.json --operator operator
watchhouse worker-policy-status
```

Policy revisions are immutable, checksummed, consecutive and retain installation identity/time. Agents and HTTP cannot install them. Limits are explicit: wall 2–30s, CPU 1–10s and no greater than wall, virtual address space 64–512MiB, file size 4KiB–1MiB, descriptors 16–256, and proof currency 1–3600s. A new attempt must use the active policy; already-reserved attempts keep their pinned policy and history. Identical installation is idempotent; rollback/conflicting bytes are rejected.

## Reserve and run a controlled attempt

Prepare a [repair source](../repair/README.md) first. `case qualify-worker CASE_ID --file FILE` is an OS-operator CLI operation; no HTTP execution endpoint exists. Command:

```json
{
 "command":{"meta":{"schema_version":1,"idempotency_key":"qualification-1","actor":"operator","reason":"Measure this worker using the trusted probe"},"expected_version":5},
 "policy_revision":1,
 "source_revision":1
}
```

The accountable operator owns the exact-version case, with no blocking challenge. Before I/O, one SQLite transaction freezes the source manifest, active worker policy and case policy provenance, requester/reason, owner, fresh nonce and deadline; it records `probing`, its event and a receipt pointing to the attempt. A retry reads the current attempt and never launches another probe, even after process loss. Changed bytes under the same key conflict. Concurrent stale versions fail explicitly.

The Linux adapter checks exact tool pins before invocation. It constructs fixed argv arrays through prlimit/Bubblewrap: explicit private user/PID/network namespaces, disabled further user namespaces, dropped capabilities, new session, parent-death protection, cleared environment, fresh `/proc` and `/dev`, read-only input/policy/probe, one writable scratch directory and bounded `/tmp`. The host root, source Git metadata, real credentials and candidate code are never mounted. Only fresh synthetic canaries and the trusted payload are provided. The command has no shell/task-text interpolation. Kernel observations are parsed under a strict version/nonce schema and bounded output/deadline; flags alone are not successful evidence.

Measured checks cover:

- Input readability, read-only input/policy, hidden host canary, empty credential environment and allowed scratch writes, with independent host-side content checks.
- A live host loopback canary that must be unreachable, plus distinct kernel network/PID namespace identities.
- Observed per-process CPU/address-space/file/descriptor/core limits. These are **not** cgroup/aggregate quotas, an RSS guarantee, a fork-bomb limit or a monetary budget.
- A known descendant whose heartbeat must advance before cancellation and stop after the launcher is killed/reaped. This is a controlled termination drill, not proof of every fatal coordinator/OS failure or arbitrary malicious process behavior.
- Tool pins rechecked after observations. A changed/missing pin invalidates the result.

Only the complete measured native result can produce `probe_passed`; missing/malformed/failed checks cannot. Fixture-labelled outcomes stay `simulated` with no verified capabilities. Standard controller tests also use private synthetic native-shaped data to exercise acceptance/currency branches; they are not live isolation evidence. The fixed payload is statically compiled in Linux CI, but ordinary CI does not claim a successful native namespace drill.

Normalized observations, host confirmations, origin, outcome and raw-output digest are retained as case-bound content-addressed evidence, bound to the attempt ID, nonce, policy and source-manifest digest. Failure evidence states what failed/was unavailable without inventing measurements. The UI links the retrievable evidence. `time_current` is computed at read time; use Refresh worker status to observe idle expiry/policy changes without a case event. It means only that the pass is within its TTL under the active policy; it does not revalidate installed binaries, host identity or grant admission. Future use must recheck all relevant identities/scopes.

## Loss, quarantine and reconciliation

The existing model-independent supervisor expires overdue `probing` attempts once into `interrupted`, marks resources quarantined and creates an owned escalation. Process state is uncertain, not declared cancelled. Late output is retained once as unaccepted evidence; it cannot restore expired proof. Terminal transaction failure leaves the prior durable attempt for supervision; retries do not repeat I/O. No canonical writer lock is held while running a probe.

Failed/uncertain cancellation or setup can quarantine the private `qualification-NONCE-*` directory under the configured scratch root. Do not infer no remaining processes merely from timeout, a missing PID or a recycled PID. The operator must inspect/reconcile resources on that worker; no automatic arbitrary-PID kill or filesystem deletion is exposed. A fresh attempt for the case is blocked while any earlier quarantine remains unresolved, including attempts beyond the snapshot display cap.

Record case-bound **operator-reported** inspection evidence through the existing evidence command. Then use `case reconcile-worker-qualification CASE_ID --file FILE` or `POST .../worker-qualification/reconcile` with:

```json
{
 "command":{"meta":{"schema_version":1,"idempotency_key":"qualification-cleanup-1","actor":"operator","reason":"Inspected the attempt resources and recorded cleanup evidence"},"expected_version":12},
 "qualification_id":"REPLACE_WITH_ATTEMPT_ID",
 "evidence_refs":["REPLACE_WITH_CASE_OPERATOR_REPORT_DIGEST"]
}
```

Only the current accountable operator can reconcile terminal quarantined resources, against the exact case version. Reconciliation is an attributed report, not native isolation verification: it clears the quarantine gate, preserves failed/interrupted outcome and history, adds no capabilities, runs/deletes nothing and resolves no escalation/repair automatically. It commits with its audit event and receipt; wrong/foreign evidence, live attempts and stale commands fail. A fresh explicitly requested qualification is still required. Late old output stays unaccepted after reconciliation.

CLI/API/web snapshots expose the latest 100 attempts with owner, state, scope, evidence, TTL currency, quarantine and reconciliation. Full events remain authoritative. `case worker-qualification-markdown CASE_ID` / `GET .../worker-qualification/markdown` render escaped structured records. Native process invocation and policy installation remain CLI-only; read/report/reconciliation routes do not grant execution.

## Actual local activation result

The installed Bubblewrap setup test on the development host failed at NETLINK_ROUTE setup with Operation not permitted. The product's pinned static-payload attempt also returned `failed`, no verified capabilities, `real_execution_admitted: false`, owned escalation and quarantined scratch. No successful live containment pass is claimed. No host restriction was disabled or bypassed. Run the controlled drill on a properly provisioned worker before claiming physical verification.

Before general execution: qualify the actual worker/runtime profile, stronger hostile-code/process/FD/seccomp/cgroup and fatal-loss behavior, filesystem sealing, scoped credentials and revocation, and a real start/events/cancel/result/reconcile adapter. Only then proceed to independently checked base/candidate regression, repair/review and PR delivery. Neither this profile nor a CLI-help advertisement satisfies those M4 gates.
