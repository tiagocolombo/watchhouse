# ADR 0011: Versioned incident operations and accepted role handoffs

Accepted 2026-11-15. First slice of the next M3 batch, based on main after merging PRs #2–#5.

Keep incident declaration immutable. A separate current operations projection records evidence-backed severity/impact/uncertainty/objective updates and role accountability. Commands serialize through case versions and receipts. The original source configuration governs severity update intervals; a handoff cannot change them.

Commander-authorized role offers require addressed acceptance. Prior accountability remains until acceptance; expiry escalates deterministically. These remain trusted local human commands, not remote identity authentication or real-agent admission. Updating coordination cannot grant execution authority.

Update cycles have stable unique escalation identities. An overdue duty is materialized even if an operator update beats the timer. Scheduling another cycle never resolves past escalations. Due-row selection excludes completed duties so earlier incidents cannot indefinitely starve later ones. Existing declarations upgrade lazily with their original deadline and escalation flag.

Migration 008 stores projections and role offers without modifying old records or migrations. Shared CLI/API/web services and current-state Markdown preserve history. Tests cover evidence/version checks, restart, wrong/late acceptance, repeated deadlines, concurrency and transactional rollback.
