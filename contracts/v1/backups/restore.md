# Isolated restore v1

```sh
watchhouse restore --backup /private/backups/verified-snapshot --output /private/restored-watchhouse --manifest-digest sha256:INDEPENDENTLY_RETAINED_HASH --operator operator --reason 'Recover the coordinator into isolation'
watchhouse --db /private/restored-watchhouse/watchhouse.db restore-status
watchhouse --db /private/restored-watchhouse/watchhouse.db serve --manual-workers
```

`restore` does not open the coordinator database selected by `--db`. Both the output directory and its private parent are OS-owned; the parent must already exist. Output must be new and outside the input backup. On Windows, protect the parent ACL. The expected manifest hash is mandatory. Known v1 schema-31 snapshots migrate forward through verified current migrations; newer/changed/unsupported histories fail closed.

The command checks the original backup, copies only bounded declared bytes, verifies that copy again, materializes its files, migrates the private database and commits isolation before removing `restore-incomplete`. A failed directory is retained and cannot start a coordinator or be overwritten. `restore-complete.json` and the original `source-manifest.json` retain provenance. Raw completed/incomplete backup snapshots are inspection-only in their reserved layout; restore them before running a coordinator. This is process-crash recovery, not a universal filesystem power-loss guarantee or protection against a hostile OS administrator.

Records, receipts, external IDs, old event history, budgets and deadlines survive. Source/candidate bytes relocate through immutable location overlays; original immutable records and digests are not rewritten. New backups work after relocation and repeated restore. Evidence counters are recomputed. New restore-review events/owned escalations can advance affected case versions; reload before issuing new commands. Exact old command receipts remain idempotent.

## Authority after restore

A resumable global pause is added, but copied authority also has permanent revision fences. Normal resume cannot remove them. Sources, notifications, model/container workers, publisher, merger, runbooks, repair sources and worker probes need fresh OS-installed revisions. Native execution needs new physical qualification; old admission is cleared. Held model/source status does not check old credential values/auth-home presence.

Old native handles are forensic references. The restored coordinator cannot adopt, observe, revoke their journals or automatically kill them, even on the original host. Pending resources become unknown/quarantined, retain snapshot scope reservations and need new operator inspection. Existing model/container `ReportCleanup` commands accept the operator's own evidence collected after the restore boundary, without touching the original control root. The resulting `reported_cleaned` state remains explicitly reported and grants no capability. Old source facts and context-bound approvals remain stale until their fresh normal validation paths pass.

Retire or independently reconcile the original deployment using its own authority before activating a replacement. A backup may precede an original effect. Unknown publication, merge and runbook jobs therefore require reads of the original stable identity, not another write. Use existing `reconcile-publication`, `reconcile-merge` and `reconcile-operation` commands. Publisher reconciliation grants a five-minute read-only permit, including bounded stage reads/retries; it cannot replay a write under old policy. Read provenance comes from the current backend. Observed stale success stays distinct from current approval, deployment and recovery.

## Notifications and reminders

Old routes stay held and old inflight deliveries become uncertain. Inspect and reconcile them using existing owned evidence commands. Installing a fresh notification revision does not attach old unrouted escalations or resend the old backlog. When a new reminder is appropriate, collect new operator-reported evidence and explicitly submit:

```sh
watchhouse --db /private/restored-watchhouse/watchhouse.db case restore-reminder CASE_ID --file reminder.json
```

```json
{
  "command": {
    "meta": {"schema_version":1,"idempotency_key":"fresh-restored-reminder-001","actor":"operator","reason":"Reviewed possible previous delivery; authorize one new reminder"},
    "expected_version":12
  },
  "escalation_id":"ESCALATION_ID",
  "evidence_refs":["sha256:NEW_OPERATOR_REPORTED_EVIDENCE"]
}
```

The authenticated `POST /api/v1/cases/{id}/restore-reminder` and web versioned-command selector share that service. A fresh notification configuration, accountable case owner/current version and post-restore evidence are required. The new reminder links the old escalation, is idempotent and preserves old duties/uncertainty. It does not automatically resolve an old escalation. After restore, all new outbound sends share a durable interval of at least 30 seconds, including across restarts.

`restore-status`, authenticated factory status, factory history and web health retain restore identity, as-of restore counts, old authority holds and owned next steps. Current queue/uncertainty counts remain separate from the historical restore record. Private paths/bytes are not exposed by a restore HTTP endpoint; creation stays on the OS plane.

Controller fixtures verify these paths with invented data. The explicit Linux actual-Claude/Codex cgroup drills additionally test that a resumed restored coordinator does not disturb an original populated group; their final-head CI results are required independently. Live accounts, original-deployment retirement, source history/mappings/destinations and separate external-effect authority remain operator activation gates.
