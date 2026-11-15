# Human notification v1

The implemented channel is Slack incoming webhooks. A trusted operator explicitly installs the versioned configuration with `watchhouse notifications --file config.json --operator operator`. Copy the [example](config.json), replace recipients with actual accountable human labels, and inject each referenced webhook secret into the coordinator environment. Secrets/URLs are never persisted or returned in snapshots. No live destination is shipped or inferred from a case owner.

Only HTTPS `hooks.slack.com/services/...` is admitted; redirects, arbitrary hosts, credentials in authority, query strings and fragments are rejected. The adapter sends only case/escalation/delivery identifiers, no telemetry or agent-provided prose. A webhook is bound to its configured Slack conversation; `owner` records the human responsible for that destination and must be verified by the operator. Creating a config grants authority to send these internal escalation notices to those destinations.

`serve`, workers and `tick` run the deterministic delivery supervisor without models. Existing escalations freeze the configured chain. Queued effects are claimed with leases/fences outside the sending transaction. HTTP 200 `ok` means Slack accepted a request, not human acknowledgment. 429 `Retry-After` schedules bounded retries; permanent rejection advances fallback. Network/5xx/unknown responses or sender loss become uncertain and are never blindly resent. Incoming webhooks lack a provider delivery reconciliation/idempotency lookup; the operator must inspect the stable delivery ID in Slack and reconcile with evidence. Different fallback recipients can receive the same escalation; this is intentional escalation, not retry of the same operation.

Acknowledgment uses the existing `team acknowledge-escalation CASE_ID --file FILE`; the local inbox owner or current configured recipient may acknowledge under the existing local operator-token/OS trust boundary. This is not remote Slack identity verification. Missed acknowledgment or response advances the chain; exhaustion remains visibly unresolved. Acknowledgment never closes the escalation.

`team resolve-escalation CASE_ID --file FILE` / `POST .../team/escalation-resolve`:

```json
{"command":{"meta":{"schema_version":1,"idempotency_key":"resolve-1","actor":"primary-human","reason":"Human accepted responsibility for the next action"},"expected_version":42},"escalation_id":"REPLACE","outcome":"Owned follow-up recorded","uncertainty":"Service outcome unknown","evidence_refs":["REPLACE_WITH_CASE_REPORTED_EVIDENCE"]}
```

`team reconcile-delivery CASE_ID --file FILE` / `POST .../team/delivery-reconcile` takes `command`, `delivery_id`, `delivered` and `evidence_refs`. Only the current accountable operator can reconcile uncertainty. Confirmed non-delivery advances fallback; it does not reset attempt budgets. Resolution/acknowledgment/delivery are separate facts. Resolving escalation duty cannot resolve an underlying challenge, assignment block or incident. Snapshots and the web review panel show deliveries and resolved escalation records; use the versioned command form for explicit resolution/reconciliation.

Adapter contract references: [Slack incoming webhooks](https://docs.slack.dev/messaging/sending-messages-using-incoming-webhooks/) and [rate limits](https://docs.slack.dev/apis/web-api/rate-limits/). Tests substitute sanitized deterministic transport outcomes and never invoke a live destination. Live activation requires verified human destinations, injected Slack webhook secrets and explicit operator configuration. No live delivery has been verified.
