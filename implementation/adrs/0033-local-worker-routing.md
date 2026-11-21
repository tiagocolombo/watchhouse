# ADR 0033: bind local execution to immutable OS worker routes

Implementation decision, 2026-11-20. R04/R06 require capability routing without treating harness names or host metadata as physical proof.

Migration 029 adds immutable routing revisions and first-dispatch worker bindings. Existing per-profile policy and native qualification remain authoritative. A route selects a kind and exact installed policy revision on a matching platform/architecture; only the existing Linux worker boundaries currently support dispatch. Removing/pausing a route prevents new effects without moving an uncertain resource to another worker. Cleanup uses the retained original binding independently of current routing integrity and preserves all existing cancellation checks/budgets.

Routing is local OS configuration, not a worker authentication protocol. The unnamed legacy path works until a route policy is installed; existing legacy cleanup remains available. Shared SQLite over a network, remote registration and native macOS/Windows candidate/model boundaries are not provided by this slice. They remain development gates; denied capability does not complete those adapters.

The native Linux model and container CI fixtures now dispatch through named routes. Controller fixtures cover pause/removal/restart, unclaimed work expiration, original-owner cleanup, corrupted policy, transactional binding rollback, platform rejection and queue-prefix isolation. Physical final-head CI is recorded separately from injected controller observations and live service/model activation.
