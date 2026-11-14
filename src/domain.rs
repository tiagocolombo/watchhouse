//! Provider-independent state and time. No harness, HTTP, or storage types here.
use chrono::{DateTime, Duration, Utc};
use serde::{Deserialize, Serialize};
use std::sync::{Arc, Mutex};

pub trait Clock: Send + Sync {
    fn now(&self) -> DateTime<Utc>;
}
#[derive(Clone)]
pub struct SystemClock;
impl Clock for SystemClock {
    fn now(&self) -> DateTime<Utc> {
        Utc::now()
    }
}
#[derive(Clone)]
pub struct ManualClock(Arc<Mutex<DateTime<Utc>>>);
impl ManualClock {
    pub fn new(now: DateTime<Utc>) -> Self {
        Self(Arc::new(Mutex::new(now)))
    }
    pub fn advance(&self, duration: Duration) {
        *self.0.lock().unwrap() += duration;
    }
}
impl Clock for ManualClock {
    fn now(&self) -> DateTime<Utc> {
        *self.0.lock().unwrap()
    }
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum CaseStatus {
    Queued,
    Running,
    AwaitingReview,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Case {
    pub schema_version: crate::contracts::V1,
    pub id: String,
    pub title: String,
    pub description: String,
    pub owner: String,
    pub status: CaseStatus,
    pub version: i64,
    pub created_at: String,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Assignment {
    pub schema_version: crate::contracts::V1,
    pub id: String,
    pub case_id: String,
    pub role: String,
    pub owner: String,
    pub objective: String,
    pub state: String,
}

pub mod qualification;
