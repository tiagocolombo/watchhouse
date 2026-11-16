//! Explicit adoption binds immutable role output to the existing candidate and native-check gates.
use super::codex::RoleOutput;
use crate::{
    application::Application,
    contracts::OperatorCommand,
    error::{Error, Result},
    real_execution::pipeline::{Candidate, FreezeCandidate, Review, ReviewCandidate},
};
use serde::{Deserialize, Serialize};
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AdoptProposal {
    pub command: OperatorCommand,
    pub job_id: String,
    pub container_revision: i64,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AdoptReview {
    pub command: OperatorCommand,
    pub job_id: String,
}
impl Application {
    pub fn adopt_model_proposal(&self, case: &str, cmd: AdoptProposal) -> Result<Candidate> {
        // The target command owns idempotency. Its exact material is derived from the retained job,
        // so retries cannot accidentally adopt different model text using the same receipt key.
        let j = self.model_job(case, &cmd.job_id)?;
        let parsed = j
            .output
            .as_ref()
            .and_then(|o| o.client.parsed.as_ref())
            .ok_or_else(|| Error::Conflict("model proposal unavailable".into()))?;
        let RoleOutput::Engineer(proposal) = &parsed.output else {
            return Err(Error::Invalid(
                "role did not produce an engineer proposal".into(),
            ));
        };
        self.freeze_candidate(
            case,
            FreezeCandidate {
                command: cmd.command,
                repair_id: j
                    .plan
                    .context
                    .repair_id
                    .clone()
                    .ok_or_else(|| Error::Conflict("repair context missing".into()))?,
                policy_revision: cmd.container_revision,
                changes: proposal.changes.clone(),
                model_job: Some(j.id),
            },
        )
    }
    pub fn adopt_model_review(&self, case: &str, cmd: AdoptReview) -> Result<Review> {
        let j = self.model_job(case, &cmd.job_id)?;
        let parsed = j
            .output
            .as_ref()
            .and_then(|o| o.client.parsed.as_ref())
            .ok_or_else(|| Error::Conflict("model review unavailable".into()))?;
        let RoleOutput::Reviewer(assessment) = &parsed.output else {
            return Err(Error::Invalid(
                "role did not produce an independent assessment".into(),
            ));
        };
        let context = &j.plan.context;
        self.review_candidate(
            case,
            ReviewCandidate {
                command: cmd.command,
                candidate_id: context
                    .candidate_id
                    .clone()
                    .ok_or_else(|| Error::Conflict("candidate context missing".into()))?,
                base_check: context
                    .base_check
                    .clone()
                    .ok_or_else(|| Error::Conflict("base check context missing".into()))?,
                candidate_check: context
                    .candidate_check
                    .clone()
                    .ok_or_else(|| Error::Conflict("candidate check context missing".into()))?,
                approve: assessment.approve,
                findings: assessment.findings.clone(),
                evidence_refs: vec![
                    j.evidence_digest
                        .clone()
                        .ok_or_else(|| Error::Conflict("role evidence missing".into()))?,
                ],
                model_job: Some(j.id),
            },
        )
    }
}
