# ADR 0003 — Local bearer session and replayable timeline

Date: 2026-11-14. Status: accepted for M0/M1. Completes P05 packaging/access choice.

## Decision

Bind serve to IPv4 loopback only. Generate a per-process local operator token unless WATCHHOUSE_TOKEN supplies at least 32 bytes. Print it to the local terminal, never put it in a URL or commit it. Authenticate every API/evidence request by explicit bearer header. The browser retains it in tab sessionStorage. There is no ambient cookie authority; validate Host and any Origin for both assets and API, with a same-origin content security policy and no-store responses. Actor is an attribution label supplied by the single trusted operator, not an authenticated multi-user identity.

Use /api/v1 commands and snapshots with schema_version 1, strict Serde types, and structured invalid/conflict/not-found/application errors. Standard extractor errors outside command decoding (e.g. invalid query syntax or oversized body) may use framework HTTP errors. CLI invokes the same Application directly against the same local database; it does not need a running HTTP server.

Use server-sent events over authenticated fetch, not EventSource with credentials in a URL. A snapshot includes ordered history and its cursor. Resume with after or Last-Event-ID (header wins); events are queried durably in batches of 200 every 250 ms. IDs/cursors come from SQLite, with no volatile broadcast dependency. On reconnect the UI refreshes the snapshot and renders by unique event ID; the stream covers subsequent events. Browser closure does not affect workers.

## Alternatives, validation, and limits

Polling a mutable case alone loses intermediate facts. A broker or WebSocket protocol adds lifecycle machinery without helping this local slice. Read transactions avoid a snapshot/cursor race; HTTP and browser tests exercise offline writes, header-based replay, reload, and duplicate-free rendering. Untrusted text is rendered with textContent; a browser fixture checks that a script-looking description is inert.

This is a local trusted-operator session, not remote access/RBAC or a production web security claim. Losing a generated token on restart requires reconnecting with the new one; configured tokens support persistent local sessions. Initial snapshots currently include the whole case timeline, and the queue lists the newest 200 cases. History pagination, retention, and load/backpressure are open work. No case content should be considered a grant of authority.
