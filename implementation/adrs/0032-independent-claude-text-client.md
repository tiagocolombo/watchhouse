# ADR 0032: Independent Claude Code text adapter and model profiles

Add a fixed pinned Claude Code 2.1.329 bare/restricted tool-free adapter through the existing durable text-role worker lifecycle. Share semantic role schemas without pretending its stream is Codex protocol. Preserve actual session/model identity, reject capability/tool/interaction/terminal drift and keep private reasoning/stderr prose out of records. New profiles require independent current cgroup/protocol qualification and never inherit another runtime's admission.

Active authority is independent per adapter/profile identity. Omitted legacy fields preserve Codex/default serialization and supersession behavior. Cross-harness authors/reviewers retain frozen shared case/repair/evidence context, separate sessions, original budgets and no author transcript.

The supported Claude mode uses an OS-referenced API key. Bare mode excludes stored subscription/keychain authentication; do not claim consumer subscription support or copy its credentials. Existing supported Codex authentication remains unchanged. Additional Claude auth modes require their own compatibility/admission proof.

Actual local native CLI compatibility was exercised without an account against synthetic Messages SSE. Parser/controller fixtures are distinct from physical cgroup proof and live model/account acceptance. The implementing final-head CI must run the independent native Claude resource/cancellation/watchdog drill. No production authority, publication or merge follows from a transport probe.

Primary references inspected 2026-11-20:
- https://code.claude.com/docs/en/cli-reference
- https://code.claude.com/docs/en/headless

M7 native worker capability routing/platform acceptance and the optional Hallmark repair spike remain separate work; M8 remains open.
