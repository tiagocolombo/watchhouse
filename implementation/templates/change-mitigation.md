# Change or mitigation plan

Artifact metadata: ID/version, incident/problem/work item, author, verifier/reviewer, timestamps, sensitivity, snapshot.

## Purpose and scope

Symptom/cause addressed; accepted hypothesis and evidence; intended behavior; affected repository/service/environment; excluded scope; operational action versus permanent repair.

## Proposed action

Exact candidate or operational steps; prerequisites; required capabilities and authority; target boundaries; expected benefit; risk and blast radius; dependencies.

For code: repository, base/head/tree/diff identity and changed paths.
For operations: runbook version, parameters, execution target and effect identity.

## Validation

Reproduction/regression result on base; result on candidate; test/check runner and environment; outputs/digests; independent review; blocking findings/challenges; unverified limitations.

If normal regression validation is impossible, document the alternative and required exception authority.

## Execution and rollout

Admission/merge decision and policy version; PR/CI references; delivery/deployment stages; monitoring; observation window; stop conditions; authorized executor.

## Rollback or recovery

Trigger conditions; exact procedure; prerequisites; risk/data compatibility; owner; verification method. If reversal is unavailable, state why and the authorized recovery alternative.

## Actual outcome

What ran/merged/deployed, when, by whom, against which target; observed external result; discrepancies; reconciliation after uncertainty; recovery evidence; remaining actions.

A successful command, merged PR, or agent assertion alone does not prove customer recovery.
