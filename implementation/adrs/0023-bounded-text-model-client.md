# ADR 0023: Native bounded text-role client

Status: implemented controller and explicit native test path; native CI acceptance pending. Live account/model acceptance remains open.

## Context

M4 requires a real replaceable executor, resource/cancellation proof, scoped supported authentication and durable recovery. The previous Codex spike showed its pinned CLI advertises `request_user_input` even with collaboration disabled, so the strict tool-free profile correctly remains denied. Candidate code must not execute in a model credential domain.

## Decision

Introduce a separate text-only Codex adapter profile. Its role output is strictly typed proposal/review data, not workflow commands. Shell, file, web, MCP/app/plugin/hook, computer/image and collaboration execution features are disabled by fixed adapter arguments; repository/user rules are ignored and cwd is a trusted empty directory. Only the inert question function may be advertised; actual question/tool/plan/unknown events block output. The no-account native qualification endpoint measures that exact advertisement. This does not claim a tool-free CLI or a verified live model. Unknown actual model/usage remains unknown.

Aggregate cgroup-v2 memory/swap/pids/CPU constraints and a whole-subtree kernel cancellation mechanism fence the trusted client plus detached descendants. The worker runs unprivileged in an explicitly delegated private subtree. No process-name killing, root execution, missing-as-success or local fallback establishes admission. A trusted detached initializer joins before spawning any client. Its watchdog kills the whole group independently of the coordinator. A durable cancellation barrier checked before/after attachment closes delayed-start races. Native qualification must observe a progressing descendant, populated group before cancellation and an empty group afterward; natural watchdog exit cannot pass this cancellation proof.

The OS operator installs immutable pinned policy and retains supported harness-owned authentication in place, separate from source, container staging and model jobs. Ordinary tests use fake resources. Native qualification uses actual Codex with an empty fixture auth home and synthetic loopback SSE, never a paid account or external model. Qualification binds exact profile/runtime pins, kernel boot identity, evidence and expiry. It does not prove provider billing/authentication or live success.

SQLite reserves every creation/start/poll/cleanup before I/O, stores exact scoped identities and retains sealed initializer journals. Restart reconciliation inspects existing work. It never automatically repeats an uncertain possibly charged call. Cleanup attempts share one budget across manual and automatic paths; failure keeps an owned quarantine/escalation. Output is staged evidence until whole-resource cleanup succeeds and current context/policy/knowledge/challenges still allow acceptance. Operator-reported cleanup needs fresh evidence and accepts no result/capability.

## Consequences

Linux requires explicitly delegated cgroup-v2 controllers and a pinned supported native client. This workspace lacks the mounted hierarchy, so it cannot provide physical native proof; CI must do so without changing global namespace/sysctl policy. Other platforms fail closed for this model boundary until their own native profiles exist. The web interface exposes durable states and inert role artifacts, not private reasoning or credentials. Explicit candidate adoption, model-review lineage, GitHub delivery and M4 end-to-end completion remain subsequent work; this adapter does not silently grant them.
