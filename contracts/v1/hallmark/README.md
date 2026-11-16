# Hallmark knowledge exchange v1

The adapter reads Hallmark's JSON export boundary, preserves upstream bytes/namespace/path identity and stages imports as untrusted records. It does not execute Hallmark, import approval authority, or couple the coordinator to Hallmark's internal repository/store. This boundary was checked against Hallmark `15ee84d9de5385658b8d9dff1408728fb9e28fbd`, specifically `src/library/portable.rs`, `learning.rs`, `commands/library.rs` and `output.rs`.

Supported input shapes match that source: a `{format:"hallmark-library", version:1, learnings:[{path,content}]}` file, the flattened `hall library export` JSON reply with a `learnings` list, or a bare entry array. Explicit incompatible format/version/action values fail. Markdown frontmatter is retained as opaque data rather than loaded as instructions or silently interpreted as native acceptance. Hallmark itself remains responsible for validating learning-file semantics when reading a re-export.

## Import and inspect

`case import-hallmark CASE --file COMMAND.json` and authenticated POST `/api/v1/cases/CASE/knowledge-imports` share:

```json
{
  "command": {
    "meta": {"schema_version":1,"idempotency_key":"import-001","actor":"operator","reason":"Review scoped external knowledge"},
    "expected_version":42
  },
  "source_namespace":"hallmark:github.com/example/project",
  "payload": {
    "format":"hallmark-library",
    "version":1,
    "learnings":[{"path":"L-0007-example.md","content":"---\nid: L-0007\nkind: rule\nstatus: accepted\n---\n# External example\n\nUntrusted source text.\n"}]
  }
}
```

Replace the version with the owned case's actual version. Import is human-control-plane attributed and transactional with its timeline event and command receipt. Same key/content replays the result. Reimporting identical namespace/path/bytes returns the existing record. Changed bytes append a distinct immutable import; they cannot overwrite an accepted native lesson. Namespaces keep equal upstream IDs from different repositories distinct.

Batches contain 1–20 distinct portable relative `.md` paths, at most 16KiB per entry and 24KiB serialized payload. Split larger exports. Reject traversal, absolute paths, case collisions and Windows reserved names. No path is extracted to the filesystem. Full source paths/frontmatter/unknown keys/content are preserved; normalization is used only for collision detection. Source namespaces are explicit and never inferred as permission.

The snapshot's `knowledge_imports` list and web panel show the newest 100 imported records as **Imported / unreviewed**, including when their raw content says `status: accepted`. Direct export/provenance lookup uses explicit IDs without that display cap. Raw Markdown uses inert text rendering. The source digest and complete provenance-record digest are validated on read.

## Map a proposal and independently review

Use the existing `case learning` / POST `learning` `propose_knowledge` command, with an explicit same-case postmortem revision, evidence, applicability, invalidation conditions and expiry. Set `draft.imported_from` to the returned `hallmark-import:IMPORT_ID` provenance reference. The core verifies this import exists in the same case and has intact bytes/metadata. Unknown, foreign or corrupt references cannot become accepted knowledge.

The operator deliberately maps the external content into a scoped native draft. This is not an automatic frontmatter-to-authority conversion. Normal distinct-reviewer/fresh-evidence, postmortem, context, challenge, retirement and expiry gates still apply. Upstream acceptance alone never enters a briefing. A later import/draft does not replace accepted knowledge governing its own review. Corrupt provenance removes the dependent lesson from current retrieval/export.

## Export

`case export-hallmark CASE --file SELECTION.json` and authenticated POST `/api/v1/cases/CASE/hallmark-export` accept either:

```json
{"type":"imported","ids":["IMPORT_ID"]}
```

```json
{"type":"knowledge","ids":["KNOWLEDGE_ID"]}
```

Choose 1–20 distinct explicit IDs. Output is the canonical `hallmark-library` version-1 JSON file on stdout/API response; no external write is performed.

Imported records roundtrip the original source paths/content exactly. Export one source namespace and one revision per path; collisions fail explicitly instead of silently renumbering or losing data. Re-exported upstream status remains upstream data.

Native export requires current independently accepted knowledge, using its accepted revision even while a replacement draft awaits review. It emits canonical Hallmark frontmatter and **status: proposed**, preserving Watchhouse document/revision/digest, source case, service/environment/repository, original kind, expiry, import origin, body and applicability/invalidation conditions. Hallmark IDs are local to that export bundle; the explicit Watchhouse source identity is stable. Hallmark's importer may renumber collisions according to its own rules. External review is required before use in that repository. Native exports do not claim full history or transferable execution/merge authority.

## Verification scope

Sanitized invented fixtures use the actual observed export structure. Tests prove all three wire shapes, byte roundtrip, revisions/collisions/deduplication, immutable records, atomic rollback/restart, case/auth boundaries, independent local promotion, accepted-versus-pending revision export, corrupt-provenance rejection and script-inert browser replay. No Hallmark executable, real historical incident or external import destination was exercised. See STATUS for observed results.
