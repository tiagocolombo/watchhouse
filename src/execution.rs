//! The only admitted adapter has no filesystem, network, model, or effect authority.
use crate::contracts::{ExecutionResult, V1};
#[derive(Debug, Clone)]
pub struct Task {
    pub schema_version: V1,
    pub assignment_id: String,
    pub objective: String,
}
pub trait ExecutionAdapter: Send + Sync {
    fn execute(&self, task: &Task) -> ExecutionResult;
}
#[derive(Clone)]
pub struct FakeAdapter;
impl ExecutionAdapter for FakeAdapter {
    fn execute(&self, task: &Task) -> ExecutionResult {
        ExecutionResult {
            schema_version: V1,
            assignment_id: task.assignment_id.clone(),
            summary: format!(
                "Fake investigation recorded for: {}. No service was inspected; independent evidence and review are required.",
                task.objective
            ),
            conclusion: "insufficient_evidence".into(),
            actual_model: None,
            usage: None,
        }
    }
}
