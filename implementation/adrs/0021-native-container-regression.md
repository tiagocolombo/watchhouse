# ADR 0021: Credential-free container checks and exact candidate regression

Accepted as an implementation proposal 2026-11-16, continuing M4 above the completion-gates stack. The owner requires completion of remaining milestone development before local acceptance testing. This slice closes a test-execution gap; M4's full real-harness-to-PR exit stays open.

Use a fixed native Linux Docker boundary configured only through immutable OS-operator policy. Pin client, installed image and initializer identities, inspect exact namespaces/mounts/capabilities/limits before start/removal and measure the actual boundary. Keep the original failed Bubblewrap result as failed evidence; a different boundary needs its own proof.

Separate trusted initializer UID 0 (only SETUID/SETGID capabilities) from UID 10001 test processes without capabilities. A same-UID trusted initializer would expose its output descriptor to untrusted descendants and allow forged check results. The immutable image and host daemon remain trusted; neither the coordinator database nor host credentials/socket are mounted. Qualification proves a progressing descendant then positively acknowledged full-container removal. Unknown removals never become success through absence inference.

Persist effect reservations before I/O; reconcile exact existing handles after response loss. Deadlines, cleanup attempts, quarantine and owned escalation work without a model. Bind native admission to current policy/generation, actual engine/kernel/boot, retained evidence and expiry. The compiled bootstrap case policy remains synthetic-only.

Freeze operator-provided text candidates independently of the base and retain shared attempt/time budgets across revisions. Require the same fixed regression to exit 1 on base and 0 on candidate, complete manifest-bound evidence, and a distinct reviewer's own fresh evidence. Display current versus invalidated review separately from immutable historical records. This does not authenticate distinct remote people or model sessions.

No model or publication credential enters this boundary. Real harness authentication/admission and reconciled GitHub delivery are subsequent dependent work. The coordinator's native packaging and a model's claim that checks passed cannot substitute for the worker proof.

See the [implemented contract](../../contracts/v1/native-repair/README.md) and STATUS for commands actually run and the native CI result. Ordinary injected controller tests and the separate physical no-model integration drill have different evidentiary scope.
