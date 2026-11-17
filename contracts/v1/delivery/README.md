# Reviewed delivery preparation v1

`watchhouse case publication-plan CASE --candidate CANDIDATE --review REVIEW` and authenticated `GET /api/v1/cases/{id}/publication-plan?candidate=...&review=...` prepare the same read-only artifact. Current review cards expose an inert inspection button.

The application rechecks the current exact candidate, native regression evidence, independent review, source/profile/knowledge/challenge context and manifest digest. It verifies all candidate bytes and canonical paths, including parent directories; bounds file count, individual and total sizes; rejects symlinks, collisions, payload/manifest disagreement and deleted files remaining in the manifest. Git tree/blob identities use Git's object format and directory sort order, independently checked against actual Git in fixtures. Binary unchanged files remain part of the expected tree. No candidate code, hooks or repository configuration executes during preparation.

The artifact records the candidate/review/base identity and exact expected Git tree SHA, together with a GitHub Git Data tree request over the frozen base. A publisher must reject a returned tree with another SHA. SHA-256 Git repositories are currently unsupported and rejected; this adapter targets GitHub's SHA-1 Git Data contract.

`external_writes_authorized` is always false. This interface has no publisher credential, network effect, branch/commit/PR creation, merge or deployment authority. The separately authorized publisher below owns external effects and uncertain-effect reconciliation. A preparation artifact or inspected review does not complete the repair-to-PR exit scenario.

## Authorized publisher outbox

The preparation artifact still grants no authority. A separate OS-installed policy enables the publisher:

```sh
watchhouse --db /trusted/watchhouse.db publisher-policy --file /trusted/publisher-v1.json --operator operator
watchhouse --db /trusted/watchhouse.db publisher-policies
watchhouse --db /trusted/watchhouse.db case publish CASE --file /trusted/publication-command.json
watchhouse --db /trusted/watchhouse.db publisher-worker --once
```

Explicit policy (example only; substitute a dedicated authorized repository and secret reference):

```json
{"schema_version":1,"revision":1,"repository":"example/authorized-test-repository","base_branch":"main","token_env":"WATCHHOUSE_PUBLISHER_TOKEN","author_name":"Watchhouse","author_email":"watchhouse@example.invalid","draft":true,"paused":false,"job_seconds":120,"request_seconds":10,"reconciliation_attempts":3,"max_revisions":3,"retry_seconds":5}
```

The token is read only in the separate publisher worker. Supply it through that worker's trusted environment; never put it in policy JSON, command JSON, source/model/container environments or candidate workspaces. Grant only the configured repository's Git contents and pull-request authority needed by this adapter. Automatic merging is not implemented by this publisher. An operator pauses new effects by installing the next immutable policy revision with `paused:true`; no model or HTTP command can install policy.

`POST /api/v1/cases/{id}/publications` and `case publish` accept:

```json
{"command":{"meta":{"schema_version":1,"idempotency_key":"publication-command-unique","actor":"operator","reason":"Publish the independently reviewed repair"},"expected_version":42},"candidate_id":"CANDIDATE","review_id":"REVIEW","policy_revision":1}
```

`GET /api/v1/cases/{id}/publications` and snapshot `publications` expose status summaries; the web command selector can enqueue the same versioned command. The authenticated API never dispatches external effects directly. The explicit publisher worker advances one stage per iteration (tree, commit, branch, PR); `--once` performs one iteration. Command receipt replay returns the original result/identity; inspect status for subsequent progress. States include ready, reading, writing, unknown, reconcile, quarantined, blocked, published, published_stale and simulated. Synthetic transport success never becomes native publication authority.

Writes reserve their identity before dispatch. Unknown/expired in-flight effects are supervised without models and reconciled using exact expected objects, branch and PR identity. Reads retry within policy; an unobserved uncertain write is never resent. Duplicate/foreign/closed PRs, changed base/head, stale authority, missing credentials and exhausted budgets produce owned blocks/quarantine. Later revisions reuse the same branch and PR and never force a ref. The shared candidate deadline and configured revision budget cannot be refreshed by retrying.

An OS operator can request another read-only inspection:

```sh
watchhouse --db /trusted/watchhouse.db reconcile-publication CASE JOB --operator operator
```

This rotates the local reservation and cannot replay a write, erase uncertainty, reset automatic budgets, or manufacture absence proof. Keep the worker running for automatic reconciliation; regular coordinator supervision retains escalation when a worker disappears. Separately configured notification delivery handles human escalation. No fixture supplies live credentials, and no live publisher acceptance is claimed. See ADR 0026 and COMPLETION.md.
