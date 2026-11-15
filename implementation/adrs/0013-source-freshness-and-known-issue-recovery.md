# ADR 0013: Fenced source reads and bounded known-issue recovery

Accepted 2026-11-15. Third slice of the next M3 batch, depending on ADRs 0011–0012.

Requested discovery windows cannot prove coverage of status-only changes to known issues whose last occurrence is old. Add a bounded known-issue sweep using the existing fixed-host read API. Track read freshness separately from discovery coverage and from the immutable source fact version. Healthy unchanged rereads update freshness without spamming case events or invalidating draft review; real changes still append evidence.

Claim a monotonic source read generation and persisted retry deadline before I/O. Only the current generation can advance projection/freshness. Late results are retained as explicitly unaccepted evidence; failures and interrupted reads cannot claim fresh observations. Older occurrence evidence or API data predating an accepted webhook hint cannot suppress the newer pending duty.

A source-configured refresh interval defaults to the existing v1 60 seconds. Skip serialization of that default to retain canonical legacy configuration bytes; changing it requires a validated immutable operator revision. Due-time ordering bounds intake/sweep work without starving later sources. Page, intake and known-read duties remain independent of model availability and of the supervisor/notification pumps.

Migration 010 stores read state without rewriting source history. Health exposes generation/configuration/freshness/error independently of case events; the web polls read-only health. Existing credential, ingress, retention, self-hosted/regional API and full historical event coverage limits remain. Fixture tests prove missed old status changes, unchanged-read deduplication and review retention, outage/restart, stale overlapping reads, interrupted result storage and atomic retry. No live coverage guarantee is claimed.
