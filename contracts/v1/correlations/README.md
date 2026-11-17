# Case correlation v1

The [structured assessment extension](../assessments/README.md) adds separately configured cross-service related-link review and duplicate assessment dispositions. Its explicit topology/current-source gates supersede the historical same-scope-only limitations below; no graph-level case merge/suppression is introduced.

`case correlate CASE_ID --file FILE` and `POST /api/v1/cases/CASE_ID/correlations` share a strict versioned command. Case snapshots expose `correlations`, and the web has a read-only comparison/link panel plus the versioned command form. Refresh the panel to assess another case's latest facts; cross-case changes do not fan out synthetic events.

```json
{
  "command": {
    "meta": {"schema_version":1,"idempotency_key":"review-pair-1","actor":"operator","reason":"Fixture replay supports comparing these two observations"},
    "expected_version":5
  },
  "target_case":"REPLACE_WITH_OTHER_CASE_ID",
  "expected_target_version":5,
  "expected_revision":0,
  "action": {
    "type":"link",
    "kind":"related",
    "source_evidence":["REPLACE_WITH_CURRENT_SOURCE_DIGEST"],
    "target_evidence":["REPLACE_WITH_CURRENT_TARGET_DIGEST"],
    "uncertainty":"Common cause and actual impact remain unknown"
  }
}
```

Both cases must have the same accountable trusted operator, current expected case versions, no blocking challenges, and authoritative API evidence in the same configured service/environment/repository scope. Cite the current source digest on each side. `kind` is `related` or `duplicate`; duplicate judgments additionally require operator-reported evidence on **each** case. Shared symbols, titles, status codes and fake results alone never authorize duplicate acceptance. Actor labels remain local attribution under the operator-token/OS boundary, not remote identity authentication.

An unordered pair has one immutable revision history, irrespective of command direction. `expected_revision` is zero initially and the latest revision thereafter. Changed command bytes under the same idempotency key conflict; exact retries return the original receipt, even after later revisions. Receipt replay is historical, not a fresh assessment.

To revoke, use fresh versions for both cases and the latest pair revision, replacing `action` with `{"type":"revoke"}`. Revocation remains available during blocking challenges or source outages. It retains prior evidence, rationale and uncertainty; both timelines record the new revision. Both case events, the revision and command receipt commit atomically. A second insert failure rolls everything back.

`context_current` means **only** that the active link's two frozen source fact digests still match the projections. It does not establish fresh reads or current correctness. New source facts make it false; renewed review can record a new revision. Unchanged rereads do not stale it. Freshness remains a separate source ledger. Even a current duplicate link never merges/suppresses source intake, transfers ownership, waives review, or closes a case, incident, escalation or obligation. It is a catalogue judgment, not an assessment disposition. Cross-service links and graph-level canonical-case merging remain future work.

## Suggestions

Read-only suggestions compare at most the newest 200 same-scope source cases, returning at most 20. Indexed service/environment/repository scope, exact known release and mapped revision, exception types/ordered bounded frame signature, occurrences within one hour, and a current successful API read on both sides are required. The one-hour filter is a suggestion heuristic, not an execution budget, monitoring timer or authority rule. Title, issue grouping, HTTP status and counts do not determine matches. Existing active links are omitted, including stale links that need renewed review. No match does not establish uniqueness; matching a bounded prefix does not establish common cause. No model or network call is involved in reading suggestions.

At most 100 latest pair heads appear in a snapshot; full immutable transitions remain in events. `case qualification-markdown CASE_ID` / `GET .../qualification/markdown` render source context, freshness, existing dispositions/obligations and links as escaped JSON blocks. Imported content remains inert.

## Diagnostic evidence

Sentry normalization now retains a representative event ID (32 hex characters), at most 20 module/function frame pairs, and allowlisted breadcrumb category names. Frame symbols must be 1–128 bytes of ASCII identifier-like characters; paths/URLs/whitespace are rejected. Traversal is bounded to 20 entries, 10 exception records per entry, 20 frames per exception and 50 breadcrumbs per entry. Messages, paths, source lines, variables, user/request data and breadcrumb data remain excluded. Symbols are still untrusted; this is an allowlist, not a guarantee arbitrary identifiers contain no sensitive text. No automatic cause, impact denominator or recovery is inferred. Missing/wrong-environment events retain no diagnostic context. Old observations without this optional field deserialize and serialize without altering their historical shape; the next enriched read may legitimately introduce a new diagnostic fact snapshot.

Synthetic service-specific acceptance is in [checkout-qualification.json](../../../tests/fixtures/sentry/checkout-qualification.json). It is invented contract history, not customer history. Actual sanitized operational history, live credentials/mappings and approved activation remain outstanding.
