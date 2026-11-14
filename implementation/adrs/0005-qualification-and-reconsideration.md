# ADR 0005 — Evidence-backed qualification and durable reconsideration

Date: 2026-11-14. Status: accepted for the remaining M1 slice.

## Decision

Keep qualification separate from execution and incident recovery. The case execution status remains queued/running/awaiting_review; an operator can record expected_behavior, apparently_transient, or insufficient_evidence only after investigation finishes. A separate disposition lifecycle is active → reopened → superseded. No classification or reopening grants repair, production, deployment, or recovery authority.

Each immutable decision includes actor/reason, accountable owner, expected case version and command key, applicability scope, uncertainty, case-bound evidence digests, reconsideration conditions, a recurrence threshold, and a typed plan. Expected behavior schedules review; apparent transience requires recovery indicators and a monitoring window; insufficient evidence requires a concrete collection task and deadline. New decisions explicitly supersede the old decision and obligation without erasing history or claiming the old work was completed. Case ownership changes to the recorded accountable owner; the completed investigator assignment keeps its original attribution.

Add operator-reported evidence with attributed statement, scope, collection method, optional observed time, and uncertainty. This is a human report, not an independently observed check. Synthetic fake output can support an insufficient-evidence record but cannot alone justify expected/transient decisions. Evidence must belong to the case and its digest must match its bytes. Neither status codes nor the absence of events is automatically classified as expected/recovered.

## Reconsideration and time

Explicit operator commands report recurrence, contradictory evidence, scope change, or a request for review, bound to the current disposition ID and expected aggregate version. A recurrence needs new operator-reported evidence recorded after this decision, not evidence already cited by it or counted in a previous recurrence; multiple references in one command still count as one occurrence. This remains an attributed operator assertion, not automated source correlation. Recurrence reopens at its configured threshold; other triggers reopen immediately. Replaying the same command returns its original receipt; an attempted recount under a new key conflicts.

Every decision atomically creates one owned, persisted review/monitoring/collection obligation. Durations are explicitly configured per command, bounded to 1 second–30 days for this local milestone; these are engineering bounds, not company severity defaults. At the deadline, a deterministic tick atomically reopens the decision, makes the obligation due, and appends events. The original deadline is retained; triggered_at explains early reopening. Being due never claims completion, recovery, notification delivery, or escalation acknowledgment. A fresh operator decision supersedes the outstanding obligation.

`serve` and `worker` tick before claiming fake work; `tick` processes up to 100 due obligations without invoking any executor. No in-memory timer is authoritative. The injected clock drives deadlines in tests. A stopped coordinator catches up when restarted. SQLite serialization makes competing ticks harmless and stale operator writes explicit. Clock jumps, automatic human notifications, full overload control, and the M2 supervisor remain separate work.

## Storage and compatibility

Migration 002 adds disposition records, obligations, case/evidence links, and recurrence evidence deduplication. It backfills existing fake-result evidence links without editing migration 001, historical events, or command receipts. Startup validates complete applied migration history before applying pending migrations; reject gaps, modified checksums, and future schemas. New checksums normalize LF; exact legacy CRLF checksums are accepted without changing stored migration history.

New command/event families use explicit v1 contracts. The unreleased local v1 snapshot gains an additive qualification projection with a default for reading old snapshots. Existing Case fields, v1 fixture envelopes, and receipts remain unchanged; unknown commands/versions still fail closed. Old binaries refuse database schema 2 rather than trying to operate it. Newly added event kinds require a current reader; no forward-compatibility claim is made for old binaries.

Both HTTP and CLI call the same application services. The web UI displays evidence provenance, current/prior decisions, uncertainty, scope, thresholds, deadlines, and the next accountable obligation, with the existing replayable timeline. A draft captures expected_version when editing begins and retains the exact command key/version after an uncertain network response; live snapshot updates cannot authorize an old draft. General disposition categories, automated source matching, formal incident artifacts, challenge/verification, and team escalation stay in their later milestones.

## Evidence and alternatives

SQLite transactions are sufficient for this local slice; a separate timer service or model scheduler would add another failure boundary. Rewriting Case.status or migration 001 would break the existing contract and obscure execution vs qualification.

Tests cover expected/transient/uncertain examples, contradictory authentication evidence, distinct recurrence thresholds, duplicate/stale/concurrent commands and ticks, case-bound evidence and digest integrity, schema-1 upgrade preservation, fault rollback, and a real child-process exit before a deadline. Browser acceptance covers evidence entry, persistent decision history, manual reconsideration, and a deadline processed while the browser is offline. This is a qualification foundation, not proof of real-service monitoring or the full M2 response team.
