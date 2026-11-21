# ADR 0042: Schedule explicit body expiry without inheriting deletion authority

Status: accepted implementation choice, 2026-11-21. Extends ADR 0041.

Use a separate immutable OS policy with six explicit evidence-store classes and nullable ages. Keep classes indefinitely unless configured otherwise; preserve the distinction between native and fixture proof. Maintain an indexed queue of nonremoved evidence so expiry filters before selection limits and does not rescan tombstone history. Persist one-body cadence and defer failed items without starving other eligible bodies.

Run the existing removal/invalidation transaction under the same current policy, age and clock reservation. No external call occurs. Retention is a deterministic core duty; closing the browser or withholding a model/worker cannot stop it. Unavailable retention cannot stop independent timer/escalation/fake duties. Surface policy/schedule failure, deferred integrity review, unknown age and responsible OS owner without exposing bodies. A canonical recorded time is required; do not invent historical age. A copied policy never grants restore deletion authority; a fresh OS revision is required.

Automatic expiry cannot guess the identity of corrupted evidence. Add a metadata-only OS inspection and explicit current-body-digest pin for targeted corruption removal. Preserve original identity plus inspected digest in its tombstone; wrong pins reject and no old private body must be recreated. Successful removal clears deferred state. Remote/harness roles cannot provision policy or request erasure.

This is still body retention. Formal/derived/receipt/journal/filesystem/export/backup copies, complete incident bundles and full M8 operating work remain separate development. The interval bounds chosen bodies, not arbitrary dependency processing time or whole-database growth. See [policy contract](../../contracts/v1/retention/policy.md) and COMPLETION.md.
