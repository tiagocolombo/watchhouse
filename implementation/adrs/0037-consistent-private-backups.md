# ADR 0037: Consistent private backups before authority-safe restore

Status: accepted implementation choice, 2026-11-21.

M8 needs more than a casual copy of a live WAL database. Export one SQLite `VACUUM INTO` snapshot, then copy and verify its immutable declared source/candidate artifacts. Retain configuration/secret references and frozen identities; never copy whole worker directories, auth homes, executable tools or unrelated files. Bound inventory, bytes and paths. Write the manifest last, never overwrite, retain failed directories, and provide read-only inspection pinned optionally to an independently retained manifest digest.

The OS filesystem owner selects the destination. No HTTP request can select paths or obtain private bytes. Unix uses private modes; Windows requires a private parent ACL. Successful creation records metadata in the existing append-only factory audit, exposed by CLI/API/web details. Export does not alter existing case receipts/versions/deadlines.

Verification checks database integrity, foreign keys, complete migration history, evidence content hashes and every declared frozen artifact. Digests detect corruption, not producer authenticity without an independently trusted expected digest. Existing policy ownership remains the trust boundary; this does not defend against a hostile administrator racing file replacement.

Implement creation/inspection as a complete independent vertical slice. Keep restore explicitly unavailable until old leases, source/notification credentials, native handles, policy revisions and uncertain external effects can be isolated and reconciled without replay. Merely restoring a database under a global resumable pause would inherit too much authority. This staging does not close M8's restore gate.

Validation and exact-head CI are recorded in STATUS.md and the implementing PR. No live account, paid model or external effect is invoked by backup fixtures.
