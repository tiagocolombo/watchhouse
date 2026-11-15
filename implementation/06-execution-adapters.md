# Execution and harness adapters

## Two independent interfaces

Invocation: a person, CLI, web app, Hermes/pi extension, or another client submits commands to Watchhouse.

Execution: Watchhouse dispatches a bounded role task to a configured Claude Code, Codex, Hermes, pi, generic CLI, or optional direct-model backend.

The coordinator owns both paths' authoritative state. Do not let a host harness become the only place incident decisions or shared context survive.

## Proposed adapter contract

~~~text
probe() -> adapter version, runtime version, capabilities, health
start(task, workspace, runtime_selection, limits, authority) -> run handle
events(handle, cursor) -> ordered normalized execution events
cancel(handle, reason) -> cancellation acknowledgment
result(handle) -> terminal outcome, structured output, artifact refs, usage
reconcile(handle) -> running, terminal, unknown
~~~

Probe fields include headless execution, structured results, streaming, cancellation/process-tree termination, session resume, tool/filesystem/network enforcement, model selection, identity reporting, usage/quota reporting, native-platform requirements, and supported authentication modes.

Capabilities must be demonstrated for the installed runtime version. Prompt instructions are not isolation. Fail admission if required capabilities cannot be enforced by the adapter plus worker boundary.

## Task envelope

Task identity and attempt; objective; role; incident/case and briefing manifest; permitted tool/data scopes; workspace identity; expected output schema/version; deadline and progress deadline; budget; approved runtime/profile and optional model selection; required platform capabilities; correlation and lease/fence.

Do not put provider credentials in tasks. Launch arguments are arrays, not interpolated shell strings. Send substantial or untrusted prompts via stdin or a task file. Treat Windows quoting, command-length limits, process trees, and Unicode as contract tests.

## Result and event model

Normalize started, heartbeat, progress, tool activity, artifact produced, approval required, blocked, usage, and completed/failed events. Keep diagnostics separate from protocol output.

Results include terminal status, schema-valid role output, evidence/artifact digests, actual runtime/model identity when available, session reference, usage provenance, and failure code. A successful process exit does not establish a valid task result.

Unknown actual model or usage is recorded as unknown. Estimated token cost is distinct from reported usage, subscription billing, and quota. Subscription access is not zero resource usage.

Failure codes distinguish startup, authentication, quota, timeout, invalid output, policy block, cancellation, lost worker, and unknown outcome. Do not classify failures solely by prose matching when structured signals exist.

## Authentication and subscriptions

Harnesses retain supported authentication, subscription configuration, token refresh, and model routing. Watchhouse selects a configured profile and invokes the runtime through its documented interface.

Do not extract OAuth credentials, spoof clients, or assume a consumer subscription works through every integration. Verify the installed adapter's supported path. Direct API execution is optional, never required for the product architecture.

Configuration separates role, adapter, profile, requested model, fallback policy, limits, and authority. If the model is inherited, record the actual selection when the runtime reports it.

## Limits and independence

Enforce wall time, attempts, process resources, concurrency, and permitted external effects outside model output. Enforce token/currency caps only where reliable usage and cancellation exist; reject tasks requiring a hard monetary cap when that guarantee cannot be provided.

Every verifier/reviewer session begins from an independently assembled briefing and frozen artifacts, without the author's private transcript. Shared evidence is available; hidden deliberation is neither required nor stored.

A runtime's local memory, project instructions, hooks, plugins, and MCP configuration can alter behavior. The adapter must declare and constrain what is loaded. Use trusted policy/instructions outside the candidate's control.

## Adapter rollout

Implement a deterministic fake adapter first, then select one real runtime after a short capability spike. Support Claude Code, Codex, Hermes, and pi as planned adapters, not as already-tested integrations.

Use native machine protocols where available; do not scrape a TUI as the primary contract. Generic subprocess adapters must conform to the same validation, cancellation, and evidence requirements.

Session resume is an optimization. A different harness can continue using the durable handoff package without importing another harness's private session.

## Implemented fake seam and M0 spike

[src/execution.rs](../src/execution.rs) implements a deterministic, side-effect-free FakeAdapter and a versioned assignment/objective Task. The injected Clock supports explicit time advancement in tests. [FakeSource](../src/intake.rs) reads a versioned sanitized fixture, with no provider dependencies. Application validates fake output, correlates it to the current run/assignment/fence, and persists synthetic evidence. There is no harness/model selection surface.

This narrow synchronous trait is not the full start/events/cancel/reconcile production protocol above. [ADR 0004](adrs/0004-execution-protocol-spike.md) records installed Codex help inspection and primary documentation for Claude Code, pi, and Hermes. Codex exec is the first M4 candidate; authentication, subscriptions, isolation, actual execution, usage, and cancellation remain unverified. A real adapter must not be admitted through the fake retry path.

## Fake-worker coordination boundary

Claim packets now include persisted executor/run/fence and an immutable bounded briefing. The core validates heartbeat/progress/block/result messages against current run authority, absolute assignment time, and progress deadline. Accepted handoffs create fresh attempts/fences. Known late results receive a distinct unaccepted-evidence wrapper; identical late output is retained once, not promoted to an accepted result. No actual model or provider credentials exist in these packets.

The CLI supports fake completion, deterministic block, and alive-without-progress scenarios. These are local simulation modes, not new real runtime adapters. Database fencing is sufficient only because fake workers have no mutable workspace or external effects. Actual subprocess cancellation, credential revocation, isolation, uncertain-effect reconciliation, and installed-runtime admission remain required before M4 real execution.

## First M4 preparation slice

[Repair preparation v1](../contracts/v1/repair/README.md) adds immutable operator configuration, object-only frozen base copies, pinned version/help metadata and durable blocked repair requests with owned escalation. Snapshot copies and CLI flags do not establish isolation. Real adapter admission remains denied; start/events/cancel/result/reconcile, qualified worker isolation/revocation and actual model execution are not implemented. See ADR 0015 and STATUS for exact observed evidence.
