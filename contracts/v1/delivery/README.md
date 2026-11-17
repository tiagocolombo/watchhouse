# Reviewed delivery preparation v1

`watchhouse case publication-plan CASE --candidate CANDIDATE --review REVIEW` and authenticated `GET /api/v1/cases/{id}/publication-plan?candidate=...&review=...` prepare the same read-only artifact. Current review cards expose an inert inspection button.

The application rechecks the current exact candidate, native regression evidence, independent review, source/profile/knowledge/challenge context and manifest digest. It verifies all candidate bytes and canonical paths, including parent directories; bounds file count, individual and total sizes; rejects symlinks, collisions, payload/manifest disagreement and deleted files remaining in the manifest. Git tree/blob identities use Git's object format and directory sort order, independently checked against actual Git in fixtures. Binary unchanged files remain part of the expected tree. No candidate code, hooks or repository configuration executes during preparation.

The artifact records the candidate/review/base identity and exact expected Git tree SHA, together with a GitHub Git Data tree request over the frozen base. A publisher must reject a returned tree with another SHA. SHA-256 Git repositories are currently unsupported and rejected; this adapter targets GitHub's SHA-1 Git Data contract.

`external_writes_authorized` is always false. This interface has no publisher credential, network effect, branch/commit/PR creation, merge or deployment authority. The durable separately authorized publisher and uncertain-effect reconciliation are the next required M4 work. A preparation artifact or inspected review does not complete the repair-to-PR exit scenario.
