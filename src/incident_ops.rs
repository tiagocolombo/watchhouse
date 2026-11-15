//! Versioned incident coordination; immutable declaration and recovery facts remain separate.
use crate::{
    application::{Application, operator, receipt, save_receipt, text},
    contracts::*,
    error::{Error, Result},
    incidents::{Incident, IncidentRoles, Severity},
    store,
};
use chrono::{Duration, SecondsFormat};
use rusqlite::{Connection, OptionalExtension, Transaction, TransactionBehavior, params};
use serde::{Deserialize, Serialize};
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum IncidentRole {
    Commander,
    Operations,
    Communications,
    Scribe,
}
impl IncidentRole {
    fn owner(self, r: &IncidentRoles) -> &str {
        match self {
            Self::Commander => &r.commander,
            Self::Operations => &r.operations,
            Self::Communications => &r.communications,
            Self::Scribe => &r.scribe,
        }
    }
    fn assign(self, r: &mut IncidentRoles, owner: String) {
        match self {
            Self::Commander => r.commander = owner,
            Self::Operations => r.operations = owner,
            Self::Communications => r.communications = owner,
            Self::Scribe => r.scribe = owner,
        }
    }
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IncidentOperations {
    pub revision: u64,
    pub severity: Severity,
    pub impact: String,
    pub uncertainty: String,
    pub objective: String,
    pub roles: IncidentRoles,
    pub evidence_refs: Vec<String>,
    pub updated_at: String,
    pub next_update_at: String,
    pub update_cycle: u64,
    pub update_escalated: bool,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RoleOffer {
    pub id: String,
    pub case_id: String,
    pub role: IncidentRole,
    pub previous_owner: String,
    pub successor: String,
    pub acknowledge_by: String,
    pub state: String,
    pub reason: String,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct IncidentOperation {
    pub command: OperatorCommand,
    pub action: IncidentAction,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
pub enum IncidentAction {
    Update {
        severity: Severity,
        impact: String,
        uncertainty: String,
        objective: String,
        evidence_refs: Vec<String>,
    },
    OfferRole {
        role: IncidentRole,
        successor: String,
        acknowledge_seconds: u32,
    },
    AcceptRole {
        offer_id: String,
    },
}
pub(crate) fn initial(i: &Incident, escalated: bool) -> IncidentOperations {
    IncidentOperations {
        revision: 1,
        severity: i.declaration.severity.clone(),
        impact: i.declaration.impact.clone(),
        uncertainty: i.declaration.uncertainty.clone(),
        objective: i.declaration.objective.clone(),
        roles: i.declaration.roles.clone(),
        evidence_refs: i.declaration.evidence_refs.clone(),
        updated_at: i.declared_at.clone(),
        next_update_at: i.next_update_at.clone(),
        update_cycle: 0,
        update_escalated: escalated,
    }
}
pub(crate) fn snapshot(db: &Connection, id: &str) -> Result<Option<IncidentOperations>> {
    let raw: Option<String> = db
        .query_row(
            "SELECT record FROM incident_operations WHERE case_id=?1",
            [id],
            |r| r.get(0),
        )
        .optional()?;
    if let Some(raw) = raw {
        return Ok(Some(serde_json::from_str(&raw)?));
    }
    let Some(i) = crate::incidents::snapshot(db, id)? else {
        return Ok(None);
    };
    let escalated: bool = db.query_row(
        "SELECT update_escalated FROM incidents WHERE case_id=?1",
        [id],
        |r| r.get(0),
    )?;
    Ok(Some(initial(&i, escalated)))
}
pub(crate) fn save(tx: &Transaction<'_>, id: &str, ops: &IncidentOperations) -> Result<()> {
    tx.execute("INSERT INTO incident_operations VALUES (?1,?2) ON CONFLICT(case_id) DO UPDATE SET record=excluded.record",params![id,serde_json::to_string(ops)?])?;
    Ok(())
}
pub(crate) fn offers(db: &Connection, id: &str) -> Result<Vec<RoleOffer>> {
    let mut stmt = db
        .prepare("SELECT record,state FROM incident_role_offers WHERE case_id=?1 ORDER BY rowid")?;
    stmt.query_map([id], |r| {
        Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?))
    })?
    .map(|r| {
        let (raw, state) = r?;
        let mut offer: RoleOffer = serde_json::from_str(&raw)?;
        offer.state = state;
        Ok(offer)
    })
    .collect()
}
pub(crate) fn human(who: &str) -> Result<()> {
    text(who, "local accountable identity", 128)?;
    if who.starts_with("fake-") {
        return Err(Error::Invalid(
            "this command requires the trusted local human control plane".into(),
        ));
    }
    Ok(())
}
pub(crate) fn due(tx: &Transaction<'_>, now: &str) -> Result<usize> {
    let mut stmt = tx.prepare("SELECT i.case_id FROM incidents i WHERE (i.update_escalated=0 AND i.next_update_at<=?1) OR EXISTS(SELECT 1 FROM incident_role_offers o WHERE o.case_id=i.case_id AND o.state='pending' AND o.acknowledge_by<=?1) ORDER BY i.next_update_at,i.rowid LIMIT 100")?;
    let ids = stmt
        .query_map([now], |r| r.get::<_, String>(0))?
        .collect::<std::result::Result<Vec<_>, _>>()?;
    let mut changed = 0;
    for id in ids {
        let mut ops = snapshot(tx, &id)?.ok_or(Error::NotFound)?;
        if !ops.update_escalated && ops.next_update_at.as_str() <= now {
            let i = crate::incidents::snapshot(tx, &id)?.ok_or(Error::NotFound)?;
            let source = if ops.update_cycle == 0 {
                i.id
            } else {
                format!("{}:update:{}", i.id, ops.update_cycle)
            };
            crate::team::escalate(
                tx,
                &id,
                "incident_update",
                &source,
                &format!(
                    "Incident update overdue; {} owns the next response",
                    ops.roles.commander
                ),
                now,
            )?;
            ops.update_escalated = true;
            save(tx, &id, &ops)?;
            tx.execute(
                "UPDATE incidents SET update_escalated=1 WHERE case_id=?1",
                [&id],
            )?;
            changed += 1;
        }
        for offer in offers(tx, &id)?
            .into_iter()
            .filter(|o| o.state == "pending" && o.acknowledge_by.as_str() <= now)
        {
            tx.execute(
                "UPDATE incident_role_offers SET state='expired' WHERE id=?1",
                [&offer.id],
            )?;
            crate::team::escalate(
                tx,
                &id,
                "incident_role",
                &offer.id,
                &format!(
                    "Role acceptance overdue; {} remains accountable",
                    offer.previous_owner
                ),
                now,
            )?;
            store::emit(
                tx,
                &id,
                operator("incident-supervisor"),
                &offer.id,
                now,
                EventData::IncidentRoleChanged {
                    offer_id: offer.id.clone(),
                    state: "expired".into(),
                    reason: "Successor did not accept; prior role owner remains accountable".into(),
                },
            )?;
            changed += 1;
        }
    }
    Ok(changed)
}
impl Application {
    pub fn operate_incident(&self, id: &str, cmd: IncidentOperation) -> Result<IncidentOperations> {
        let m = &cmd.command.meta;
        text(&m.idempotency_key, "key", 128)?;
        text(&m.reason, "reason", 2000)?;
        human(&m.actor)?;
        let fp = store::digest(&format!(
            "incident-operation:{id}:{}",
            serde_json::to_string(&cmd)?
        ));
        let mut db = self.store.connect()?;
        let tx = db.transaction_with_behavior(TransactionBehavior::Immediate)?;
        if let Some(r) = receipt(&tx, &m.idempotency_key, &fp)? {
            return Ok(r);
        }
        if store::case(&tx, id)?.version != cmd.command.expected_version {
            return Err(Error::Conflict(
                "stale incident operation; reload before retrying".into(),
            ));
        }
        let mut ops = snapshot(&tx, id)?.ok_or(Error::NotFound)?;
        let now = self.now();
        match &cmd.action {
            IncidentAction::Update {
                severity,
                impact,
                uncertainty,
                objective,
                evidence_refs,
            } => {
                if m.actor != ops.roles.commander {
                    return Err(Error::Invalid(
                        "current commander must record incident updates".into(),
                    ));
                }
                for (name, value) in [
                    ("impact", impact),
                    ("uncertainty", uncertainty),
                    ("objective", objective),
                ] {
                    text(value, name, 2000)?
                }
                crate::qualification::evidence_refs(&tx, id, evidence_refs, false)?;
                let i = crate::incidents::snapshot(&tx, id)?.ok_or(Error::NotFound)?;
                let raw: String = tx.query_row(
                    "SELECT record FROM source_configs WHERE revision=?1",
                    [i.source_snapshot.configuration_revision],
                    |r| r.get(0),
                )?;
                let cfg: crate::sentry::SentryConfig = serde_json::from_str(&raw)?;
                let key = match severity {
                    Severity::Critical => "critical",
                    Severity::High => "high",
                    Severity::Moderate => "moderate",
                };
                let seconds = *cfg
                    .incident_update_seconds
                    .get(key)
                    .ok_or_else(|| Error::Invalid("missing trusted severity profile".into()))?;
                // Materialize an overdue duty before scheduling the next one; an update never resolves it.
                if !ops.update_escalated && ops.next_update_at <= now {
                    let source = if ops.update_cycle == 0 {
                        i.id.clone()
                    } else {
                        format!("{}:update:{}", i.id, ops.update_cycle)
                    };
                    crate::team::escalate(
                        &tx,
                        id,
                        "incident_update",
                        &source,
                        "Update arrived after its deadline; prior duty still requires explicit resolution",
                        &now,
                    )?;
                }
                ops.severity = severity.clone();
                ops.impact = impact.clone();
                ops.uncertainty = uncertainty.clone();
                ops.objective = objective.clone();
                ops.evidence_refs = evidence_refs.clone();
                ops.update_cycle += 1;
                ops.update_escalated = false;
                ops.next_update_at = (self.clock.now() + Duration::seconds(seconds.into()))
                    .to_rfc3339_opts(SecondsFormat::Millis, true);
                tx.execute(
                    "UPDATE incidents SET next_update_at=?1,update_escalated=0 WHERE case_id=?2",
                    params![ops.next_update_at, id],
                )?;
            }
            IncidentAction::OfferRole {
                role,
                successor,
                acknowledge_seconds,
            } => {
                if m.actor != ops.roles.commander {
                    return Err(Error::Invalid(
                        "only the commander can offer a role transfer".into(),
                    ));
                }
                human(successor)?;
                let p = crate::policy::for_case(&tx, id)?;
                if *acknowledge_seconds == 0
                    || *acknowledge_seconds > p.max_ack_seconds
                    || role.owner(&ops.roles) == successor
                {
                    return Err(Error::Invalid(
                        "distinct successor and policy-bounded acknowledgment required".into(),
                    ));
                }
                let offer = RoleOffer {
                    id: store::id("role"),
                    case_id: id.into(),
                    role: *role,
                    previous_owner: role.owner(&ops.roles).into(),
                    successor: successor.clone(),
                    acknowledge_by: (self.clock.now()
                        + Duration::seconds((*acknowledge_seconds).into()))
                    .to_rfc3339_opts(SecondsFormat::Millis, true),
                    state: "pending".into(),
                    reason: m.reason.clone(),
                };
                let pending = offers(&tx, id)?.into_iter().any(|o| {
                    o.state == "pending"
                        && std::mem::discriminant(&o.role) == std::mem::discriminant(role)
                });
                if pending {
                    return Err(Error::Conflict(
                        "role already has a pending transfer".into(),
                    ));
                }
                tx.execute(
                    "INSERT INTO incident_role_offers VALUES (?1,?2,?3,'pending',?4)",
                    params![
                        offer.id,
                        id,
                        serde_json::to_string(&offer)?,
                        offer.acknowledge_by
                    ],
                )?;
                store::emit(
                    &tx,
                    id,
                    operator(&m.actor),
                    &m.idempotency_key,
                    &now,
                    EventData::IncidentRoleChanged {
                        offer_id: offer.id.clone(),
                        state: "pending".into(),
                        reason: m.reason.clone(),
                    },
                )?;
            }
            IncidentAction::AcceptRole { offer_id } => {
                let offer = offers(&tx, id)?
                    .into_iter()
                    .find(|o| o.id == *offer_id)
                    .ok_or(Error::NotFound)?;
                if offer.state != "pending"
                    || offer.successor != m.actor
                    || offer.acknowledge_by <= now
                    || role_owner_changed(&offer, &ops)
                {
                    return Err(Error::Conflict(
                        "role offer expired, changed or not addressed to this actor".into(),
                    ));
                }
                offer.role.assign(&mut ops.roles, offer.successor.clone());
                tx.execute(
                    "UPDATE incident_role_offers SET state='accepted' WHERE id=?1",
                    [offer_id],
                )?;
                store::emit(
                    &tx,
                    id,
                    operator(&m.actor),
                    &m.idempotency_key,
                    &now,
                    EventData::IncidentRoleChanged {
                        offer_id: offer_id.clone(),
                        state: "accepted".into(),
                        reason: m.reason.clone(),
                    },
                )?;
            }
        }
        ops.revision += 1;
        ops.updated_at = now.clone();
        save(&tx, id, &ops)?;
        store::emit(
            &tx,
            id,
            operator(&m.actor),
            &m.idempotency_key,
            &now,
            EventData::IncidentUpdated {
                operations: Box::new(ops.clone()),
                reason: m.reason.clone(),
            },
        )?;
        save_receipt(&tx, &m.idempotency_key, &fp, &ops)?;
        tx.commit()?;
        Ok(ops)
    }
}
fn role_owner_changed(offer: &RoleOffer, ops: &IncidentOperations) -> bool {
    offer.role.owner(&ops.roles) != offer.previous_owner
}
