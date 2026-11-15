# M3 activation drill — prepared, not executed

This runbook is an operator handoff. Repository tests have exercised synthetic contract fixtures and injected transports only. No live evidence or successful activation is recorded here.

## Required inputs

- One explicitly authorized test service, environment, repository and exact release-to-commit mapping, with an accountable operator.
- Sanitized historical issue/event examples, including a known expected response, a genuine defect, a duplicate grouping, an out-of-order update and a missed delivery. State how they were sanitized and the real scope/time window; do not commit customer payloads, identities or secrets.
- Verified Sentry Cloud application/install/plan read permissions, webhook secret and read-only API token injected through the configured environment references. See the [source contract](../contracts/v1/sentry/README.md).
- Trusted reachable ingress compatible with the current loopback Host/Origin boundary. Public relay/TLS deployment remains separate work; do not expose the operator API as a shortcut.
- Authorized human test destinations, Slack incoming-webhook secret references and fallback chain, with participants expecting the test. See [notification configuration](../contracts/v1/notifications/README.md).

Missing any input leaves that live step blocked. Continue offline verification independently. Never record a successful live result from simulated responses.

## Concrete drill and evidence to retain

| Step | Action | Evidence / acceptance |
| --- | --- | --- |
| 1 | Install reviewed source/policy/notification revisions through the trusted operator CLI. | Configuration revisions and digests, service/release scope, credential-reference names only; no secret values. |
| 2 | Deliver an authorized test issue notification; repeat the same body. | Durable acknowledgment and receipt; one source case, no automatically declared incident. Invalid signatures rejected. |
| 3 | Read enrichment, then deliver an older observation. | Current case source digest, API provenance, normalized allowlist contents, read generation; older observation cannot regress projection. |
| 4 | Intentionally omit a test delivery, then run bounded source reconciliation and known-issue refresh. | Requested discovery window/completion, known-issue read state and recovered observation; no duplicate case/incident. State retention/plan gaps. |
| 5 | Qualify expected behavior using the actual scoped request contract; compare related issue cases. | Operator evidence on both cases for duplicate judgments, exact versions/digests, rationale, uncertainty and reconsideration deadline. A status code or suggestion alone is insufficient. |
| 6 | Declare a coordinated test incident only if authorized. Record update, hypothesis/experiment proposal and an independently reviewed unsent draft. | Incident/operations revisions, addressed role acceptance where needed, owned deadlines, exact draft context and distinct reviewer. This sends no stakeholder message. |
| 7 | Stop model executors and force an owned test deadline to expire using a reviewed short test policy. Exercise primary delivery failure/fallback if the operator authorized both recipients. | Persistent escalation/delivery attempts, actual destination receipt, human acknowledgment, retry/fallback behavior and explicit resolution. Ack is not resolution. |
| 8 | Interrupt coordinator/source access, restart and reread. | Pending intake/duties retained; failed reads visible; old snapshots stay explicit; duplicate commands do not repeat accepted effects. |
| 9 | Revoke one correlation and change another linked source's facts. | Immutable pair revisions in both timelines; revoked/stale state visible after refresh; duties/incident recovery unchanged. |

For each step retain UTC start/end, operator, config revisions, fixture/history provenance, case/incident IDs, command keys, event cursors/evidence digests, observed result and any uncertainty. Link actual provider delivery evidence only after it exists. Remove test notification settings through a new trusted revision when finished.

This drill cannot establish deployment or service recovery; those need M5 criteria, sufficient traffic and observation windows. Current code supports only bounded Sentry Cloud issue snapshots and one representative event, not complete historical replay. Actual service-history acceptance and live results must be added to STATUS with evidence before marking M3 accepted.
