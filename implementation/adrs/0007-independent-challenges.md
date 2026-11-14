# ADR 0007: Independent challenges and assessment gates

Accepted 2026-11-14. Implements the next bounded P03/P04 slice.

Challenges freeze an immutable event ID and aggregate version rather than mutable prose. Separate fake challenger, respondent and verifier responsibilities receive only public claim/evidence references; private executor transcripts are excluded. Local actor names remain simulations under the existing trusted operator boundary. No real-worker authority is introduced.

A response is evidence-backed but cannot resolve its own challenge. The verifier must record a separate observation absent from the answer. Dissent/uncertainty is retained in append-only transition events. Missed deadlines create an owned local escalation without models. Blocking challenges fence result acceptance, disposition, validation and closure. Validation/closure binds conservatively to the exact case version: any later event requires revalidation. Assessment closure is separate from recovery and does not stop timers.

Migration 004 adds challenge projections and gates without rewriting historical events or migrations. Commands, projections, events and receipts commit atomically. CLI/API/web share the service. Tests cover restart, deadline deduplication, stale versions, evidence/role independence and changed-version closure.
