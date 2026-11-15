# Trusted policy v1

`local-fake-v1.json` is compiled as the initial side-effect-free policy; it is not dynamically read from a candidate workspace. Copy it outside candidate-controlled directories, change the name and advance revision by one, then deliberately install through the trusted OS operator CLI:

```sh
watchhouse --db /trusted/watchhouse.db policy --file /trusted/policy-v2.json --operator operator
```

No worker/API policy editing exists. This command requires operator access to the canonical database. Fake adapters cannot access files or invoke it. An untrusted real harness must never receive that OS/database access. No local attribution string provides protection against a hostile process with operator access.

Policy records are immutable, bounded, strict JSON. Lease <= progress <= assignment; response <= assignment; attempts 1..100; timers remain within hard protocol safety bounds. A policy cannot grant production, publishing, merging or communication powers. Cases pin policy at creation, including queued assignments. Changes apply only to new cases. Historical and migrated deadlines are retained. Snapshot `policy` and event `policy_version` show provenance. Installation source/identity/time are retained in `policy_versions`.
