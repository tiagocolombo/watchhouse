# Interrupted M4 development checkpoint

Recorded 2026-11-16 after the execution environment became unavailable. This is a recovery note, not an accepted implementation milestone.

## Last verified repository context

The working checkout was on local branch `dev/m4-real-execution`, based on `dev/native-test-bundles` at `d0e460544d67390a28a08ff6405dde7d726825e3`. PR #15 remains open above #14, #13 and #12. No pending PR was merged.

The new native execution code was **uncommitted and not pushed**. Its preservation cannot be verified while the environment is unavailable. Do not claim this checkpoint contains that source or that a feature PR exists.

On environment recovery, inspect the existing working tree before checking out, resetting or recreating anything. Preserve any uncommitted changes. If the checkout is missing, treat the native code as unrecovered and reconstruct it from requirements; do not infer its existence from this note.

## Draft work observed before interruption

New draft files:

- `migrations/017_native_execution.sql`
- `src/real_execution/{mod,protocol,docker,worker,pipeline}.rs`
- `src/bin/watchhouse-worker-init.rs`

Associated edits were made in Cargo.toml, lib/store/contracts/application/team, main/http, repair Markdown rendering, and web app/index files.

The draft contains a strict credential-free worker envelope, immutable Docker policy, fixed container arguments and inspection, durable create/start/cancel reservations, unknown/quarantine handling, bounded cleanup attempts, frozen text candidates, base/candidate regression jobs, independent attributed review, CLI/API commands and a visible operating panel. These are draft mechanisms, not proof the path is correct or complete.

The initializer uses a trusted root parent with only SETUID/SETGID capabilities and a UID 10001 child with no capabilities so the untrusted test process cannot forge the parent's protocol stdout. Qualification measures read-only mounts, namespaces, seccomp, cgroup settings, bounded scratch, and a progressing descendant followed by positively acknowledged exact-container removal. Candidate execution has no model or GitHub authority.

## Validation actually observed

- `cargo check --locked` passed after the draft CLI/API/web/report wiring and formatting.
- `npm run build --prefix web` completed without TypeScript errors in that draft check sequence.
- No new domain/controller/browser tests were written or run for this draft. Clippy and complete cross-platform CI were not run for it.
- The actual existing Bubblewrap qualification was rerun and still failed closed: zero verified capabilities, quarantine true, real execution admission false. A basic namespace command succeeding did not change that result.
- Docker/Podman and a usable Docker cgroup environment were unavailable on this host. No native Docker qualification or regression was verified.
- Installed Codex version/help/features were inspected only. No real model, account, paid request, notification, Sentry integration, GitHub delivery or automatic merge was exercised.

The prior published stack's 140 Rust tests, six browser scenarios and native packaging CI are baseline evidence for that stack only. They do not validate this uncommitted draft.

## Critical follow-up review before any feature commit

A final edit command was rejected by the unavailable execution environment. Its changes are **not confirmed applied**. Inspect the files and address these items rather than assuming they are present:

1. Require an explicitly operator-installed case policy before native work; the compiled bootstrap profile is for synthetic work.
2. Validate qualification/admission evidence integrity and the latest qualification generation; new/failed qualification must not leave old admission usable.
3. Verify the base regression's complete frozen manifest, policy identity and content-addressed evidence when evaluating candidate review.
4. Handle malformed worker output with durable unknown/quarantine/escalation rather than leaving the reservation stranded on an error.
5. Count all actual cleanup attempts, preserve cleanup uncertainty, and ensure retries cannot reopen a terminal result. Review late create/start/cancel races and reconcile-after-crash behavior.
6. Check remaining shared repair wall time before queuing a new check. Candidate revisions cannot reset either attempt or overall budgets.
7. Confirm the host canary is held during the real probe, the descendant actually progresses, and recovered logs cannot recreate missing host/cancellation proof.
8. Verify positive removal after successful ordinary tests as well as probes; do not leak completed container resources or accept missing-container lookups as cleanup acknowledgments.
9. Review exact Docker inspection, client/image/initializer and engine/kernel/boot identity, root/child privilege separation, resource enforcement and bounded output under adversarial inputs.
10. Add deterministic controller/protocol/crash/race/rollback/idempotency tests and a separate native no-model Docker drill on a capable CI runner. Fixture backends must not become selectable native-admission overrides.
11. Finish model adapter and GitHub delivery/reconciliation after their prerequisite boundary is proven. Neither exists in this draft.

Run formatting, strict Clippy, meaningful Rust and HTTP/browser tests, native compilation and actual container acceptance before committing the feature phase. Update ADRs, contracts, STATUS and the completion ledger with observed evidence. The documentation-only completion-gates branch does not contain migration 017 or native execution code.
