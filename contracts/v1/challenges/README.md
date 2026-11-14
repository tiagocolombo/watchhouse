# Independent review v1

`team challenge CASE_ID --file FILE` and `POST /api/v1/cases/CASE_ID/challenges` use the same strict command. The web review panel accepts that JSON and shows the persisted records. Actor names are simulated local roles under the trusted operator/OS boundary, not worker authentication.

```json
{
  "command": {"meta": {"schema_version":1,"idempotency_key":"review-1","actor":"fake-challenger","reason":"Validate the exact claim"},"expected_version":2},
  "action": {"type":"open",
  "target_event_id":"REPLACE_WITH_EVENT_ID",
  "target_version":1,
  "recipient":"fake-investigator",
  "arbiter":"fake-verifier",
  "requested_evidence":"Reproduce the claim with a separate experiment",
  "uncertainty":"No service has been inspected",
  "blocking":true,
  "acknowledge_seconds":30,
  "response_seconds":120}
}
```

Keep `command` and replace the fields inside `action` for transitions:

| Action `type` | Fields | Required role/state |
| --- | --- | --- |
| `acknowledge` | `id` | recipient; open, before acknowledgment deadline |
| `investigate` | `id` | recipient; acknowledged |
| `answer` | `id`, `answer`, `evidence_refs` | recipient; acknowledged/investigating before response deadline |
| `resolve` | `id`, `disposition`, `uncertainty`, `evidence_refs` | independent arbiter; answered/escalated |

Each transition uses a fresh key and current expected case version. Resolution requires a separately recorded verifier observation, including evidence absent from the answer. Original answers and all residual uncertainty remain in the event history. `tick` escalates missed deadlines once; escalation does not clear a blocking challenge. The arbiter can resolve an escalated challenge with evidence.

`team gate CASE_ID --file FILE` / `POST .../gate` accepts `{"command":{...},"close":false,"evidence_refs":["sha256:..."]}`. The fake verifier must supply its own attributed evidence after investigation completes. `close:true` requires the accountable case owner, no unresolved blocking challenge or pending review obligation, and validation of the immediately preceding case version. Any intervening case event invalidates validation/closure; historical gate records do not suppress monitoring. This closes only the reviewed case assessment, never declares incident recovery or grants execution authority.
