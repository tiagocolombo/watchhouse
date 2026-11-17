# ADR 0030: Separate eligible merge authority

Implement a durable merge controller downstream of exact native publication and current candidate/regression/independent review. OS policy separately scopes repository/base, merger credential, paths, checks/apps, approvals and budgets. Ordinary tests inject remote observations and cannot establish live authorization. Sensitive changes and unsupported strategies require explicit human handling; no bypass or exception API exists.

Use GitHub's synchronous exact-head merge endpoint with merge commits only. The endpoint has no base compare-and-swap: require server-enforced strict up-to-date branch protection, stale-review dismissal, admin enforcement, conversation resolution and no PR-review bypass actors. The published head has exactly the frozen base parent. Native preflight repeats immediately before effect, and returned merge parents/tree must match exactly. A configured operator must preserve these server protections; missing/incomplete/unknown facts deny admission. Do not support async stack merge or implicit downstack effects.

Reserve before effects, retain uncertain outcomes, reconcile by GET only, and preserve original deadlines. Definitive no-effect blocks may be retried at most three times; uncertainty cannot replay. Continue owned supervision without models and preserve separate deployment/recovery state.

Primary API contracts consulted 2026-11-17:
- https://docs.github.com/en/rest/pulls/pulls#merge-a-pull-request
- https://docs.github.com/en/rest/branches/branch-protection#get-branch-protection

This does not authorize any development PR merge. A dedicated protected test repository, credentials and independent review/check configuration are missing; live acceptance stays open.
