# Postmortems and scoped knowledge v1

`watchhouse case learning CASE_ID --file command.json`, authenticated POST `/api/v1/cases/{id}/learning`, and the web's versioned command form share one service. `case learning-markdown` and GET the `/learning/markdown` child render professional postmortem sections, action tables, scoped lessons and retained structured records.

All commands use the existing `command`/`meta`/`expected_version` envelope plus `action`. New keys identify new commands; retries return historical receipts. Actor labels represent local operator attribution, not independent remote authentication. The author cannot accept their own revision, but a shared local operator remains responsible for truthful identity and evidence.

## Postmortems

`draft_postmortem`: optional `id`/`expected_revision` together, plus `draft`:

- title, summary, chronology and response_assessment;
- impact: nullable requests/affected/started_at/ended_at, method and uncertainty. Quantified impact requires denominator and bounded non-future window; unknowns remain null;
- cause: `{"status":"unknown","question":"..."}` or `{"status":"supported","hypothesis_id":"...","revision":1}`. A supported cause must reference the current supported hypothesis revision; a proposal or repeated assertion cannot satisfy it;
- 1..20 distinct same-case `action_ids`, case evidence_refs and bounded review_seconds.

Commander/scribe drafts it. `accept` requires id/revision, clear challenges, current frozen incident/source/recovery context, resolved response, current supported cause/action references and the independent reviewer's own distinct evidence collected after drafting. Evidence order uses case versions as well as timestamps, so a report predating submission cannot pass when clocks are equal. Legacy unversioned unreviewed recovery submissions require fresh resubmission before new review.

A new draft does not replace a previously accepted version. Accepting the new revision changes the current accepted version and invalidates lessons tied to the old accepted postmortem. Source, operating/recovery context, hypothesis or evidence drift invalidates affected acceptance; immutable history remains. Follow-up actions can stay open after postmortem acceptance.

## Knowledge

`propose_knowledge`: optional id/expected_revision plus draft fields: title, kind (`lesson`, `known_error`, `runbook`, `service_map`, `decision`, `process`), body, applicability, invalidation_conditions, same-case postmortem_id/postmortem_revision, evidence_refs, expires_seconds, nullable imported_from.

Scope is copied from the explicitly mapped incident: exact service/environment/repository/release, including an unknown release as null rather than a wildcard. Imported provenance is retained as data; no accepted-state field or external fetch is allowed. A proposal can exist before parent acceptance, but cannot be accepted until the exact parent is accepted in current context. Independent `accept` requires new reviewer-authored evidence and a different author. `retire` requires current commander, exact id/revision, case evidence and a reason; retirement is immutable. To revive knowledge, propose and review a new revision.

Retrieval:

```sh
watchhouse knowledge --service checkout --environment test \
  --repository example/checkout --release fixture-release-1 --query authentication
```

Exact scope is mandatory. Query terms are literal lexical filters, not SQL patterns or URLs. At most the newest 200 accepted candidates are considered and 20 current results returned; incompleteness never implies no relevant lesson exists. Proposed, expired, retired, challenged, corrupt-evidence or stale-parent/context versions are excluded. Unaccepted revisions cannot modify or hide the preceding accepted version. Expiry/review duties escalate without models, including expiry of an accepted version while a newer proposal awaits review.

Each new fake worker briefing freezes the accepted version IDs/digests, title/body, applicability, invalidation conditions and expiry. Existing briefings are not rewritten when knowledge changes. Knowledge is advisory data: it cannot install policy, authorize execution, bypass challenges or suppress a case automatically. A runbook entry is not an executable production runbook.

Migration 016 retains immutable documents/digests, independent acceptances and retirements. Existing briefings default their new accepted_knowledge field to empty. CLI search output is a native Watchhouse reference format; Hallmark import/export compatibility has not been claimed or verified.

See [ADR 0019](../../../implementation/adrs/0019-reviewed-operational-learning.md), [example generator](../../../examples/README.md) and [STATUS](../../../implementation/STATUS.md).
