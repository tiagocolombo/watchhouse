# ADR 0024: Adopt immutable model data with independent session lineage

Status: implemented, automated tests passed; live model acceptance remains open.

## Context

A text-role result is neither a frozen candidate nor independent review authority. Operator adoption must not relabel model work as human authorship, alter its changes/findings or carry stale approval into another base. The coordinator must validate evidence rather than rely on a model's statement that it ran tests.

## Decision

Extend existing candidate/review commands with an optional immutable model-job reference. Add explicit adoption commands that derive their material from retained native role output. Preserve legacy fingerprints when the optional reference is absent. Candidate publication gates continue to use the existing native fail-on-base/pass-on-candidate test evidence.

Accept only completed non-synthetic native roles after positive whole-resource cleanup, with exact profile, source, repair, accepted-knowledge and case-linked evidence binding. Short execution deadlines revoke further client effects; completed data remains usable only under current admission/context and the existing shared repair deadline. Native transport proof is not a live-account claim.

Record model authors/reviewers by their actual harness session plus job ID. Reviewer jobs get a separate briefing without author private reasoning and must bind the exact candidate and both independently observed native checks. A reused author session, stale candidate, modified decision/findings or wrong evidence/check IDs blocks adoption. The coordinator retains a reviewer's actual approval or rejection; human labels cannot impersonate it. Human reviews still require independently collected human evidence.

Re-evaluate model lineage when evaluating candidate/review currency. Corrupt author evidence invalidates the candidate and dependent review; later context/policy/challenge changes cannot retain the old approval. Derivation and adoption share the existing atomic state/event/receipt services. Local single-operator identity labels remain attribution; remote RBAC is a separate M8 gate.

## Validation boundary

Controller fixtures inject native-shaped observations to test lineage, evidence corruption, session independence, exact proposal adoption and idempotency without running a model/account/kernel. Physical Codex/cgroup acceptance remains the distinct ADR 0023 CI drill. No live account/model, publication, merge or production effect is inferred.
