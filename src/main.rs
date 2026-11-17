use clap::{Parser, Subcommand};
use std::{path::PathBuf, sync::Arc, time::Duration};
use watchhouse::{application::Application, contracts::*, domain::SystemClock, store::Store};
#[derive(Parser)]
#[command(version, about = "Watchhouse durable incident coordinator")]
struct Cli {
    #[arg(long, default_value = ".watchhouse/watchhouse.db", global = true)]
    db: PathBuf,
    #[command(subcommand)]
    command: Command,
}
#[derive(Subcommand)]
enum Command {
    /// Serve the bundled UI and process fake assignments independently of browsers.
    Serve {
        #[arg(long, default_value_t = 8787)]
        port: u16,
        #[arg(long, env = "WATCHHOUSE_TOKEN", hide_env_values = true)]
        token: Option<String>,
        /// Keep the deterministic scheduler active but let separate fake workers claim work.
        #[arg(long)]
        manual_workers: bool,
    },
    /// Process queued/expired fake work against the same transactional store.
    Worker {
        #[arg(long)]
        once: bool,
        #[arg(long, default_value = "fake-investigator")]
        identity: String,
        #[arg(long, conflicts_with = "stall")]
        block: bool,
        #[arg(long, conflicts_with = "block")]
        stall: bool,
    },
    Team {
        #[command(subcommand)]
        action: TeamCommand,
    },
    /// Install a trusted policy only through the OS-level operator control plane.
    Policy {
        #[arg(long)]
        file: PathBuf,
        #[arg(long)]
        operator: String,
    },
    Notifications {
        #[arg(long)]
        file: PathBuf,
        #[arg(long)]
        operator: String,
    },
    Sentry {
        #[arg(long)]
        file: PathBuf,
        #[arg(long)]
        operator: String,
    },
    /// Prepare a frozen base and inspect a pinned runtime without model execution.
    RepairSource {
        #[arg(long)]
        file: PathBuf,
        #[arg(long)]
        operator: String,
    },
    RepairSourceStatus,
    /// Install an immutable, pinned local Docker boundary through the OS operator plane.
    PublisherPolicies,
    PublisherWorker {
        #[arg(long)]
        once: bool,
    },
    /// Inspect a lost publisher effect through the OS operator plane; never replay a write.
    ReconcilePublication {
        case: String,
        job: String,
        #[arg(long)]
        operator: String,
    },
    PublisherPolicy {
        #[arg(long)]
        file: PathBuf,
        #[arg(long)]
        operator: String,
    },
    ContainerPolicy {
        #[arg(long)]
        file: PathBuf,
        #[arg(long)]
        operator: String,
    },
    /// Dispatch configured native container work; does not invoke a model.
    ContainerWorker {
        #[arg(long)]
        once: bool,
    },
    /// Install an immutable model-client profile through the local OS operator plane.
    ModelPolicy {
        #[arg(long)]
        file: PathBuf,
        #[arg(long)]
        operator: String,
    },
    /// Run explicitly queued model work. May use paid authentication when configured.
    ModelWorker {
        #[arg(long)]
        once: bool,
    },
    WorkerPolicyStatus,
    WorkerPolicy {
        #[arg(long)]
        file: PathBuf,
        #[arg(long)]
        operator: String,
    },
    Knowledge {
        #[arg(long)]
        service: String,
        #[arg(long)]
        environment: String,
        #[arg(long)]
        repository: String,
        #[arg(long)]
        release: Option<String>,
        #[arg(long, default_value = "")]
        query: String,
    },
    SourceStatus,
    SourceSync,
    /// Apply and validate migrations without starting workers.
    Migrate,
    /// Reconsider up to 100 persisted deadlines without invoking any executor.
    Tick,
    Case {
        #[command(subcommand)]
        action: CaseCommand,
    },
}
#[derive(Subcommand)]
enum CaseCommand {
    Publish {
        id: String,
        #[arg(long)]
        file: PathBuf,
    },
    PublicationPlan {
        id: String,
        #[arg(long)]
        candidate: String,
        #[arg(long)]
        review: String,
    },
    ImportHallmark {
        id: String,
        #[arg(long)]
        file: PathBuf,
    },
    ExportHallmark {
        id: String,
        #[arg(long)]
        file: PathBuf,
    },
    AdoptModelProposal {
        id: String,
        #[arg(long)]
        file: PathBuf,
    },
    AdoptModelReview {
        id: String,
        #[arg(long)]
        file: PathBuf,
    },
    RequestModel {
        id: String,
        #[arg(long)]
        file: PathBuf,
    },
    ModelJob {
        id: String,
        #[arg(long)]
        job: String,
    },
    ManageModel {
        id: String,
        #[arg(long)]
        file: PathBuf,
    },
    QualifyContainer {
        id: String,
        #[arg(long)]
        file: PathBuf,
    },
    ContainerJob {
        id: String,
        #[arg(long)]
        job: String,
    },
    ManageContainer {
        id: String,
        #[arg(long)]
        file: PathBuf,
    },
    FreezeCandidate {
        id: String,
        #[arg(long)]
        file: PathBuf,
    },
    CheckCandidate {
        id: String,
        #[arg(long)]
        file: PathBuf,
    },
    ReviewCandidate {
        id: String,
        #[arg(long)]
        file: PathBuf,
    },
    Learning {
        id: String,
        #[arg(long)]
        file: PathBuf,
    },
    LearningMarkdown {
        id: String,
    },
    Recovery {
        id: String,
        #[arg(long)]
        file: PathBuf,
    },
    RecoveryMarkdown {
        id: String,
    },
    AdapterTrialStatus {
        id: String,
        #[arg(long)]
        trial: String,
    },
    /// Run a fake adapter protocol command; never starts a real harness.
    AdapterTrial {
        id: String,
        #[arg(long)]
        file: PathBuf,
    },
    AdapterTrialEvents {
        id: String,
        #[arg(long)]
        trial: String,
        #[arg(long, default_value_t = 0)]
        after: i64,
    },
    ReconcileWorkerQualification {
        id: String,
        #[arg(long)]
        file: PathBuf,
    },
    QualifyWorker {
        id: String,
        #[arg(long)]
        file: PathBuf,
    },
    WorkerQualificationMarkdown {
        id: String,
    },
    RepairMarkdown {
        id: String,
    },
    RequestRepair {
        id: String,
        #[arg(long)]
        file: PathBuf,
    },
    RecordIncidentArtifact {
        id: String,
        #[arg(long)]
        file: PathBuf,
    },
    InvestigationMarkdown {
        id: String,
    },
    OperateIncident {
        id: String,
        #[arg(long)]
        file: PathBuf,
    },
    DeclareIncident {
        id: String,
        #[arg(long)]
        file: PathBuf,
    },
    IncidentMarkdown {
        id: String,
    },
    Create {
        #[arg(long)]
        title: String,
        #[arg(long, default_value = "")]
        description: String,
        #[arg(long)]
        key: String,
        #[arg(long, default_value = "operator")]
        actor: String,
        #[arg(long, default_value = "Manual intake")]
        reason: String,
    },
    /// Record or revoke an evidence-backed correlation without merging cases.
    Correlate {
        id: String,
        #[arg(long)]
        file: PathBuf,
    },
    QualificationMarkdown {
        id: String,
    },
    /// Record an attributed observation from a versioned JSON command file.
    RecordEvidence {
        id: String,
        #[arg(long)]
        file: PathBuf,
    },
    /// Record an evidence-backed disposition and durable reassessment obligation.
    RecordDisposition {
        id: String,
        #[arg(long)]
        file: PathBuf,
    },
    /// Apply an explicit review/recurrence/contradiction/scope-change command.
    Reconsider {
        id: String,
        #[arg(long)]
        file: PathBuf,
    },
    List,
    Show {
        id: String,
    },
    Rename {
        id: String,
        #[arg(long)]
        title: String,
        #[arg(long)]
        expected_version: i64,
        #[arg(long)]
        key: String,
        #[arg(long, default_value = "operator")]
        actor: String,
        #[arg(long)]
        reason: String,
    },
}
#[derive(Subcommand)]
enum TeamCommand {
    ResolveEscalation {
        id: String,
        #[arg(long)]
        file: PathBuf,
    },
    ReconcileDelivery {
        id: String,
        #[arg(long)]
        file: PathBuf,
    },
    Challenge {
        id: String,
        #[arg(long)]
        file: PathBuf,
    },
    Gate {
        id: String,
        #[arg(long)]
        file: PathBuf,
    },
    Claim {
        #[arg(long, default_value = "fake-investigator")]
        executor: String,
        #[arg(long)]
        key: String,
    },
    Inbox,
    Finish {
        #[arg(long)]
        file: PathBuf,
    },
    Heartbeat {
        #[arg(long)]
        file: PathBuf,
    },
    Progress {
        #[arg(long)]
        file: PathBuf,
    },
    ReportBlock {
        #[arg(long)]
        file: PathBuf,
    },
    Message {
        id: String,
        #[arg(long)]
        file: PathBuf,
    },
    AcknowledgeMessage {
        id: String,
        #[arg(long)]
        file: PathBuf,
    },
    OfferHandoff {
        id: String,
        #[arg(long)]
        file: PathBuf,
    },
    AcceptHandoff {
        id: String,
        #[arg(long)]
        file: PathBuf,
    },
    AcknowledgeEscalation {
        id: String,
        #[arg(long)]
        file: PathBuf,
    },
}
fn read_command<T: serde::de::DeserializeOwned>(file: PathBuf) -> anyhow::Result<T> {
    Ok(serde_json::from_slice(&std::fs::read(file)?)?)
}
async fn work(
    app: Application,
    once: bool,
    manual: bool,
    identity: String,
    block: bool,
    stall: bool,
) -> anyhow::Result<()> {
    let mut held: Option<watchhouse::application::Claim> = None;
    let mut heartbeat_at = tokio::time::Instant::now();
    loop {
        let worker = app.clone();
        let who = identity.clone();
        let previous = held.clone();
        let heartbeat_due = heartbeat_at.elapsed() >= Duration::from_secs(5);
        let outcome = tokio::task::spawn_blocking(move || -> watchhouse::error::Result<_> {
            if manual {
                return Ok((
                    worker.process_due_obligations()? + worker.supervise_team()? > 0,
                    None,
                ));
            }
            if stall {
                let changed = worker.process_due_obligations()? + worker.supervise_team()?;
                if let Some(c) = previous {
                    if heartbeat_due {
                        let command = RunCommand {
                            meta: CommandMeta {
                                schema_version: V1,
                                idempotency_key: uuid::Uuid::new_v4().to_string(),
                                actor: who,
                                reason: "Fake process remains alive, without useful progress"
                                    .into(),
                            },
                            proof: RunProof {
                                run_id: c.run_id.clone(),
                                fence: c.fence.clone(),
                            },
                        };
                        match worker.heartbeat(command) {
                            Ok(_) => {}
                            Err(watchhouse::error::Error::Conflict(_)) => return Ok((false, None)),
                            Err(e) => return Err(e),
                        }
                    }
                    return Ok((changed > 0, Some(c)));
                }
                return Ok((changed > 0, worker.accept_or_claim_fake(&who)?));
            }
            Ok((worker.run_fake_worker_once(&who, block)?, None))
        })
        .await?;
        let (ran, next) = match outcome {
            Ok(value) => value,
            Err(watchhouse::error::Error::Conflict(message)) => {
                eprintln!("Worker state changed: {message}");
                (false, None)
            }
            Err(error) => return Err(error.into()),
        };
        held = next;
        if heartbeat_due {
            heartbeat_at = tokio::time::Instant::now();
        }
        if once {
            let notifier = app.clone();
            tokio::task::spawn_blocking(move || {
                notifier.process_notifications(&watchhouse::notifications::SlackWebhook)
            })
            .await??;
            return Ok(());
        }
        if !ran {
            tokio::time::sleep(Duration::from_millis(250)).await;
        }
    }
}
async fn sources(app: Application) -> anyhow::Result<()> {
    loop {
        let a = app.clone();
        tokio::task::spawn_blocking(move || a.sentry_tick(&watchhouse::sentry::LiveSentry))
            .await??;
        tokio::time::sleep(Duration::from_secs(1)).await;
    }
}
async fn notifications(app: Application) -> anyhow::Result<()> {
    loop {
        let a = app.clone();
        tokio::task::spawn_blocking(move || {
            a.process_notifications(&watchhouse::notifications::SlackWebhook)
        })
        .await??;
        tokio::time::sleep(Duration::from_millis(250)).await;
    }
}
fn main() -> anyhow::Result<()> {
    // The coordinator dispatch future is large in debug builds. Keep its stack explicit
    // across native platforms; child worker resource policies remain separate.
    std::thread::Builder::new()
        .name("watchhouse-coordinator".into())
        .stack_size(8 * 1024 * 1024)
        .spawn(run)?
        .join()
        .map_err(|_| anyhow::anyhow!("coordinator thread panicked"))?
}
#[tokio::main]
async fn run() -> anyhow::Result<()> {
    let cli = Cli::parse();
    if let Some(parent) = cli.db.parent().filter(|p| !p.as_os_str().is_empty()) {
        std::fs::create_dir_all(parent)?;
    }
    let app = Application::new(Store::open(cli.db)?, Arc::new(SystemClock));
    match cli.command {
        Command::ModelPolicy { file, operator } => {
            let source = std::fs::canonicalize(&file)?;
            println!(
                "{}",
                serde_json::to_string_pretty(&app.install_model_policy(
                    read_command(file)?,
                    &operator,
                    &source.to_string_lossy()
                )?)?
            );
        }
        Command::ModelWorker { once } => loop {
            let a = app.clone();
            tokio::task::spawn_blocking(move || a.supervise_team()).await??;
            let ran = app
                .run_model_once(&watchhouse::model_runtime::ops::Native)
                .await?;
            if once {
                break;
            }
            // Polling a running client must not busy-loop; it never extends the original deadline.
            tokio::time::sleep(Duration::from_millis(if ran { 100 } else { 250 })).await;
        },
        Command::PublisherPolicies => println!(
            "{}",
            serde_json::to_string_pretty(&app.publisher_policies()?)?
        ),
        Command::PublisherWorker { once } => loop {
            let a = app.clone();
            let ran = tokio::task::spawn_blocking(move || {
                a.supervise_team()?;
                a.run_publisher_once(&watchhouse::delivery::lifecycle::Native)
            })
            .await??;
            if once {
                break;
            }
            tokio::time::sleep(Duration::from_millis(if ran { 100 } else { 250 })).await;
        },
        Command::ReconcilePublication {
            case,
            job,
            operator,
        } => println!(
            "{}",
            serde_json::to_string_pretty(&app.reconcile_publication(&case, &job, &operator)?)?
        ),
        Command::PublisherPolicy { file, operator } => {
            let source = std::fs::canonicalize(&file)?;
            println!(
                "{}",
                serde_json::to_string_pretty(&app.install_publisher_policy(
                    read_command(file)?,
                    &operator,
                    &source.to_string_lossy()
                )?)?
            );
        }
        Command::ContainerPolicy { file, operator } => println!(
            "{}",
            serde_json::to_string_pretty(
                &app.install_container_policy(read_command(file)?, &operator)?
            )?
        ),
        Command::ContainerWorker { once } => loop {
            let a = app.clone();
            tokio::task::spawn_blocking(move || a.supervise_team()).await??;
            let ran = app
                .run_container_once(&watchhouse::real_execution::worker::Native)
                .await?;
            if once {
                break;
            }
            if !ran {
                tokio::time::sleep(Duration::from_millis(250)).await;
            }
        },
        Command::Policy { file, operator } => {
            let source = std::fs::canonicalize(&file)?;
            let bytes = std::fs::read(&source)?;
            anyhow::ensure!(bytes.len() <= 16384, "policy exceeds 16KiB");
            let policy = serde_json::from_slice(&bytes)?;
            println!(
                "{}",
                serde_json::to_string_pretty(&app.install_policy(
                    policy,
                    &operator,
                    &source.to_string_lossy()
                )?)?
            );
        }
        Command::Notifications { file, operator } => {
            println!(
                "{}",
                serde_json::to_string_pretty(
                    &app.configure_notifications(read_command(file)?, &operator)?
                )?
            );
        }
        Command::Sentry { file, operator } => println!(
            "{}",
            serde_json::to_string_pretty(&app.configure_sentry(read_command(file)?, &operator)?)?
        ),
        Command::RepairSource { file, operator } => println!(
            "{}",
            serde_json::to_string_pretty(
                &app.install_repair_source(read_command(file)?, &operator)
                    .await?
            )?
        ),
        Command::WorkerPolicyStatus => println!(
            "{}",
            serde_json::to_string_pretty(&app.worker_policy_status()?)?
        ),
        Command::WorkerPolicy { file, operator } => println!(
            "{}",
            serde_json::to_string_pretty(
                &app.install_worker_policy(read_command(file)?, &operator)?
            )?
        ),
        Command::RepairSourceStatus => println!(
            "{}",
            serde_json::to_string_pretty(&app.repair_source_status()?)?
        ),
        Command::Knowledge {
            service,
            environment,
            repository,
            release,
            query,
        } => println!(
            "{}",
            serde_json::to_string_pretty(&app.knowledge_search(
                &service,
                &environment,
                &repository,
                release.as_deref(),
                &query
            )?)?
        ),
        Command::SourceStatus => {
            println!("{}", serde_json::to_string_pretty(&app.sentry_health()?)?)
        }
        Command::SourceSync => println!(
            "{} source observations processed",
            app.sentry_tick(&watchhouse::sentry::LiveSentry)?
        ),
        Command::Migrate => println!("All registered migrations validated"),
        Command::Tick => println!(
            "{} obligation(s) became due",
            app.process_due_obligations()?
                + app.supervise_team()?
                + app.process_notifications(&watchhouse::notifications::SlackWebhook)?
        ),
        Command::Team { action } => {
            let value = match action {
                TeamCommand::Claim { executor, key } => {
                    serde_json::to_value(app.claim_work(ClaimWork {
                        meta: CommandMeta {
                            schema_version: V1,
                            idempotency_key: key,
                            actor: "operator".into(),
                            reason: "Local fake worker requested queued work".into(),
                        },
                        executor,
                    })?)?
                }
                TeamCommand::Challenge { id, file } => {
                    serde_json::to_value(app.challenge(&id, read_command(file)?)?)?
                }
                TeamCommand::Gate { id, file } => {
                    serde_json::to_value(app.record_gate(&id, read_command(file)?)?)?
                }
                TeamCommand::ResolveEscalation { id, file } => {
                    serde_json::to_value(app.resolve_escalation(&id, read_command(file)?)?)?
                }
                TeamCommand::ReconcileDelivery { id, file } => {
                    serde_json::to_value(app.reconcile_delivery(&id, read_command(file)?)?)?
                }
                TeamCommand::Inbox => serde_json::to_value(app.escalation_inbox()?)?,
                TeamCommand::Finish { file } => {
                    serde_json::to_value(app.finish_fake(read_command(file)?)?)?
                }
                TeamCommand::Heartbeat { file } => {
                    serde_json::to_value(app.heartbeat(read_command(file)?)?)?
                }
                TeamCommand::Progress { file } => {
                    serde_json::to_value(app.report_progress(read_command(file)?)?)?
                }
                TeamCommand::ReportBlock { file } => {
                    serde_json::to_value(app.report_block(read_command(file)?)?)?
                }
                TeamCommand::Message { id, file } => {
                    serde_json::to_value(app.post_team_message(&id, read_command(file)?)?)?
                }
                TeamCommand::AcknowledgeMessage { id, file } => {
                    serde_json::to_value(app.acknowledge_message(&id, read_command(file)?)?)?
                }
                TeamCommand::OfferHandoff { id, file } => {
                    serde_json::to_value(app.offer_handoff(&id, read_command(file)?)?)?
                }
                TeamCommand::AcceptHandoff { id, file } => {
                    serde_json::to_value(app.accept_handoff(&id, read_command(file)?)?)?
                }
                TeamCommand::AcknowledgeEscalation { id, file } => {
                    serde_json::to_value(app.acknowledge_escalation(&id, read_command(file)?)?)?
                }
            };
            println!("{}", serde_json::to_string_pretty(&value)?);
        }
        Command::Case { action } => {
            let output = match action {
                CaseCommand::Publish { id, file } => {
                    serde_json::to_value(app.enqueue_publication(&id, read_command(file)?)?)?
                }
                CaseCommand::PublicationPlan {
                    id,
                    candidate,
                    review,
                } => serde_json::to_value(app.prepare_publication(&id, &candidate, &review)?)?,
                CaseCommand::ImportHallmark { id, file } => {
                    serde_json::to_value(app.import_hallmark(&id, read_command(file)?)?)?
                }
                CaseCommand::ExportHallmark { id, file } => {
                    serde_json::to_value(app.export_hallmark(&id, read_command(file)?)?)?
                }
                CaseCommand::AdoptModelProposal { id, file } => {
                    serde_json::to_value(app.adopt_model_proposal(&id, read_command(file)?)?)?
                }
                CaseCommand::AdoptModelReview { id, file } => {
                    serde_json::to_value(app.adopt_model_review(&id, read_command(file)?)?)?
                }
                CaseCommand::RequestModel { id, file } => {
                    serde_json::to_value(app.request_model(&id, read_command(file)?)?)?
                }
                CaseCommand::ModelJob { id, job } => {
                    serde_json::to_value(app.model_job(&id, &job)?)?
                }
                CaseCommand::ManageModel { id, file } => serde_json::to_value(
                    app.manage_model(
                        &id,
                        read_command(file)?,
                        &watchhouse::model_runtime::ops::Native,
                    )
                    .await?,
                )?,
                CaseCommand::QualifyContainer { id, file } => {
                    serde_json::to_value(app.qualify_container(&id, read_command(file)?)?)?
                }
                CaseCommand::ContainerJob { id, job } => {
                    serde_json::to_value(app.container_job(&id, &job)?)?
                }
                CaseCommand::ManageContainer { id, file } => serde_json::to_value(
                    app.manage_container(
                        &id,
                        read_command(file)?,
                        &watchhouse::real_execution::worker::Native,
                    )
                    .await?,
                )?,
                CaseCommand::FreezeCandidate { id, file } => {
                    serde_json::to_value(app.freeze_candidate(&id, read_command(file)?)?)?
                }
                CaseCommand::CheckCandidate { id, file } => {
                    serde_json::to_value(app.request_candidate_check(&id, read_command(file)?)?)?
                }
                CaseCommand::ReviewCandidate { id, file } => {
                    serde_json::to_value(app.review_candidate(&id, read_command(file)?)?)?
                }
                CaseCommand::Learning { id, file } => {
                    serde_json::to_value(app.learning_command(&id, read_command(file)?)?)?
                }
                CaseCommand::LearningMarkdown { id } => {
                    print!("{}", app.learning_markdown(&id)?);
                    return Ok(());
                }
                CaseCommand::Recovery { id, file } => {
                    serde_json::to_value(app.recovery_command(&id, read_command(file)?)?)?
                }
                CaseCommand::RecoveryMarkdown { id } => {
                    print!("{}", app.recovery_markdown(&id)?);
                    return Ok(());
                }
                CaseCommand::AdapterTrialStatus { id, trial } => {
                    serde_json::to_value(app.adapter_trial_status(&id, &trial)?)?
                }
                CaseCommand::AdapterTrial { id, file } => {
                    serde_json::to_value(app.adapter_trial(&id, read_command(file)?)?)?
                }
                CaseCommand::AdapterTrialEvents { id, trial, after } => {
                    serde_json::to_value(app.adapter_trial_events(&id, &trial, after)?)?
                }
                CaseCommand::ReconcileWorkerQualification { id, file } => serde_json::to_value(
                    app.reconcile_worker_qualification(&id, read_command(file)?)?,
                )?,
                CaseCommand::QualifyWorker { id, file } => {
                    serde_json::to_value(app.qualify_worker(&id, read_command(file)?).await?)?
                }
                CaseCommand::WorkerQualificationMarkdown { id } => {
                    print!("{}", app.worker_qualification_markdown(&id)?);
                    return Ok(());
                }
                CaseCommand::RepairMarkdown { id } => {
                    print!("{}", app.repair_markdown(&id)?);
                    return Ok(());
                }
                CaseCommand::RequestRepair { id, file } => {
                    serde_json::to_value(app.request_repair(&id, read_command(file)?)?)?
                }
                CaseCommand::Create {
                    title,
                    description,
                    key,
                    actor,
                    reason,
                } => serde_json::to_value(app.create_case(CreateCase {
                    schema_version: V1,
                    idempotency_key: key,
                    actor,
                    reason,
                    title,
                    description,
                })?)?,
                CaseCommand::RecordEvidence { id, file } => {
                    serde_json::to_value(app.record_evidence(
                        &id,
                        serde_json::from_slice::<RecordEvidence>(&std::fs::read(file)?)?,
                    )?)?
                }
                CaseCommand::Correlate { id, file } => {
                    serde_json::to_value(app.correlate_case(&id, read_command(file)?)?)?
                }
                CaseCommand::QualificationMarkdown { id } => {
                    print!("{}", app.qualification_markdown(&id)?);
                    return Ok(());
                }
                CaseCommand::RecordDisposition { id, file } => {
                    serde_json::to_value(app.record_disposition(
                        &id,
                        serde_json::from_slice::<RecordDisposition>(&std::fs::read(file)?)?,
                    )?)?
                }
                CaseCommand::Reconsider { id, file } => {
                    serde_json::to_value(app.reconsider_case(
                        &id,
                        serde_json::from_slice::<ReconsiderCase>(&std::fs::read(file)?)?,
                    )?)?
                }
                CaseCommand::DeclareIncident { id, file } => {
                    serde_json::to_value(app.declare_incident(&id, read_command(file)?)?)?
                }
                CaseCommand::IncidentMarkdown { id } => {
                    print!("{}", app.incident_markdown(&id)?);
                    return Ok(());
                }
                CaseCommand::OperateIncident { id, file } => {
                    serde_json::to_value(app.operate_incident(&id, read_command(file)?)?)?
                }
                CaseCommand::RecordIncidentArtifact { id, file } => {
                    serde_json::to_value(app.record_incident_artifact(&id, read_command(file)?)?)?
                }
                CaseCommand::InvestigationMarkdown { id } => {
                    print!("{}", app.investigation_markdown(&id)?);
                    return Ok(());
                }
                CaseCommand::List => serde_json::to_value(app.list_cases()?)?,
                CaseCommand::Show { id } => serde_json::to_value(app.snapshot(&id)?)?,
                CaseCommand::Rename {
                    id,
                    title,
                    expected_version,
                    key,
                    actor,
                    reason,
                } => serde_json::to_value(app.rename_case(
                    &id,
                    RenameCase {
                        schema_version: V1,
                        idempotency_key: key,
                        actor,
                        reason,
                        title,
                        expected_version,
                    },
                )?)?,
            };
            println!("{}", serde_json::to_string_pretty(&output)?);
        }
        Command::Worker {
            once,
            identity,
            block,
            stall,
        } => {
            if once {
                work(app, true, false, identity, block, stall).await?;
            } else {
                tokio::select! { result = work(app.clone(),false,false,identity,block,stall) => result?, result=sources(app.clone())=>result?, result=notifications(app)=>result?, _ = tokio::signal::ctrl_c() => {} }
            }
        }
        Command::Serve {
            port,
            token,
            manual_workers,
        } => {
            let token = token.unwrap_or_else(|| uuid::Uuid::new_v4().to_string());
            anyhow::ensure!(
                token.len() >= 32,
                "WATCHHOUSE_TOKEN must be at least 32 bytes"
            );
            let listener =
                tokio::net::TcpListener::bind((std::net::Ipv4Addr::LOCALHOST, port)).await?;
            let authority = listener.local_addr()?.to_string();
            eprintln!(
                "Watchhouse: http://{authority}\nLocal operator token: {token}\nBuilt-in assignments are synthetic; native container checks and model roles require separately configured container-worker/model-worker processes."
            );
            let router = watchhouse::http::router(app.clone(), token, authority);
            tokio::select! {
                result = axum::serve(listener,router).with_graceful_shutdown(async { let _ = tokio::signal::ctrl_c().await; }) => result?,
                result = work(app.clone(),false,manual_workers,"fake-investigator".into(),false,false) => result?,
                result = sources(app.clone()) => result?,
                result = notifications(app) => result?,
            }
        }
    }
    Ok(())
}
