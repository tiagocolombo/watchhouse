# ADR 0039: Bind configured human API access to identities

Status: accepted implementation choice, 2026-11-21.

Keep loopback-only local operation and add an explicit remote mode behind an OS-owned HTTPS reverse proxy. Require a separate proxy assertion credential and distinct credential references for human principals. Missing, duplicate, reused, disabled, paused or restored credentials deny admission. The API never installs its own policy or provisions accounts. This is a trusted proxy boundary; direct TLS termination and external identity enrollment are not inferred.

Use global observer/operator/admin capabilities as the first access slice. Bind existing actor metadata to the principal rather than granting arbitrary attribution. Keep domain ownership, independent review, command receipts and all execution/publication/merge/runbook gates. Deny the legacy fake-worker API to every remote human role. Scoped service-owner visibility and independent remote worker trust/transport remain required subsequent development.

Migration 033 retains immutable policy revisions and bounded reserve-before-dispatch audit intent/status. Audit excludes raw credentials/payloads and distinguishes server response from client delivery. Lost completion stays unknown without invalidating an already committed domain receipt. Exhaustion blocks remote admission while OS CLI and deterministic duties continue; health makes exhaustion/incomplete completion visible. Fixed rate/request/stream/body bounds supplement, but do not replace, trusted ingress limits.

Revalidate current policy/key on requests, after body reads and before each SSE batch. Already authorized work may complete; revocation does not roll it back. Restored policy revisions stay permanently fenced until fresh OS configuration. The browser uses authenticated identity/capabilities, with the server enforcing authorization independently of disabled controls. Human-owned escalation uses the case owner, with the existing operator fallback for worker-owned cases.

Controller fixtures and an actual disposable localhost HTTPS browser fixture provide distinct recorded proof. No production proxy, actual account, public certificate or live integration is activated by the change. See the [access contract](../../contracts/v1/access/README.md).
