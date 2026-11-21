# Consistent backups v1

The OS operator plane can create and verify a private, consistent export:

```sh
watchhouse --db /private/watchhouse.db backup --output /private/backups/unique-directory --operator operator --reason 'Before a planned upgrade'
watchhouse backup-inspect --directory /private/backups/unique-directory --manifest-digest sha256:RETAINED_MANIFEST_HASH
```

The output directory must not exist. Its parent must already exist and be protected by the operator's OS permissions. Unix creates the root as 0700 and copied data/manifest as 0600; Windows inherits the parent's ACL. This is a private forensic export containing the database's existing operational records and configuration references. There is no HTTP endpoint for selecting filesystem paths or downloading these bytes.

`backup` uses SQLite `VACUUM INTO` to include one committed read snapshot, including live WAL contents. It then copies only the declared frozen source and candidate files from that snapshot, verifies their exact bytes, and writes `manifest.json` last. Ordinary database metadata, receipts, deadlines, events, evidence and policy/configuration revisions remain in the database. Frozen Git identities and executable flags remain in the manifests; copied backup files are inert data. Neighboring work/control files, credential copies/auth homes, private process transcripts, executable runtimes and container images are not copied. No original source checkout is needed to inspect the export.

The completed report returns backup identity, manifest digest, creation time, entry/artifact counts and bytes. A successful creation appends a metadata-only `backup_created` factory audit entry, visible through `factory-status`, authenticated `GET /api/v1/factory` and the web factory details. Creation does not change case versions or accepted command receipts. If audit insertion fails after the files were verified, the command fails but the completed export remains inspectable; do not overwrite or blindly recreate it.

`backup-inspect` does not open, create or migrate the coordinator database selected by `--db`. It reads the completed export and checks the exact inventory, safe portable paths, absence of links/special files, hashes/sizes, SQLite integrity/foreign keys, supported contiguous migration history, evidence hashes, and correspondence between the database and complete frozen-artifact manifests. Missing, extra, modified, unsupported or incomplete content fails closed. A separately retained `--manifest-digest` pins the manifest; unpinned hashes detect corruption but do not authenticate a malicious backup producer.

Fixed v1 bounds: 16 GiB database, 2 GiB copied artifacts, 8 MiB per artifact file, 16 MiB manifest, 10,000 artifact roots and 100,000 directory entries. Exceeding a bound fails; no content is silently omitted. Interrupted/failed directories remain incomplete and are never overwritten or removed automatically. An incomplete marker blocks startup while creating the export. The manifest is a completion marker, not an atomic directory transaction or a universal power-loss durability guarantee. Inspection must pass before any future restore.

[Isolated restore v1](restore.md) now reconstructs verified snapshots without inheriting external authority. `restore_available:true` indicates supported restore tooling, not live activation. It clears native admission, fences old revisions/handles, preserves uncertainty/deadlines and requires explicit reconciliation/fresh OS authority. No backup/restore fixture establishes live integration proof.
