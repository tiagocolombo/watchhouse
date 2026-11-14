use clap::{Parser, Subcommand};
use std::{path::PathBuf, sync::Arc, time::Duration};
use watchhouse::{application::Application, contracts::*, domain::SystemClock, store::Store};
#[derive(Parser)]
#[command(version, about = "Watchhouse local fake-only coordinator")]
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
            return Ok(());
        }
        if !ran {
            tokio::time::sleep(Duration::from_millis(250)).await;
        }
    }
}
#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();
    if let Some(parent) = cli.db.parent().filter(|p| !p.as_os_str().is_empty()) {
        std::fs::create_dir_all(parent)?;
    }
    let app = Application::new(Store::open(cli.db)?, Arc::new(SystemClock));
    match cli.command {
        Command::Migrate => println!("Schema version 3 validated"),
        Command::Tick => println!(
            "{} obligation(s) became due",
            app.process_due_obligations()? + app.supervise_team()?
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
            tokio::select! { result = work(app,once,false,identity,block,stall) => result?, _ = tokio::signal::ctrl_c() => {} }
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
                "Watchhouse: http://{authority}\nLocal operator token: {token}\nFake execution only; case results require operator review."
            );
            let router = watchhouse::http::router(app.clone(), token, authority);
            tokio::select! {
                result = axum::serve(listener,router).with_graceful_shutdown(async { let _ = tokio::signal::ctrl_c().await; }) => result?,
                result = work(app,false,manual_workers,"fake-investigator".into(),false,false) => result?,
            }
        }
    }
    Ok(())
}
