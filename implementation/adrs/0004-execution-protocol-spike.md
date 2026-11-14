# ADR 0004 — Fake-only admission; first real adapter candidate

Date: 2026-11-14. Status: accepted for M0; real compatibility remains unverified. Resolves the M0 protocol-selection spike, not M4 integration.

## Findings

| Harness | Evidence checked | Candidate boundary | Still unverified |
| --- | --- | --- | --- |
| Codex | Installed codex-cli 0.167.0-alpha.4; codex exec --help | exec, stdin prompt, --json event stream, --output-schema final result; profile/model/sandbox options are advertised | Runtime execution, authentication mode, billed usage, full process-tree termination, configuration containment, OS behavior |
| Claude Code | [Official headless guide](https://code.claude.com/docs/en/headless), read 2026-11-14 | Noninteractive -p and structured output are documented | Installed version, actual auth/profile behavior, cancellation, isolation, usage guarantees |
| pi | [Official RPC guide](https://github.com/earendil-works/pi/blob/main/packages/coding-agent/docs/rpc.md), read 2026-11-14 | JSONL stdin/stdout subprocess protocol, distinct command acknowledgment and run completion | Installed version, actual event framing/cancel behavior, configuration containment and authority |
| Hermes | [Official CLI reference](https://hermes-agent.nousresearch.com/docs/reference/cli-commands), read 2026-11-14 | Scripted one-shot CLI is documented; profile selection stays within adapter | Installed version, machine output and terminal semantics, auth/cancel/isolation behavior |

Only the Codex binary was present. No model was invoked, no account was probed, and no subscription compatibility was inferred. Source documentation is moving; pin actual runtime versions and capture sanitized protocol fixtures at M4.

## Decision and consequences

Retain all four as planned adapters. Use Codex exec as the first M4 candidate because a machine protocol can be inspected locally; final admission depends on a fresh installed-version capability test after M2 recovery/isolation exists. Do not create a generic shell adapter or translate flags into domain entities.

The implemented ExecutionAdapter trait is deliberately a narrow synchronous fake seam, not the full production protocol. Fake-v1 accepts a versioned objective/assignment task, returns a bounded versioned synthetic result, reports no actual model/usage, and has no filesystem/network/tools/effect access. Completion validates that output under a fixed fake-only policy before any authoritative write. It does not claim check success, service inspection, or recovery.

A real adapter must add probe/start/events/cancel/result/reconcile, explicit capability admission, frozen briefing/authority/limits, worker isolation, and tested cancellation before registration. Mutable side effects require reconciliation and cannot inherit the fake's automatic retry semantics. No real adapter can be selected by the current CLI or web app.
