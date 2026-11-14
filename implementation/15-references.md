# References and verification notes

These primary sources informed the operating model. Watchhouse's detailed schemas, architecture, and default policies are proposals for this product, not claims of certification or exact copies of any company's process.

Reviewed during the design discussion on 2026-11-14. Runtime documentation and subscription support can change; inspect the installed version and current official guidance before implementing adapters.

## Incident management and SRE

- [Google SRE Workbook: Incident Response](https://sre.google/workbook/incident-response/) — command, operations, communication, coordination.
- [Google SRE Book: Managing Incidents](https://sre.google/sre-book/managing-incidents/) — living incident records, response structure, handoff.
- [Google SRE Workbook: Postmortem Culture](https://sre.google/workbook/postmortem-culture/) — blameless analysis and systemic improvement.
- [PagerDuty: During an Incident](https://docs.pagerduty.com/ops-guides/incident-response-guide/during-an-incident) — delegated response and incident coordination.
- [PagerDuty: Postmortem Template](https://docs.pagerduty.com/ops-guides/incident-response-guide/postmortem-template) — operational review structure and follow-up.

Severity numbers, acknowledgment times, update cadence, and mandatory review thresholds are organization-specific configuration. No external template is a substitute for actual evidence.

## Integrations and runtimes

- [Sentry integration webhooks](https://docs.sentry.io/integrations/integration-platform/webhooks/) — raw-body signature verification and response requirements.
- [Sentry issue webhooks](https://docs.sentry.io/integrations/integration-platform/webhooks/issues/) — lifecycle notifications.
- [Sentry issue-alert webhooks](https://docs.sentry.io/integrations/integration-platform/webhooks/issue-alerts/) — alert-action notifications.
- [Sentry error webhooks](https://docs.sentry.io/integrations/integration-platform/webhooks/errors/) — plan-specific availability.
- [Pi RPC documentation](https://github.com/earendil-works/pi/blob/main/packages/coding-agent/docs/rpc.md) — machine-oriented subprocess integration.
- [Hermes CLI documentation](https://hermes-agent.nousresearch.com/docs/reference/cli-commands) — scripted execution and profile selection.
- [Hermes providers](https://hermes-agent.nousresearch.com/docs/integrations/providers) — runtime-owned provider configuration.
- [Hermes Claude subscription plugin](https://hermes-agent.nousresearch.com/marketplace/claude-subscription-directsdk) — documented experimental integration; validate current support before use.

Claude Code and Codex adapters are product requirements, not verified implementations in this repository. Their current official protocol/authentication references must be added during the capability spike. Datadog adapter documentation must likewise be verified when selecting the actual integration endpoints.

## Hallmark

Repository inspected: [tiagocolombo/hallmark](https://github.com/tiagocolombo/hallmark).

- [Runner implementation](https://github.com/tiagocolombo/hallmark/blob/main/src/runner.rs).
- [ADR 0006: Claude execution](https://github.com/tiagocolombo/hallmark/blob/main/docs/decisions/0006-claude-p-for-every-model-run.md).
- [ADR 0013: Common runner](https://github.com/tiagocolombo/hallmark/blob/main/docs/decisions/0013-one-runner-for-claude-p.md).
- [ADR 0014: Workers and leases](https://github.com/tiagocolombo/hallmark/blob/main/docs/decisions/0014-daemon-workers-and-leases.md).
- [ADR 0027: Local Library](https://github.com/tiagocolombo/hallmark/blob/main/docs/decisions/0027-the-library-stays-local.md).
- [Knowledge implementation](https://github.com/tiagocolombo/hallmark/tree/main/src/library).
- [Import/export implementation](https://github.com/tiagocolombo/hallmark/blob/main/src/library/portable.rs).

The inspected design has Claude-specific execution and per-clone knowledge. A reusable standalone library or complete repair protocol must not be assumed to exist. Pin a commit when implementing compatibility; main-branch links are orientation references.
