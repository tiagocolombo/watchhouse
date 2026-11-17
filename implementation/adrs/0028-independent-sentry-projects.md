# ADR 0028: Independent Sentry project profiles and fair reconciliation

Status: implemented with remote CI contract verification; live activation remains open.

The single-project source adapter was a real dependency for M3 cross-service use. Preserve its immutable default identity and existing source/case/receipt keys while adding independently versioned project heads, explicit signed routes, credential references and retained input provenance. Global configuration revisions remain monotonic; historical replay cannot reactivate a profile. No model/HTTP configuration mutation is introduced.

Require explicit versioned read budgets for newly registered projects. Retain frozen v1 compatibility for already supported default profiles and label that provenance. Validate request/window/overlap/page limits, forbid cross-organization project-ID aliases, and keep the fixed provider origin with redirect/proxy denial. Do not infer live credentials or permissions from configuration presence.

A persisted round-robin cursor dispatches one due project discovery page at a time. Known-issue and delivery enrichment select that exact project's active profile. Resume bounded windows from coverage watermarks, retaining overlap and incomplete-page state through restart. A long outage cannot silently advance coverage to now; a capped window remains explicitly incomplete.

Read generations plus active project-version fences reject late results and obsolete discovery pages. Configuration changes are visible in freshness and invalidate model/candidate authority, accepted hypothesis currency and reported-impact context. Collection supervision stays deterministic and model-independent. No polling result grants incident recovery or production authority.

The execution workspace became unavailable during development. The unfinished source was reconstructed from reviewed edits and preserved on GitHub. Pinned formatter output was applied from CI without bypassing format/lint/test gates. Linux/macOS/Windows CI passed the 201-test Rust suite, native builds and coordinator packaging at the intermediate source head; Linux also passed native Docker/model-client fixture drills and ten browser scenarios. Final documentation/stale-context follow-up still requires exact-head CI. No local validation is claimed for the hosted continuation.

The source transport domain gates now have multi-project fixtures, including actual synthetic HMAC selection and no cross-project fallback. Authorized real accounts, ingress, destinations and actual sanitized service history are still missing live-acceptance inputs. Continue M5–M8 independently; do not hand off local acceptance or close the live gate.
