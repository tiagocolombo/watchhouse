# Implementation guide

This folder is the durable design and implementation handoff for Watchhouse. It is intended to let a new contributor or agent continue without the original conversation.

Baseline date: 2026-11-14. M0/M1 plus the first bounded M2 core communication/supervision/handoff slice are implemented; see [STATUS](STATUS.md) for verified scope. Other capabilities below remain planned.

## Reading order

| Document | Purpose |
| --- | --- |
| [STATUS](STATUS.md) | Current reality and next work |
| [Requirements](01-requirements.md) | Confirmed product scope, invariants, and boundaries |
| [Architecture](02-architecture.md) | Components, state ownership, durability, and deployment |
| [Domain contracts](03-domain-contracts.md) | Entities, versioning, relationships, and events |
| [Incident process](04-incident-process.md) | Triage, response, recovery, review, and follow-up |
| [Team and communication](05-team-communication.md) | Roles, collaboration, challenges, stuck work, escalation |
| [Execution adapters](06-execution-adapters.md) | Harness independence, capabilities, run lifecycle |
| [Integrations](07-integrations.md) | Observability, repositories, delivery, Hallmark |
| [Knowledge](08-knowledge.md) | Lessons, runbooks, known errors, controlled learning |
| [Web and API](09-web-api.md) | Operating interface, application API, visibility |
| [Policy and security](10-policy-security.md) | Authority, isolation, evidence trust, merge decisions |
| [Roadmap](11-roadmap.md) | Ordered implementation slices and exit criteria |
| [Verification](12-verification.md) | Scenarios and acceptance evidence |
| [Operating Watchhouse](13-operations.md) | Factory health, backup, upgrades, failure handling |
| [Decisions](14-decisions.md) | Confirmed direction, proposals, open choices |
| [References](15-references.md) | Primary background sources and verification notes |
| [Templates](templates/README.md) | Operational artifact structures |

## Interpretation

MUST states a product invariant or a requirement of the proposed implementation contract. SHOULD states a recommended default. Exact technology selections, schemas, endpoints, and timing values remain proposals unless recorded as accepted decisions.

The product requirements are confirmed from the owner's instructions. The detailed implementation below is a proposed blueprint, not a claim that every technical choice has been approved.

## Work sequence

Begin with a persisted case, deterministic job scheduler, fake execution adapter, and a minimal web timeline. Establish crash recovery and escalation before connecting an autonomous coding agent.

Build complete, observable vertical slices. Do not build disconnected prompt files and call them a working team.

Use STATUS.md as the short handoff. Use the numbered documents for detailed contracts. Keep requirements traceable to roadmap milestones and acceptance scenarios.
