# Configured HTTPS access v1

Implemented in migration 033 and ADR 0039. This slice provides global observer, operator and administrator identities. Service-owner data scopes and independent remote-worker transport remain separate development, not live-activation waivers.

## OS configuration and transport

Install a JSON policy with `watchhouse --db state.db access-policy --file /private/access.json --operator operator`; inspect with `access-status` and `access-audit`. Revisions advance consecutively and are immutable, including exact retry behavior. No HTTP endpoint installs policy or credentials. Agents and model identities cannot install it.

```json
{"schema_version":1,"revision":1,"public_origin":"https://watchhouse.example","proxy_token_env":"WATCHHOUSE_REMOTE_PROXY","max_audit_rows":10000,"paused":false,"principals":[{"id":"alice","role":"operator","token_env":"WATCHHOUSE_REMOTE_ALICE","disabled":false},{"id":"observer","role":"observer","token_env":"WATCHHOUSE_REMOTE_OBSERVER","disabled":false},{"id":"administrator","role":"admin","token_env":"WATCHHOUSE_REMOTE_ADMIN","disabled":false}]}
```

All references must be dedicated `WATCHHOUSE_REMOTE_` environment names. Supply distinct random 32..512-byte bearer values outside policy files. Proxy and enabled principal values must all differ; missing/invalid references fail closed. Disabled principals need no credential. At least one enabled administrator is required unless paused. Up to 64 principals and 100..1,000,000 immutable audit intents are supported. Configuration contains references, never credential values.

`serve --remote` retains a loopback-only HTTP backend. Configure an OS-owned TLS-terminating reverse proxy on the same trusted host; prevent access to its private marker and coordinator environment. The proxy must overwrite, not append or trust client values for, `Host` (actual backend authority), `X-Forwarded-Host` (configured public origin authority), `X-Forwarded-Proto: https` and `X-Watchhouse-Proxy-Token` (proxy credential). Preserve the client's Authorization and Origin. Reject or remove duplicate forwarding headers at ingress. Watchhouse rejects duplicates itself. This is an explicit trusted-proxy transport assertion, not native server TLS or cryptographic TLS-peer verification. A hostile host administrator is outside this boundary. Direct loopback callers still need proxy and principal credentials.

The public origin must be a canonical HTTPS origin without a trailing slash/path/query/userinfo. An Origin, when present, must match it exactly. No cookie authority or permissive CORS exists. HSTS, CSP, no-store, nosniff and no-referrer accompany accepted transport responses. The local shared token is never a remote fallback. Plain `serve` keeps the existing local attribution interface.

## Capabilities and identity

Authenticated `GET /api/v1/session` returns principal, role, policy revision and UI capabilities; it returns no secret/hash. Observer credentials can inspect global operational records and request a read-only Hallmark export. Operators may submit the enumerated case commands; administrator credentials additionally control factory pause/stop/resume. New mutation routes deny remote access until explicitly assigned a capability. None of these human credentials can use the fake-worker claim/progress/result/accept-handoff APIs. OS installation and independent worker trust are separate capabilities.

Every mutating command's existing actor metadata must match the credential identity exactly (top-level, meta and nested command metadata are checked together). Bodies are forwarded byte-for-byte after checking; command fingerprints, expected versions, original receipts, domain ownership, independent reviewers and separately installed effect authority continue to govern. An administrator cannot impersonate another reviewer. Case-owner escalation addresses the actual human owner, retaining the historical operator fallback for fake/model owners. The web app uses the authenticated identity and disables ungranted form controls. Generic JSON commands still require the caller's correct actor.

Policy currency and actual key digest are checked on requests and again after bounded body reads. Key replacement, disabling a principal, new policy revision, pause or restored-authority fence revokes future admission. SSE rechecks before each 250ms event batch and terminates with `access_revoked`; an already authorized batch/request can finish. Browser storage remains per-tab sessionStorage; issuing/revoking credentials and proxy certificate/domain provisioning are OS tasks. No identity provider or account enrollment is claimed.

## Bounds and audit

Remote ingress has fixed process-local bounds: 64 requests/second, 64 concurrent request preparations and 16 active event streams. Body reads are limited to 32 KiB and ten seconds. The reverse proxy must also impose TLS/header/connection/time limits; backend framework/socket limits are not a complete Internet edge defense.

Before authenticated operational reads and mutation dispatch, reserve a durable append-only intent containing principal/role/revision, method, route template, resource-path digest, optional exact body digest and decision. Record HTTP status separately; never retain raw bodies, query strings, tokens or headers. Session metadata, unauthenticated/transport rejection and pre-audit body-limit/time failures are not included. Request IDs link responses to intents. HTTP status proves server response construction, not receipt by the browser. An interrupted completion remains unknown; the original domain command receipt governs retry. No success is fabricated. OS audit shows the newest 200 intents; factory health counts incomplete intents older than 30 seconds.

Audit admission is transactional and bounded across restarts. When full, remote operational access returns 429 before mutations; local CLI, timers, escalation and reconciliation remain available. Session metadata remains available for reconnect diagnosis. OS export/retention and explicit capacity revision are required before more remote records; this slice does not prune audit or claim whole-disk retention. Incomplete audit raises degraded health; exhausted configured audit raises limited health. Restores permanently hold copied access revisions; install a fresh consecutive OS revision and revalidate the proxy/identities before remote startup.

## Verification classes

Rust fixtures inject synthetic credentials, including distinct-key/proxy/origin/duplicate-header denial, roles/actor spoofing/worker denial, exact receipt retry, live stream revocation, key rotation, body bounds, lost completion health, audit exhaustion with continued local duties, owner escalation and restore isolation. A browser fixture runs an actual local TLS proxy with a disposable self-signed certificate and synthetic principals. It verifies UI capability/identity, case ownership, spoof denial, observer inspection and revocation. It does not verify public hosting, certificate issuance, real identity enrollment or any live incident integration.
