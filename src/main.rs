use clap::{Parser, Subcommand};
use std::{path::PathBuf, sync::Arc, time::Duration};
use watchhouse::{
    application::Application,
    contracts::{CreateCase, ReconsiderCase, RecordDisposition, RecordEvidence, RenameCase, V1},
    domain::SystemClock,
    execution::FakeAdapter,
    store::Store,
};
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
    },
    /// Process queued/expired fake work against the same transactional store.
    Worker {
        #[arg(long)]
        once: bool,
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
async fn work(app: Application, once: bool) -> anyhow::Result<()> {
    loop {
        let worker = app.clone();
        let ran = tokio::task::spawn_blocking(move || worker.run_once(&FakeAdapter)).await??;
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
        Command::Migrate => println!("Schema version 2 validated"),
        Command::Tick => println!(
            "{} obligation(s) became due",
            app.process_due_obligations()?
        ),
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
        Command::Worker { once } => {
            tokio::select! { result = work(app,once) => result?, _ = tokio::signal::ctrl_c() => {} }
        }
        Command::Serve { port, token } => {
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
                result = work(app,false) => result?,
            }
        }
    }
    Ok(())
}
