# ADR 0025: Prepare exact Git identities from reviewed candidates

Status: implemented and locally validated as read-only preparation; external delivery remains open.

A publication stage must not let builder-supplied text or a stale review select different remote content. Add read-only delivery preparation over the existing current candidate/review gates. Compute the complete expected Git tree from bounded, canonical, verified candidate files and derive the partial remote tree payload over its exact base. Reject parent-directory symlinks, collisions, content drift and unsupported identities. No repository command or candidate code executes in the preparer.

CLI, authenticated API and inert review-card inspection share one application service. The artifact records candidate/review/base/manifest identity and explicitly grants no external writes. A later separate publisher must reserve effects durably, validate exact returned identities and reconcile uncertain branch/commit/PR creation. Publishing credentials stay outside candidate and model processes; merge remains separate authority.

Tests compare computed trees with real benign Git fixtures, including nested ordering and binary files, and reject mutation/symlink/payload drift. Controller coverage reuses exact native-shaped regression/review evidence to prove preparation accepts only the current reviewed artifact and rejects subsequent file changes. These fixtures are not live GitHub publication or live-model evidence. The complete M4 exit remains open.
