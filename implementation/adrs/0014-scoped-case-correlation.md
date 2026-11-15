# ADR 0014: Scoped case correlation without suppression

Accepted 2026-11-15, continuing M3 after PRs #6–#8 merged into main `33da6d9`.

Add diagnostic evidence from a bounded allowlist of representative-event symbols and categories. Preserve old observation shapes when the optional diagnostic field is absent. Never ingest stack paths/code/variables, private messages or request/user details for this feature. Symbols remain untrusted; provider event grouping is not a causal model.

Offer read-only, deliberately narrow suggestions: same configured service/environment/repository, exact mapped release/revision, exception/frame signature, last occurrences within one hour and successful current read state on both sides. An indexed scan of the newest 200 same-scope cases emits at most 20 candidates. These are review hints, not authority, exhaustive coverage or automatic deduplication. The timing filter does not govern agent execution.

A trusted operator owning both cases can record a related/duplicate link against exact versions and current source evidence in both cases. Duplicate judgments additionally require operator-reported evidence on both sides. Unresolved blocking challenges prevent acceptance. Unordered pair revisions, both case events and command receipts share one transaction. Links can be revoked during a challenge or outage; old versions cannot be edited/deleted. New fact digests make review currency stale, while unchanged rereads preserve it. Original receipts stay historical. No link transfers or closes any case, incident, escalation or duty; catalogue judgments remain distinct from assessment dispositions and canonical-case merging.

The CLI/API/web expose the same operation and comparisons; escaped qualification Markdown preserves diagnostics, freshness, decisions and links. The web refreshes comparisons explicitly because target-only events do not rewrite source case history. Local actor attribution remains the existing single-operator trust boundary.

Synthetic checkout contract acceptance covers intentional unauthenticated rejection versus an actual fixture failure, misleading diagnostic matches, scoped evidence, review deadlines and duplicate judgments. This is not real sanitized customer history. Live activation remains blocked on operator-supplied mappings/history, secrets, destinations, ingress and permissions. M3 is not declared fully accepted, and no real execution or production authority is introduced.
