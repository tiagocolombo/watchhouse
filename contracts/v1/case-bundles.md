# Internal case bundles (v1)

An internal bundle is a consistent retained record view, not a coordinator restore, public communication, factual endorsement, native execution proof or recovery verdict.

## Implemented interfaces

- `GET /api/v1/cases/{id}/bundle?include_evidence=false&include_patches=false`: authenticated local/global human or service-owner case grant. Observers may download. Two responses per coordinator may remain active at once, with permits retained through body completion/drop; an 8 MiB serialized frame bound rejects oversized cases without truncation. Unknown query fields reject. Identity is derived from the authenticated session. Responses are attachment JSON and no-store.
- `GET /api/v1/cases/{id}/exports`: newest 100 export metadata records within the authorized case. Prepared download means response construction, not verified client delivery.
- `watchhouse --db STATE case-export --case CASE --output NEW_DIRECTORY --operator OPERATOR --reason REASON [--include-evidence] [--include-patches]`: OS plane, new private directory only. Streams the declared retained SQL ownership closure in one SQLite read transaction, including every retained document revision and linked command receipt. No workspace snapshot caps apply.
- `watchhouse case-export-inspect --directory DIRECTORY [--manifest-digest SHA256]`: read-only and does not open coordinator state.
- `watchhouse --db STATE case-export-reconcile --id ID --directory DIRECTORY --manifest-digest SHA256 --operator OPERATOR --reason REASON`: original registered destination and identity only; bounded validated incomplete marker, complete declared file inventory and pinned manifest required. Restored registrations are permanently forensic. Re-export in a new directory after restore.

## Content and integrity

A header records case/version, case event cursor, evidence invalidation epoch, source freshness/current context, recovery currency, actor attribution, options and exclusions. All reads use one database snapshot. Header clocks do not imply source observation time.

Named sections include case state/events, assignments/runs/control views, team messages/acknowledgements/handoffs/blocks/escalation routing and deliveries, all incident artifact/approval revisions, dispositions/obligations, impact, repair/candidate/review/execution/publication/merge/operation records, recovery/actions, knowledge revisions/imports/acceptances/retirements, source observation/read state, case factory control records and receipt lineage. Service-owner correlations require grants for both endpoints. No automatic neighbors/global topology are included.

SQL text fields that are structured JSON are decoded. Known control/runtime/credential fields are removed recursively and their paths listed; code `changes` is omitted unless requested. An original canonical SQL-column-map digest and a distinct exported-view digest make this transformation explicit. Export is refused for malformed structured columns. These are internal records: free-text prose and already incorporated references remain visible. This is not semantic de-identification or a credential scanner.

The evidence manifest distinguishes present, redacted, missing and corrupt bytes. Removed bodies never reappear. Optional retained bytes carry original and observed digests; corrupt bytes cannot be mislabeled present. Excluding raw evidence does not remove derived prose from records. Global configuration/authentication/provider queues/cursors and private runtime homes, frozen filesystem snapshots, working directories and raw harness transcripts are excluded. The exporter never reads those paths.

Browser frames hash canonical header/counts/sections. Directory manifests bind ordered, size/row/hash-pinned JSONL parts and counts. Rows are bounded to 8 MiB, parts to 64 MiB, directory data to 16 GiB/2,048 parts/ten million records and manifest to 1 MiB. Limit failures are explicit; no silent truncation. Inspectors reject tampered counts/bytes/views, foreign case rows, duplicate/nonportable/symlink paths and undeclared files. A separately pinned manifest verifies integrity against that supplied identity; hashes alone are not producer authenticity.

A private incomplete marker and owned metadata survive interrupted export. A manifest is written last. Successful marker removal precedes complete database metadata; an interruption can require pinned reconciliation. No overwrite or automatic filesystem deletion occurs. Directory permissions are 0700 and file permissions 0600 on Unix; Windows inherits a private parent ACL supplied by the OS operator.

Current-store redaction cannot retract previously downloaded/exported copies. Managed export/backup expiry and wider derived/formal/receipt/journal retention remain separate unfinished development.
