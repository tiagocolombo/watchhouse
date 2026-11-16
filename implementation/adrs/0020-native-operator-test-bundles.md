# ADR 0020: Native coordinator test bundles for operator testing

Accepted 2026-11-15, stacked on PR #14. Package the implemented coordinator/UI and an inspectable synthetic scenario before full live execution is available.

Extend existing Linux/macOS/Windows CI after ordinary validation to generate the real application-service demo, package the native debug executable, TESTING.md, reports/database and source/platform/hash manifest, then extract/execute-check the archive on that native runner. Upload platform/architecture-specific test artifacts with bounded retention. The checker validates fixed names/digests and runs only trusted CI-produced bundles; hashes are integrity checks, not signing or source authentication.

Use a near-current demo clock with its reported window in the past so new bundles contain usable acceptance/expiry examples. Old bundles may correctly show expired knowledge and overdue duties. The real coordinator runs without Rust/Node installed because UI assets are embedded. A fresh default workspace remains separate from the included demo database.

This completes a concrete local testing path, not M7/M8 or general release distribution. Builds are debug-profile, unsigned, native-runner artifacts; installers, broader OS/library compatibility, license/distribution decisions, second real adapters, backup/restore, remote RBAC and production hardening remain open. No product worker isolation/model access/production recovery is established by CI executing its own compiled tests/demo. No existing PR is merged or release published by this packaging workflow.
