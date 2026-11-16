# Knowledge, runbooks, and learning

## Default behavior

Provide a built-in operational knowledge store. Use it to retrieve scoped, accepted knowledge into role briefings. Watchhouse must work without a Hallmark installation.

Learning here means preserving validated operational knowledge and improving future decisions through retrieval, evaluation, and controlled runbook changes. Model fine-tuning is not an initial requirement.

## Knowledge types

- Service maps: ownership, dependencies, critical workflows, repository/release relationships.
- Known errors: symptoms, applicability, cause status, workarounds, detection and reopening conditions.
- Runbooks: versioned prerequisites, authorized steps, stop/rollback criteria, verification.
- Lessons: prevention, detection, mitigation, investigation patterns, and validated false positives.
- Decisions: rationale, alternatives, authority, residual risk, review date.
- Process improvements: escalation, communication, tooling, and validation changes.

## Entry contract

Stable ID and namespace; kind; title/body; service/repository/environment/release/path scope; source incidents/evidence; author/reviewer; proposed/accepted/retired status; confidence and counterevidence; created/reviewed/expiry timestamps; supersession history; use and outcome references.

Separate team/organization knowledge from repository-specific knowledge. Cross-service generalization requires review. Preserve provenance when importing or renumbering.

A previous expected 401 case is not a blanket rule to ignore authentication failures. Lessons must retain applicability conditions and evidence that would invalidate them.

## Promotion and retirement

Agents propose new entries or revisions. Independent review verifies source evidence, applicability, overgeneralization, contradictions, and whether the entry would weaken detection or gates.

Default: proposals do not alter operational decisions until accepted. Organizations may configure bounded auto-acceptance for low-risk factual maps; policy changes and broad suppressions require stronger authority.

Use a frozen accepted version in each run briefing. A proposal or branch cannot modify the knowledge governing its own review. Record the entry versions actually used.

Retire or flag entries after contradictory evidence, invalidated dependencies, expired scope, changed service behavior, or review deadlines. Do not automatically retire a critical runbook solely because it was rarely used.

## Hallmark compatibility

Hallmark currently stores per-clone Markdown lessons with frontmatter, history, acceptance state, search, and briefings. Its JSON import/export format is a practical compatibility boundary; current export does not include full history.

Start with an adapter to that format. Preserve source namespace and IDs, detect collisions, and never silently overwrite accepted entries. Imported operational authority is not trusted by default.

Evaluate extraction of shared components later. Do not couple Watchhouse's domain to Hallmark's repository object or internal directory layout.

## Evaluating learning

Record when a lesson influenced a disposition, hypothesis, check, or review. Evaluate on held-out historical cases: useful retrieval, missed actionable defects, unnecessary repairs, repeated mistakes, and invalid suppressions.

Do not promote an incident's speculative explanation into accepted knowledge just because agents repeated it. Postmortem review and future outcome evidence can revise the lesson.

Search/retrieval can begin with metadata and lexical search. Embeddings or a specialized search engine are optional enhancements, not the initial correctness foundation.

## Implemented local operator slice

[Learning v1](../contracts/v1/learning/README.md) implements postmortem and knowledge revisions, independent attributed promotion, expiry/retirement, exact-scope lexical retrieval and frozen accepted references in fake worker briefings. Imported provenance is data, never authority. [Hallmark exchange v1](../contracts/v1/hallmark/README.md) now implements bounded actual-format import/export, immutable untrusted source records and explicit independent native promotion. Native exports remain proposed in the destination. Actual Hallmark execution, live-agent integration, held-out history evaluation and broader scope/generalization remain pending. See ADR 0019 and STATUS.
