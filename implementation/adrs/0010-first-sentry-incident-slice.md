# ADR 0010: Authenticated source observations and explicit incident declaration

Accepted 2026-11-15. First M3 vertical slice; neither full M3 nor live activation is claimed.

The first source adapter is read-only Sentry Cloud, explicitly mapped to one organization/project/service/environment/repository and releases. CLI-installed immutable configuration references injected secrets and trusted severity update profiles. It cannot grant production or publication authority.

Verify HMAC-SHA256 over exact raw webhook bytes before durable acceptance. Constant-time verification and body limits are mandatory. Signed-body receipts and normalized event deduplication are separate. No header is assumed cryptographically bound; durable deduplication handles replay without inventing a timestamp freshness guarantee. Discard raw private bodies after bounded normalization. Imported URLs are never fetched.

Enrichment reads fixed allowlisted endpoints and excludes user/request/message/variable data. API snapshots differ from webhook hints and service recovery. UTC-normalized older occurrence snapshots cannot regress current projections. Stable case identity and transactional observations survive the case-creation boundary. Missing enrichment remains a visible retry duty. Source and notification pumps cannot own deterministic team supervision.

Reconciliation persists windows, overlap and pagination cursors; page intake and cursor progress commit together. Incomplete pages, cyclic cursors and outages do not claim complete coverage. Coverage is bounded by issue snapshots available in the requested API window; historical per-error events and old status-only changes require a wider future integration drill.

Incident declaration is a trusted operator classification against exact case/source evidence. It freezes source/configuration/policy provenance, severity/rationale, impact/unknowns, roles, objectives, investigation references and an unsent draft. JSON and escaped Markdown follow the incident template's major sections. Unknown mitigation/recovery stays unknown; source resolution cannot close an incident. Severity update deadlines escalate deterministically. Broader incident revision, mitigation/recovery, postmortems and actions remain unfinished.

Synthetic sanitized fixtures, a cryptographic vector, injected read-only transports and a signed HTTP/browser scenario exercise the boundary without live services. Activation is blocked on actual mappings, secrets/read permissions and trusted ingress. No live verification is claimed.
