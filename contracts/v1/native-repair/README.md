# Native container checks and frozen candidates v1

This slice implements credential-free native **test execution**, frozen operator-provided text candidates, exact base/candidate regression records and independent attributed review. It does not implement a model adapter, GitHub publication, automatic merge, deployment or recovery. The old blocked repair-preparation record remains immutable; inspect `repair_progress` and `container_jobs` for subsequent facts.

## Trusted configuration and native admission

Install the case's governing policy through `policy --file FILE --operator OPERATOR` before creating its case. The compiled bootstrap profile authorizes synthetic work only. Install an immutable sequential container policy using `container-policy --file FILE --operator OPERATOR`; no HTTP policy-install or arbitrary worker-launch route exists.

A container policy includes schema_version (1), revision, canonical docker_path/docker_digest, Unix socket, installed image SHA256 ID, runner_digest, canonical staging_root, fixed image-resident program and argument array, memory_bytes, pids, cpu_micros per 100000-microsecond period, work_bytes, wall_seconds, output_bytes, proof_seconds, dispatch_grace_seconds, cleanup_attempts and cleanup_retry_seconds. The operator must provision trusted client/image/initializer identities; sample flags and an image name are insufficient. The local Docker daemon/OS administrator and operator-installed image are trusted control-plane components.

`case qualify-container CASE --file FILE` queues a controlled probe. `container-worker --once` advances at most one dispatch or cleanup transition; omit `--once` for a persistent native worker. The server's built-in assignment worker remains synthetic. The native worker invokes no model and has no candidate credentials. Normal supervision continues independently of browser/model availability.

Qualification measures the pinned initializer, read-only root/input/policy mounts, private network/PID namespaces, actual cgroup limits, seccomp/no-new-privileges, bounded writable scratch, credential-free environment and child UID/capabilities. A trusted parent with only SETUID/SETGID capabilities owns the output protocol; untrusted tests run as UID/GID 10001 without capabilities. The child's attempt to open the parent's protocol descriptor must fail. A progressing descendant is kept alive until exact positively inspected container removal is acknowledged. The host network canary is held throughout the probe. Recovered logs alone cannot recreate host/removal proof.

Native admission binds the installed policy, engine/kernel/host-boot identities, qualification generation, content-addressed evidence and expiry. A newer policy/qualification or missing/corrupt/expired proof invalidates new execution. CPU quota limits aggregate rate; this is not a hard monetary/token budget. No authentication or subscription capability is inferred.

## Versioned commands

Every command contains `command: {meta: {schema_version: 1, idempotency_key, actor, reason}, expected_version}`. Obtain the case's current version first. Reuse the identical command/key after a lost response; changing its content requires a new key and fresh version. Actor labels are local operator attribution, not remote RBAC.

| CLI after `case` | HTTP POST after `/api/v1/cases/CASE/` | Additional command fields |
| --- | --- | --- |
| `qualify-container CASE --file FILE` | `container-qualification` | `policy_revision` |
| `freeze-candidate CASE --file FILE` | `candidates` | `repair_id`, `policy_revision`, `changes` |
| `check-candidate CASE --file FILE` | `candidate-checks` | `candidate_id`, `side` (`base` or `candidate`) |
| `review-candidate CASE --file FILE` | `candidate-reviews` | `candidate_id`, `base_check`, `candidate_check`, `approve`, `findings`, `evidence_refs` |
| `manage-container CASE --file FILE` | `container-manage` | `job_id`, `action` (`reconcile`, `cancel`, `report_cleanup`), `evidence_refs` |

Read a specific job using `case container-job CASE --job JOB` or authenticated GET `container-jobs/JOB`. The case snapshot, timeline, web panel and `case repair-markdown` expose immutable candidates, current/stale reviews, native versus simulated observations, deadlines, evidence and quarantined resources.

A text change contains `path`, `before_digest` (null for a new file), and `after` (null for deletion). Paths are portable relative regular-file paths; no `.git`, traversal, symlink, case collision or Windows reserved name is accepted. Existing executability is retained; candidate changes cannot change their own execution policy. Source objects are copied and hashed without Git metadata, filters, hooks or untracked files.

Checks use the same fixed trusted-image test command and profile against separately frozen base/candidate manifests. The base must produce ordinary exit 1; the candidate must produce ordinary exit 0. A timeout/OOM/invalid output is not regression failure evidence. Each output is bound to operation, nonce, profile and complete manifest. Native terminal output is accepted only after positively observed exit and acknowledged scoped removal. Shared per-repair attempt/time budgets survive candidate revisions; checks cannot outlive that budget.

A distinct reviewer needs their own fresh evidence collected after candidate submission. Review binds the exact candidate/base/profile/source and both native check records. Later candidate/source/policy changes, expiry, blocking challenges, evidence corruption and later changes-required review invalidate approval. This is attributed operator review; it is not yet an independent model session or externally authenticated reviewer.

## External effect recovery

The database reserves create/start/cancel before I/O. Unknown acknowledgments retain the exact reserved name/handle and quarantine; dispatch never blindly repeats create/start. Reconciliation inspects the existing boundary before accepting created/terminal state. Cancellation removes only the positively inspected owned full container ID. Lookup failure or absence is not a removal acknowledgment.

Offline deadlines schedule bounded cleanup attempts. Exhaustion retains quarantine and owned escalation. `report_cleanup` requires the operator's new scoped inspection evidence; it clears the reported resource hold without establishing native execution, accepting a result or resolving the escalation. Late/cancelled output is retained as unaccepted evidence and cannot restore authority. A repeated command does not repeat external I/O.

Ordinary tests inject controller observations and do not establish physical proof. `bash tools/native_container_drill.sh` explicitly builds a local credential-free image and runs native qualification, descendant cancellation, failing base/passing candidate and attributed review. Linux CI runs that drill separately from ordinary Rust/browser checks. It invokes no models or external repository actions. Inspect the actual CI result before claiming native verification.
