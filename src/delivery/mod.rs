//! Publication preparation has no credentials or external write authority.
pub mod objects;

use crate::{
    application::Application,
    contracts::V1,
    error::{Error, Result},
    real_execution::pipeline,
};
use serde::{Deserialize, Serialize};
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PublicationPlan {
    pub schema_version: V1,
    pub case_id: String,
    pub candidate_id: String,
    pub candidate_digest: String,
    pub review_id: String,
    pub repository: String,
    pub base_commit: String,
    pub tree: objects::PreparedTree,
    pub external_writes_authorized: bool,
}
impl Application {
    pub fn prepare_publication(
        &self,
        case: &str,
        candidate_id: &str,
        review_id: &str,
    ) -> Result<PublicationPlan> {
        let db = self.store.connect()?;
        let tx = db.unchecked_transaction()?;
        let c = pipeline::get(&tx, candidate_id)?;
        if c.case_id != case {
            return Err(Error::NotFound);
        }
        pipeline::current(&tx, &c, &self.now())?;
        let r = pipeline::review(&tx, review_id, &self.now())?;
        if !r.current || r.case_id != case || r.candidate_id != c.id {
            return Err(Error::Conflict(
                "publication requires the exact current independently reviewed candidate".into(),
            ));
        }
        if crate::store::digest(&serde_json::to_string(&c.files)?) != c.digest {
            return Err(Error::Conflict(
                "candidate manifest identity changed".into(),
            ));
        }
        let tree = objects::prepare_tree(&c)?;
        Ok(PublicationPlan {
            schema_version: V1,
            case_id: case.into(),
            candidate_id: c.id,
            candidate_digest: c.digest,
            review_id: r.id,
            repository: c.repository,
            base_commit: c.base_commit,
            tree,
            external_writes_authorized: false,
        })
    }
}
