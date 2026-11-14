# ADR 0006 — Durable fake-team communication, supervision, and acknowledged handoff

Date: 2026-11-14. Status: accepted for the first bounded M2 slice. Partially implements P03/P04; M2 remains unfinished.

## Decision

Keep the coordinator's brain in Rust Application services, with provider-independent records in domain/team.rs and SQLite mapping in team/store.rs. The CLI and HTTP are adapters over those services. The web app shows state and submits validated operator intentions; it never owns timers, assignment authority, or handoff execution.

Add migration 003 without rewriting migrations 001/002, accepted events, or receipts. Add case threads, typed directed messages/recipient acknowledgments, assignment coordination state, immutable run authority/briefings, owned blockers/checkpoints, offered/accepted handoffs, local escalation records, progress digests, and attributed late results. Existing execution-status fields remain unchanged; a running investigation can be blocked or awaiting handoff in the new coordination projection. Snapshots add a defaulted team projection for reading older payloads.

Messages are bounded data, never capability grants or independent review attestations. Operator messages are observation/question/status; block and handoff notices commit through their corresponding commands. Replies stay in the case thread. Typed operator commands use case versions and global idempotency receipts. Fenced worker activity uses the run proof, not a case version, to avoid conflicts with unrelated discussion. Real worker identity/authentication is not implied by local attribution strings.

## Supervision and recovery

Every accepted fake attempt has a persisted executor/fence, heartbeat, useful-progress timestamp/deadline, and frozen briefing. A new checkpoint digest is required to report progress; a heartbeat never advances progress. This establishes deterministic stalled-but-alive detection, not a semantic proof that a reported new checkpoint is useful. Hard shared limits bound reworded reports as well.

For this development-only fake profile: lease 30 seconds, useful-progress deadline 60 seconds, block response 120 seconds, absolute assignment deadline 300 seconds, and three attempts total. Existing active histories receive at least one remaining attempt on upgrade. Per-message and handoff acknowledgment windows are explicit bounded operator inputs. General trusted policy configuration is still M2 work; these numbers are not an organization's severity policy.

The deterministic supervisor detects lost heartbeats, missed progress, or absolute deadline; atomically captures an owned block/checkpoint/message, revokes the current fence, and stops admission from that attempt. Exhausted attempts make response immediately due. Block response, handoff acceptance, and required message acknowledgment deadlines create deduplicated owned records in the local operator inbox. Models and a browser are unnecessary for this work. A stopped coordinator catches up on restart; it cannot notify while stopped.

**This supersedes ADR 0002's M0 automatic expired-fake retry path.** Lost attempts now retain owned blockers and require explicit accepted transfer rather than silent retry. Historical run.lease_expired events remain readable. Recovery acceptance tests were strengthened to exercise the block/handoff path while preserving accepted work and rejecting stale results.

## Handoff and late results

A handoff preserves its objective, investigator role, checkpoint/evidence, latest relevant message IDs, original absolute deadline, and remaining shared attempt budget. Offering it never transfers accountability. The successor must accept; new run/fence, owner transfer, acceptance event, and receipt commit atomically. Expiry keeps the old owner accountable and records an escalation. New proposals cannot reset the absolute deadline or attempt count.

Late known proofs can submit untrusted output only as attributed, wrapped, unaccepted evidence; repeated identical output is stored once per run/digest. Audit events may increment the case version, but no case outcome/assignment state advances. Unknown proofs do not invent attribution. Accepted and late evidence remain distinct even when their result bytes are otherwise identical.

The fake adapter and simulation workers have no external mutation or mutable workspace. Therefore revoking their database authority is sufficient for this slice; no OS cancellation or credential revocation claim is made. A real subprocess adapter requires separate stop/quarantine/resource isolation and uncertain-effect reconciliation before any reassignment. The only admitted identities are local fake-* investigators with fixed fake capabilities; there is no runtime registration path.

## Running the core

`worker --block --once` deterministically creates an owned fake blocker. An operator offers a handoff via a versioned file command. `worker --identity fake-specialist --once` discovers its offer, accepts, and executes the frozen task, without a web server. `worker --stall` stays alive and heartbeats while reporting no useful progress, so the supervisor eventually blocks it. `serve --manual-workers` keeps timers and HTTP alive while separate CLI workers dispatch. `tick` evaluates qualification plus team deadlines without an executor. Ordinary version contention is retried by the worker loop; infrastructure failures remain surfaced.

## Evidence and remaining boundaries

Tests cover schema-2 active-run upgrades, immutable proof/briefing history, receipt replay, direct headless worker coordination, stalled-but-alive detection, finite time/attempt budgets, concurrent offers/acceptance/ticks, fault rollback, block/offer/accept process-loss boundaries, acknowledgment vs resolution, stale output retention, forged proof/schema/role rejection, and browser replay/visibility. Standard tests use only local SQLite/fake executors.

Independent challenge/verification gates, broader roles, multiple concurrent responsibilities, external notification/delivery/fallback chains, escalation resolution, configurable production policies, OS worker isolation, and packaged service installation remain unimplemented. No external message is sent by this slice. Full M2 exit is not claimed.
