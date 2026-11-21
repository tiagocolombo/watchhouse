# ADR 0035: durable scoped admission holds and explicit stop fences

Implementation decision, 2026-11-20. M8 needs operating controls that do not disable the independent supervisor or turn an uncertain effect into a claimed failure.

Migration 030 separates global factory control version/audit from case aggregate versions. A live scope projection matches global/service/repository/incident/case holds and explicit effect classes. Ordinary pause filters new reservations; already-running observation and cleanup continue. Stop additionally expires fake authority, schedules native cleanup through existing original-worker bindings, and replaces uncertain mutation reservation fences. Resume removes only the hold; resource/receipt/budget/native/review facts remain governing.

Final-read races recheck factory admission before publication/merge writes. Reservations that precede a pause may already have effects; stop invalidates their result fence and requires scoped read-only reconciliation. No arbitrary production stop, rollback, credential revocation or process kill is inferred from a UI control.

The model-independent supervisor retains its latest tick and a monotonic observed-clock high watermark. Backward clock prevents new effects but never resets the watermark or disables cleanup. Health describes factory operation and visible uncertainty, not application service recovery.

CLI/API/web share versioned command receipts and status. Tests cover pause isolation, future intake, stop/late-result fencing, rollback/restart, pending duties/model-offline escalation, stale versions, backward clock and publication/merge/gateway effect races. Linux native model CI additionally stops an actually populated synthetic cgroup through the factory control; final-head proof is tracked separately. This does not close the remaining M8 capacity/backup/privacy/RBAC/replay gates or M7 native macOS/Windows workers.
