# ADR 0034: keep Hallmark optional and use bounded knowledge exchange

Implementation decision, 2026-11-20. D01/R13 specify a separate product and an optional Hallmark repair integration; M7 requires recording the spike result.

The [pinned source and actual CLI spike](../spikes/hallmark-repair-backend.md) built Hallmark 0.3.18 at 15ee84d, ran version/help/three schema probes in an empty credential-free environment, and inspected workflow, authority, cancellation and privacy boundaries. Machine-readable inspection works. Its full repair daemon would introduce independently owned worktree/review/shipping authority and does not expose Watchhouse's currently required sealed whole-resource lifecycle contract.

Select the existing bounded knowledge-format exchange; do not install or call Hallmark's repair loop. This resolves the deliberately optional spike without pretending an unsafe/denied wrapper implements a required worker. A later optional repair adapter needs its own qualified outer boundary, fixed protocol, effect isolation and independent acceptance. Native macOS/Windows workers and M8 remain required development; no live model, repair or external effect was verified by the read-only probe.
