# ADR 0008: Immutable operator policy revisions

Accepted 2026-11-14. Implements trusted fake-runtime timer/budget configuration, without granting real execution or notification authority.

The compiled JSON bootstrap replaces scattered development constants. Strict bounded policy documents are installed deliberately through the OS-level operator CLI, never an agent message, claim, candidate file, or HTTP endpoint. The current deployment trusts operator database/CLI access; a hostile process with that same OS authority is outside this fake-only boundary. Real agents must use isolated credentials/processes before M4.

Each case pins a revision before its first event. Name, revision, canonical digest, installer, source and install time persist; policy rows cannot be changed or deleted. New revisions must advance monotonically. Existing cases, original deadlines and shared budgets do not change. Migrated cases pin the legacy compiled profile and keep their actual persisted attempt/deadline state. New events record exact policy provenance. Snapshots expose governing configuration and web review panels display it. Corrupt policy bytes fail closed.

Validation covers numeric bounds/order, unknown fields/versions, attempted agent installation, immutable/reused/skipped revisions, old/new case pinning, restart and execution supervision under different policies. Explicit new-case rollout avoids silently expanding an agent's existing authority. Emergency changes for active cases require a future separate, audited exception workflow.
