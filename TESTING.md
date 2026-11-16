# Test Watchhouse locally

The native test bundle contains the coordinator with its web interface embedded, a synthetic demo database, formal reports, a source/platform manifest and SHA256SUMS. Rust, Node and model credentials are not needed to run the bundle. These are CI debug builds for operator testing; installers, signing, wider runtime compatibility and production worker admission remain later work.

## Start the demo

Extract the bundle for your operating system and architecture into a new directory. Open a terminal there.

macOS/Linux:

```sh
./watchhouse --db demo/gallery.db serve --manual-workers
```

Windows PowerShell:

```powershell
.\watchhouse.exe --db demo/gallery.db serve --manual-workers
```

Open http://127.0.0.1:8787, enter the operator token printed by the process, and select the checkout case. Keep the process running while using the interface. The web page observes the core; closing the browser does not stop timers or coordination.

The demo is fictional. It includes a signed synthetic source delivery, incident roles, proposed causal hypothesis/unexecuted experiment, unsent communication, blocked fake investigator and acknowledged specialist handoff, operator-reported recovery with independent attributed review, a continuing corrective action, accepted postmortem and scoped lesson. Unknown total impact and cause remain unknown. Synthetic completion is not an actual repair or production recovery. Distinct role names represent one local operator trust domain.

The generator dates the active recovery/knowledge scenario near build time. Review/expiry deadlines continue to run; old bundles can correctly show stale knowledge and overdue actions. Extract a fresh bundle for a clean scenario, or use the versioned command contracts to record a new plan/review. No live Sentry, Slack or model access is configured.

## What to exercise

1. Inspect the timeline, source freshness, roles, investigation, fake handoff, recovery, postmortem and lesson. Read the included Markdown reports.
2. Use the evidence inspector to distinguish source fixtures, operator reports and synthetic execution.
3. Submit a versioned command using the current case version. Repeat the same key to inspect idempotency; try an obsolete version and confirm it conflicts.
4. Open a blocking challenge and confirm review/acceptance is blocked. The challenge contract uses admitted fake challenger/verifier identities for this simulation.
5. Reopen recovery, define a new plan, and report low traffic or incomplete coverage. It must stay inconclusive. New accepted recovery needs independent attributed evidence and review.
6. Submit corrective-action proof, then verify with the assigned verifier's own new evidence. An owner's claim alone cannot close it. Overdue duties remain after response resolution.
7. Draft a new lesson revision and confirm it does not replace the prior accepted version. Expired/retired/stale-context entries must not enter new briefings.
8. Close/reopen the browser, restart the coordinator, and confirm committed records and obligations persist. Timers may add overdue escalations.

The command schemas are in the repository's `contracts/v1` folders; the web form accepts the same JSON. `case show CASE_ID` prints current state. `case incident-markdown`, `case recovery-markdown` and `case learning-markdown` render records. Use `--help` for command arguments. The server binds loopback and authenticates its local API.

## Fresh workspace

From the extracted directory, omit `--db demo/gallery.db` to create a separate default local workspace:

```sh
./watchhouse serve
```

Use `watchhouse.exe` on Windows. New manual cases use fake investigation. Actual repair execution, publication, merging and production actions remain unavailable until their own integrations and authority are implemented/configured. Testing these local records does not establish live integration compatibility.

## Report an issue

Include source_commit/system/machine from manifest.json, steps, expected/actual result and sanitized relevant case/event IDs. State whether it affects a fresh demo or a new workspace. Exclude operator tokens, credentials, production payloads and personal data. Preserve the original database before investigating a state issue; backup/restore tooling is not yet implemented.

For source development: `npm ci --prefix web`, `npm run build --prefix web`, `cargo build --locked`. Generate a new demo with `cargo run --locked --example generate_gallery -- /absolute/empty/directory`. Run Rust/browser checks listed in README. Ordinary checks need no model, production access or notification destination.
