# Policy, authority, and safety boundaries

## Authority model

Policy is a versioned configuration owned by the operator, outside candidate workspaces and agent-controlled changes.

Capabilities scope repositories, services/environments, data access, tools, filesystem/network access, operational actions, publishing, merging, external communications, and knowledge acceptance.

Evaluate policy deterministically from verified facts and explicit exceptions. Agents recommend; the application grants scoped authority. The commander coordinates within policy and cannot grant new powers.

## Automatic merge

Automatic eligible merging is a confirmed product goal. Admission requires:

1. Repository, base branch, change category, paths, and risk class are allowed.
2. Candidate identity and final relevant base are known.
3. Required reproduction, tests, checks, independent review, and CI satisfy policy.
4. Required repository approvals/protection remain satisfied.
5. No unresolved blocking findings/challenges or conflicting active changes.
6. Limits and any required exception approvals are current.
7. Immediately before the effect, external head/base/eligibility are rechecked.
8. An idempotent or reconcilable merge operation is recorded.

Record both allowed and denied decisions with inputs, versions, reasons, and expiration. Never count the builder's statement that tests passed as verified check evidence.

Approval carryover, test reuse, and merge-strategy handling need explicit rules and tests. Until implemented, candidate/base changes trigger conservative revalidation.

## Separate powers

Publishing and merging do not imply deployment or arbitrary production execution. Treat production mitigation, secret changes, data migrations/deletion, auth/billing changes, and customer communications as separate action classes.

Use declared sensitive-path and action policies. An exception states exact scope, authority, compensating controls, expiry, and required follow-up. No silent bypass for emergencies.

Builders get no repository publication credentials. Production effects run through controlled executors where feasible, not arbitrary shell access with broad cloud credentials.

## Untrusted input

Logs can contain attacker-controlled instructions, URLs, payloads, and secrets. Source signatures establish provenance, not instruction authority.

Separate trusted task/policy instructions from evidence. Bound/redact telemetry and restrict fetches to configured providers/hosts. Defend enrichment against SSRF, credential forwarding, oversized artifacts, malicious archives, and source-map paths.

Candidate code, repository configuration, hooks, tests, plugins, and imported lessons can execute or influence agents. Run builds/tests in qualified isolation. Pin trusted role definitions and knowledge outside the candidate.

Use actual OS/container/worker boundaries. Git worktrees alone are not security isolation; they share repository metadata and often host privileges.

## Independent validation

Freeze artifact versions for review. Avoid the author's private transcript in reviewer context. The same model may staff different roles, but independent evidence gathering and separate capability scopes remain mandatory.

Record disagreement; evaluate claims through experiments and evidence. Model agreement is not a gate. Review attestations bind to candidate, checks, environment, policy, and role identity.

## Secrets and privacy

Store secret references, use platform secret stores or configured secure injection, redact transcripts, and scope access by incident/service. Do not copy harness OAuth stores into the factory or worker artifacts.

Define retention for raw evidence, transcripts, exported reports, and postmortems. Public summaries and internal incident detail are different audiences. Restrict sensitive security/data incidents.

## Threat and recovery scenarios

Test injected telemetry, a candidate modifying its own instructions, forged agent results, stale-worker writes, duplicate publishing, malicious imported knowledge, leaked credentials in logs, unsafe remote web access, and a compromised adapter.

Pause new effects independently from monitoring and notifications. Emergency stop revokes applicable capabilities and cancels workers; reconcile potentially completed external operations before resuming.

## Current authority in the first slice

The only executable policy profile is local-fake-only-v1: manual intake, title updates, attributed operator evidence/qualification/reconsideration, deterministic review deadlines, and side-effect-free synthetic investigation. No selectable real harness, shell action, production mutation, publication, communication, policy update, or knowledge acceptance endpoint exists. A shared local bearer token represents one trusted operator; actor labels are attribution, not RBAC identities. Direct CLI authority is OS-level database access.

Results require the matching assignment/run/case/current fence and an unexpired lease; late results cannot advance authoritative state. Completion means synthetic evidence recorded and operator review pending, never recovery or independent verification. General policy evaluation, pause/stop controls, subprocess containment, scoped external effects, and independence gates above remain required before operational automation.

Expected/transient qualification requires operator-reported case evidence and explicit uncertainty/scope; it is not a source-suppression rule or an independent verification attestation. Recurrence and deadline processing only reopen an owned review obligation. New classifications cannot expand executor authority, create repairs, or declare recovery. All new writes preserve aggregate version/key checks and event history; current ownership is explicit.

## First M2 fake authority

Only local fake-* investigator identities with fixed side-effect-free behavior can claim/receive handoff. Run proof and current lease/progress/absolute deadlines fence worker updates; operator transfers use case version checks and require successor acceptance. No chat body, message type, actor label, or handoff expands real capabilities or acts as independent approval. The bearer operator can simulate local worker attribution; this is not worker authentication.

Late known proofs create wrapped unaccepted evidence and an audit event, never an authoritative result. Proof/briefing contents are immutable. Block recovery retains original deadline/shared attempt budget. The fake boundary has no mutable external resources, so fencing makes it safe; real OS process stop/quarantine, credentials, isolation, and external-effect reconciliation are still absent and must precede any real adapter. Local escalations do not send notifications. See ADR 0006.
