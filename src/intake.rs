//! A sanitized deterministic source for contract tests; no provider credentials or I/O.
use crate::contracts::CreateCase;
#[derive(Clone)]
pub struct FakeSource;
impl FakeSource {
    pub fn manual_command(&self, key: &str) -> CreateCase {
        let mut cmd: CreateCase =
            serde_json::from_str(include_str!("../contracts/v1/manual-case.json"))
                .expect("versioned fake source fixture must validate");
        cmd.idempotency_key = key.into();
        cmd
    }
}
