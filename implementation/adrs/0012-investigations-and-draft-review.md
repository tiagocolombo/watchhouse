# ADR 0012: Immutable investigation versions and independent draft review

Accepted 2026-11-15. Second slice of the next M3 batch; depends on ADR 0011.

Store hypotheses, experiment plans/results and communication drafts as immutable artifact versions. Freeze exact hypothesis revision/scope for discriminating experiments. A supported/refuted claim must cite a completed matching experiment and evidence; changing the claim cannot reuse that experiment. Failed/refuted/inconclusive findings and uncertainty remain in history. Recording a plan never executes its text or grants capabilities.

Current operations/commander responsibilities own investigations; communications responsibility owns drafts. A distinct current commander/operations reviewer can approve an exact draft revision under unchanged incident/source context and clear blocking challenges. Approval becomes stale after later case events and never authorizes sending. Local actor labels remain trusted attribution, not authentication of remote people or models.

Owned experiment/review obligations have policy-bounded deadlines. Missing evidence/review escalates without models. Late completion/approval materializes missed duties first; past escalations require separate resolution. Pending/due obligations block assessment closure. Deadline processing does not depend on artifact snapshots or model availability.

Migration 009, version/key checks and one SQLite transaction preserve artifacts, obligations, events, approvals and receipts. CLI/API/web and structured Markdown share records. Fixture tests cover claim drift, self/stale approval, delayed review, deadline/closure gates, immutable history, concurrency, rollback and inert browser replay. No external communication or production effect was added.
