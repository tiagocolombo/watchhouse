# Verification and acceptance

## Approach

Test domain invariants and failure boundaries with fake clocks, sources, adapters, workers, and external-effect simulators. Standard tests must not require subscriptions, paid models, live production, or network access.

Contract fixtures pin schema/runtime versions and make unsupported behavior explicit. Live compatibility runs are opt-in and report actual runtime versions and permissions.

## Acceptance scenarios

| ID | Scenario | Required evidence |
| --- | --- | --- |
| V01 | Correctly rejected authentication request | Catalogue rationale; no repair; scoped reconsideration |
| V02 | Authentication refresh regression also returns 401 | Reproduction admits defect; not suppressed by V01 lesson |
| V03 | Short dependency timeout | Monitored disposition; threshold/recurrence reopens |
| V04 | Sentry and Datadog report one fault | Linked signals and reversible correlation, no duplicate repair |
| V05 | One provider issue contains two causes | Separate hypotheses/cases with provenance |
| V06 | Duplicate/out-of-order webhook and missed delivery | Idempotent intake, historical timeline, reconciliation coverage |
| V07 | Worker crashes or stops heartbeating | Checkpoint recovery and fenced reassignment |
| V08 | Worker alive but repeating failed experiments | Progress deadline triggers block and revised approach |
| V09 | All model providers unavailable | Supervisor, human notifications, escalation timers remain active |
| V10 | Old worker returns after reassignment | Result attributed but cannot mutate authoritative workflow |
| V11 | Quota/auth failure mid-fix | Typed failure, bounded fallback, preserved budget and workspace |
| V12 | Unsupported isolation or usage guarantee | Admission denied or routed to qualified worker |
| V13 | Investigator and challenger disagree | Targeted experiment or explicit arbitration; no vote-based approval |
| V14 | Missing authority for production rollback | Proposal retained; escalation delivered; no execution |
| V15 | Patch without adequate regression evidence | Validation/review blocks advancement |
| V16 | Candidate or relevant base changes after review | Evidence invalidated/revalidated before merge |
| V17 | Crash after PR creation or merge | External state reconciled; no duplicate effect |
| V18 | Incident mitigated before permanent repair | Response resolves separately; problem/actions remain owned |
| V19 | Merge without observed deployment | No false deployed/recovered status |
| V20 | No events because traffic stopped | Recovery inconclusive, not successful |
| V21 | Postmortem invents cause or impact number | Challenge blocks acceptance until corrected/marked unknown |
| V22 | Corrective action marked done without proof | Acceptance criteria and independent verification enforced |
| V23 | Unaccepted/imported lesson weakens suppression | Proposal excluded; trusted version remains |
| V24 | Malicious telemetry or candidate instructions | No authority expansion, secret access, or uncontrolled fetch |
| V25 | Escalation unacknowledged | Fallback chain, delivery evidence, accountable ownership |
| V26 | Operator closes/reopens browser | No processing interruption or missing/duplicated timeline state |
| V27 | Backup restored on a new coordinator | Obligations preserved; stale leases revoked; effects reconciled |
| V28 | Event flood during an active incident | Escalation/recovery jobs remain schedulable |
| V29 | Sensitive evidence is redacted | Tombstone/provenance preserved; affected decisions flagged |
| V30 | Apple build assigned to Linux worker | Rejected/rerouted by capability requirements |

## Test layers

- Pure domain/state-machine tests: transitions, policy, clocks, correlation, closure gates.
- Persistence tests: transactions, concurrency, migrations, outbox, recovery, artifact completeness.
- Worker tests: leases, process-tree termination, stale results, workspace isolation, fence revocation.
- Adapter contracts: protocol framing, errors, capability claims, output validation, cancellation.
- Workflow simulations: full response, challenges, mitigation, repair, review, recovery, actions.
- Browser acceptance: operating views, evidence navigation, stale commands, escalation and controls.
- Opt-in integration: sanitized source fixtures, dedicated GitHub test repository, installed harness/version.
- Operational drills: degraded models, broken source, unreachable notification, storage pressure, restore.

## Evaluation

Replay historical cases with frozen evidence/policy/knowledge snapshots. Separate training/reference cases from evaluation cases. Score dispositions, unnecessary repairs, missed actionable defects, causal accuracy, fix effectiveness, recurrence, escalation quality, and resource use.

Human review is required for evaluation labels where operational truth is uncertain. Retain ambiguous labels instead of optimizing against invented certainty.

## Definition of done per feature

Implementation plus meaningful tests, visible state/evidence, typed failure/recovery behavior, documented authority, updated contracts, and STATUS.md entries with actual commands/results. Avoid tests that merely mirror field assignments.

## Verified first slice (2026-11-14)

- Rust workflow tests: atomic intake/claim/completion rollback under injected SQL failures; append-only history; versioned fixture validation and rejection; migration checksums; duplicate key replay/mismatch; concurrent updates/claims; invalid/misattributed/stale result rejection; deterministic adapter and fake clock/source.
- Process recovery: a child exits without destructors after intake, claim, execution, and completion. Reopen the real file, advance the controlled clock, retry pending work, and retain one accepted case/result with all committed history.
- HTTP tests: token/origin/host enforcement, strict schema decoding, structured conflicts, snapshot plus Last-Event-ID stream replay, cursor validation, and evidence path coverage through browser acceptance.
- CLI integration: separate processes migrate, create/retry, execute, show results, and reject stale updates against the same store/services.
- Chromium: create through UI, inspect persisted evidence, confirm script-looking input is inert, disconnect while another client changes the case, reconnect/reload without duplicate entries, and reject a competing stale browser command. Desktop/mobile screenshots are reviewed locally.

V26 is covered for this bounded workflow. The fake lease tests establish a prerequisite for V07/V10; they do not prove full M2 takeover, checkpoint/handoff, external-effect cancellation, notification, or escalation behavior. At the initial slice, V01–V03 dispositions/reopening were unfinished; the extension evidence below now covers bounded manual qualification. Other wider acceptance scenarios above remain planned. Exact commands/results and platform limits are in [STATUS](STATUS.md).

## Verified M1 qualification extension

- Expected behavior requires reported supporting evidence and has expiry review; transient hypotheses preserve recovery uncertainty and reopen at a distinct recurrence threshold; insufficient evidence carries a concrete collection obligation. Contradictory authenticated-401 evidence reopens an expected unauthenticated-401 decision; no status-code suppression is performed.
- Duplicate command receipts, changed-key conflicts, stale/concurrent decisions, concurrent deadline ticks, reused/pre-decision recurrence evidence, foreign-case evidence, and corrupt digests have explicit outcomes. SQL faults roll back evidence, decisions/ownership/obligations, and timer events together.
- Migration-1 databases upgrade to schema 2 while retaining historical event bytes/command receipts and backfilling fake-result evidence links. Versioned examples reject incompatible plan/version fields; old snapshots remain readable.
- A child process exits after accepting a decision; a new process with the fake clock at its deadline reopens its owned obligation once. CLI and HTTP exercise the same commands; browser acceptance records evidence/decisions, retains history, and replays deadline events after going offline.

This satisfies the bounded M1 expected/transient/uncertain qualification exit. V01/V03 mechanisms and V02 reconsideration are covered by manual sanitized examples; live source semantics, real reproduction, independent verification, M2 notifications/handoff, and actual service recovery remain unverified. See STATUS for commands and results.

## Verified first M2 core slice

- Rust/CLI prove a fake investigator can block, persist a checkpoint/directed message, wait under its prior accountable owner, then be replaced by an addressed successor that accepts and finishes without HTTP. Role remains investigator and absolute deadline/attempt budget carry over.
- Healthy heartbeats without progress trigger a block; repeated checkpoint content does not reset progress; even continuously new reports cannot extend absolute time. Exhausted attempts prevent another run. Missing handoff/message acknowledgments create one local escalation; acknowledgment never resolves it or transfers work.
- Concurrent offers/acceptance/ticks have explicit outcomes. Injected faults roll back checkpoint/evidence/messages/block authority, handoff/owner transfer, and escalation together. Children exit after block/offer/accept; new processes recover durable obligations and proofs.
- Schema-2 active runs upgrade with original fence, event bytes, and command receipts. Strict fixtures reject unknown schema/role fields; old snapshots default the team projection. Known late output is attributed once and excluded from accepted outcomes; forged/misattributed proof is rejected.
- Browser acceptance observes block/checkpoint and messages, offers a handoff, disconnects while the core accepts/completes and rejects old output, then replays without duplicate timeline facts. It also observes/acknowledges a local overdue-message escalation and checks inert message text/mobile width.

V07/V08/V10/V26 foundations are covered in this bounded fake boundary. V09 timers/inbox work with no model; V25 external delivery/fallback and full notification semantics remain unimplemented. No independent challenge or real-worker isolation/production-recovery claim is made. Current commands/results are in STATUS.
