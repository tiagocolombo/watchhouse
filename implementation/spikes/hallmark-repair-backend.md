# Optional Hallmark repair-backend spike

Observed 2026-11-20; upstream [15ee84d9de5385658b8d9dff1408728fb9e28fbd](https://github.com/tiagocolombo/hallmark/tree/15ee84d9de5385658b8d9dff1408728fb9e28fbd), package 0.3.18. This is an actual CLI/source compatibility investigation. It is not a repair execution, isolation qualification, live model or account drill.

## Executed observation

Cloned the pinned public source into a separate temporary checkout. `cargo build --locked --bin hall` passed with Rust 1.90.0. In a new empty directory and home, with only a fixed system PATH, HOME/XDG directories and NO_COLOR in the child environment, invoked:

```sh
hall --version
hall help --json
hall schema error
hall schema event
hall schema config
```

Every command exited 0 with empty stderr. Version was `hall 0.3.18 (15ee84d)`. Help and all three schemas parsed as JSON; help advertised command/contract/error/exit-code metadata. No file was created in the empty home/cwd. [Observed metadata](hallmark-cli-observations.json) retains sizes and top-level keys. No model, daemon, setup, worktree, notification, GitHub or repair command was invoked. No credential was supplied or copied. Reproduction should rebuild the exact commit and repeat these read-only commands; binary digests are build-specific and do not establish native admission.

## Compatibility findings

| Concern | Pinned upstream evidence | Consequence for Watchhouse |
| --- | --- | --- |
| Machine-readable contract | `src/cli.rs`, `src/commands/meta.rs`, `docs/commands.md`; actual help/schema calls | Read-only inspection and future explicit data translation are feasible. JSON alone is not an execution protocol. |
| Workflow ownership | `src/config.rs` worktree/gates/fixer/ship/update/review settings; daemon and item commands | The full loop owns worktrees, review, budgets and shipping. It cannot replace Watchhouse's independent policy/authority/receipt store. |
| Runtime authority | `src/runner.rs` RunRequest, confinement, command environment; `src/config.rs` sandbox | Claude permissions/sandbox and caller-selected environment are not a Watchhouse-verified outer resource boundary. Native Windows auto sandbox omits the Claude sandbox. This is a compatibility limit, not a blanket claim about upstream safety. |
| Cancellation/recovery | `src/runner.rs` ClaudeRunner::exec kills/waits the immediate child after timeout; `src/assayer/checks.rs` gate process-group/taskkill handling; daemon lifecycle | The inspected interfaces do not provide Watchhouse's sealed nonce/profile/boot/resource identity, independent hard-wall watcher and whole-resource reconciliation proof. Gate process-tree handling is distinct from model-run containment. |
| Evidence/privacy | Whole-record export/trace includes model-run steps; item journal can contain prompts | Do not import raw private transcripts or turn a Hallmark approval into Watchhouse acceptance. Existing bounded knowledge exchange remains the selected integration. |
| External effects | Configured ship/update and merger services | Running the complete daemon would require separate destination authority and reconciliation. No such effect was authorized or attempted by this spike. |

## Decision

Do not select Hallmark's full repair loop as an execution backend in the current implementation. Keep the implemented bounded knowledge-format adapter (ADR 0022), and keep the direct qualified text clients plus separate credential-free candidate executor (ADRs 0021/0023/0032). Hallmark remains optional; no installation is required by Watchhouse.

A future repair adapter would need an independently qualified outer worker, a fixed versioned request/result/cancel/reconcile boundary, explicit authority disabling internal publication/merge, and translated immutable candidate evidence without private traces. This is a future optional proposal, not a required missing adapter and not a permission bypass. R13 is preserved; this spike closes the roadmap's optional investigation only. Native macOS/Windows workers, remote trust and M8 remain required development.
