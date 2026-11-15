# Repair preparation v1 — M4 starts here

This slice prepares trusted frozen inputs and records **denied** real-execution admission. It does not admit a real worker, run a model/build/test, produce a candidate or publish a PR. All repair requests currently remain blocked with accountable escalation. A copy of a repository is not a security sandbox.

## OS operator control plane

Replace the explicit absolute paths, repository assertion, full Git commit and pinned runtime SHA-256 in [config.json](config.json). Create the source and snapshot-root directories yourself; they must not overlap. The runtime must be outside both. Then run:

```sh
watchhouse repair-source --file /trusted/repair-source-v1.json --operator operator
watchhouse repair-source-status
```

There is no HTTP/worker installation endpoint. Revisions start at one and advance consecutively. Config/manifest/probe records are immutable, checksummed and retain operator/time. An identical install returns its original record; it does not refresh a probe or repair a lost snapshot. Use a new revision for changed inputs, binary or recreated snapshots. Local actor labels are attribution under the trusted OS/operator-token boundary, not remote authentication. The repository name is an explicit operator mapping, not independent verification of GitHub ownership or publication authority.

The source reader uses bounded, fixed object-level Git commands (`rev-parse`, `ls-tree`, `cat-file`) against a full commit. No checkout, shell command construction, filter, test or source hook is invoked. Caller Git configuration/environment, authentication/credential variables and user home are not inherited; missing-object lazy fetching and replacement refs are disabled. The Git build must support `--no-lazy-fetch` (unsupported versions fail closed). Source repository config remains untrusted data; trusted installed Git/OS tools remain part of this operator boundary.

Only regular tracked blobs are copied into a new private base directory. The commit, every tree and every blob are independently checked against their Git object identities. Every copied file additionally receives a SHA-256 digest; the commit, tree, path/mode/size list, total bytes and manifest digest are frozen. No `.git` metadata, untracked files, symlinks or submodules are copied. Conservative portable path rules reject traversal, metadata aliases, Windows device/ADS names and case collisions. Binary bytes stay exact. Git executable mode is retained as metadata, not executable authority. Read-only file permissions are an integrity convention; a hostile host process with operator access can still mutate them. Requests detect missing/extra/altered files. Neither this copy nor its permissions establish worker isolation.

Limits are explicit trusted configuration: 1–5000 files, 1–8MiB per blob, total at least the per-blob limit and at most 128MiB; preparation 1–60 seconds, version/help probe 1–10 seconds. Git/probe output is bounded and deadline-limited; stderr is discarded. File hashing/copying has byte bounds and verification has entry/time bounds, but there is no hard CPU/memory quota or claim of process-tree cancellation. Runtime executable hashing is capped at 512MiB. Crash-created unregistered directories remain quarantined by absence of a canonical record; automatic garbage collection/backup/revocation remain future work. A failed metadata transaction exposes no accepted source; only a committed record retains its directory.

The pinned runtime is invoked only as `--version` and `exec --help` from a fresh directory/home/config with no inherited account secrets. A pin mismatch is not invoked. Version/help-output digests and a bounded list of advertised flags are retained. These are **advertisements**: `verified` stays empty. No authentication/subscription, actual model, usage, filesystem/network isolation, process-tree cancellation or session protocol has been verified. A missing/unrecognized binary leaves a visible probe error without authorizing execution.

## Case request

`case request-repair CASE_ID --file FILE` / `POST /api/v1/cases/CASE_ID/repairs` / the web versioned command form:

```json
{
  "command":{"meta":{"schema_version":1,"idempotency_key":"repair-1","actor":"operator","reason":"Prepare a scoped fixture repair"},"expected_version":5},
  "source_revision":1,
  "objective":"Correct the reported fixture behavior",
  "expected_behavior":"Fixture add(2,2) returns 4",
  "regression_plan":"Demonstrate failure at pinned base and success at independently checked frozen candidate",
  "evidence_refs":["REPLACE_WITH_CASE_OPERATOR_REPORTED_EVIDENCE"]
}
```

Only the accountable trusted operator may request preparation, against the exact case version, case-bound operator-reported evidence and no blocking challenge. A source-observed case additionally requires the explicit repository and deployed revision to match the configured frozen base exactly; unknown/mismatched mappings fail closed. A manual case relies on the operator's explicit scope assertion. Unknown command fields/versions are rejected. No agent-authored objective or test plan can create execution authority.

The request freezes source/config/manifest/base/tree identities, the case's governing policy provenance, reported expectation/regression proposal and owner. It creates a durable blocked record, a case event and an owned `repair_admission` escalation in one transaction. Exact retries return the original receipt; competing versions conflict. Filesystem verification occurs outside the canonical writer lock; the transaction rechecks case ownership/version and immutable source provenance. Config changes apply only to new explicitly versioned requests. A lost/changed base is visible as an additional denial. No request enters the fake-worker queue; fake completion cannot fulfill it. Acknowledging/resolving an escalation does not execute or complete a repair, and new requests do not resolve old duties.

Snapshots expose the newest 100 requests; full history remains in events. The web displays owner, pinned base, denials and next action. `case repair-markdown CASE_ID` / `GET .../repairs/markdown` render escaped structured preparation records. None of these is a check attestation, candidate approval or recovery assessment.

## Next boundary

A real isolated worker must prove filesystem/network restrictions, candidate-independent policy/briefings, scoped credential injection/revocation, bounded resources, process-tree cancellation and loss reconciliation before an adapter can be admitted. Then implement start/events/cancel/result/reconcile, regression-on-base/candidate, independent review and PR effect reconciliation. Codex remains the first installed-version candidate; there is no unsupported capability override or generic arbitrary-command executor in this slice.
