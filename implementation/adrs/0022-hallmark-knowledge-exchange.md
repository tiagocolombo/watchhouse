# ADR 0022: Hallmark JSON exchange without approval transfer

Accepted as an implementation proposal 2026-11-16. Complete the M5 format-compatibility boundary independently of M4 runtime admission. Hallmark remains optional; this adds no backend publication/merge delegation.

Inspect the actual upstream JSON/parser/learning/reply definitions at commit `15ee84d9de5385658b8d9dff1408728fb9e28fbd`, rather than inventing a nominally compatible schema. Accept its canonical export, flattened CLI reply and bare array. Retain bounded opaque Markdown, namespace, full source path and content/record digests in append-only case-scoped records; never extract files or load imported instructions. Changed source content becomes a new record, not an overwrite.

Reuse native proposal/review/context/evidence gates. A `hallmark-import:ID` provenance reference must resolve to intact same-case imported bytes before proposal and continued acceptance. No upstream `accepted` flag changes local authority or accepted briefings. The operator explicitly maps content, applicability and invalidation into a native draft tied to a postmortem.

Offer exact raw re-export for selected upstream records and conservative native export for selected current accepted lessons. Reject path/namespace/revision collisions. Native exports are proposed in Hallmark, with stable explicit Watchhouse source identity and scoped metadata/body. This prevents local acceptance from becoming an unreviewed cross-repository suppression. Export IDs are bundle-local; no full-history claim is made.

Migration 018, shared CLI/API, timeline and inert web inspection complete this bounded format adapter. Actual Hallmark execution, historical evaluation, runbook/deployment/telemetry integrations, and M5's broader operational acceptance remain distinct. See the [implemented contract](../../contracts/v1/hallmark/README.md) and STATUS.
