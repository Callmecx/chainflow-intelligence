use chainflow::{
    api::{self, AppState},
    config::Rules,
    models::*,
    providers::{FixtureProvider, Provider, ProviderHub},
    storage::Store,
};
use clap::{Parser, Subcommand};
use std::{path::PathBuf, sync::Arc};

#[derive(Parser)]
#[command(
    version,
    about = "ChainFlow Intelligence: read-only public market research"
)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}
#[derive(Subcommand)]
enum Command {
    ScanOnce {
        #[arg(long)]
        output: PathBuf,
        #[arg(long, default_value = "data/chainflow.sqlite")]
        db: String,
        #[arg(long)]
        fixture: Option<PathBuf>,
    },
    Serve {
        #[arg(long, default_value = "data/chainflow.sqlite")]
        db: String,
        #[arg(long, default_value_t = 8787)]
        port: u16,
        #[arg(long, default_value_t = 120)]
        interval_seconds: u64,
    },
    Doctor {
        #[arg(long, default_value = "data/chainflow.sqlite")]
        db: String,
    },
    Archive {
        #[arg(long, default_value = "data/chainflow.sqlite")]
        db: String,
        #[arg(long)]
        output: String,
        #[arg(long, default_value_t = 30)]
        retention_days: i64,
    },
    Restore {
        #[arg(long, default_value = "data/chainflow.sqlite")]
        db: String,
        #[arg(long)]
        input: String,
    },
}
#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .json()
        .with_env_filter(
            tracing_subscriber::EnvFilter::from_default_env()
                .add_directive(tracing::Level::INFO.into()),
        )
        .with_writer(std::io::stderr)
        .init();
    let cli = Cli::parse();
    let rules = Rules::from_env()?;
    match cli.command {
        Command::ScanOnce {
            output,
            db,
            fixture,
        } => {
            let store = Store::open(&db)?;
            let (result, mode) = if let Some(path) = fixture {
                let provider = FixtureProvider::from_json(&std::fs::read_to_string(path)?)?;
                (provider.scan(&[]).await, "DEMO_FIXTURE")
            } else {
                (
                    ProviderHub::new()?.scan(&store.watchlist().await?).await,
                    "PUBLIC_SNAPSHOT",
                )
            };
            let count = store
                .ingest(result.items, result.sources, result.provider_errors, rules)
                .await?;
            let mut snapshot = store.snapshot(mode).await?;
            // A one-shot file contains only current scan data, not old cached DB rows.
            let scan_at = timestamp(&snapshot.generated_at_utc).unwrap_or_else(chrono::Utc::now);
            snapshot.items.retain(|v| {
                timestamp(&v.token.fetched_at).is_some_and(|at| (scan_at - at).num_seconds() < 300)
            });
            if count == 0 {
                snapshot
                    .provider_errors
                    .push("UNAVAILABLE: scan produced no fresh samples".into());
                snapshot.items.clear();
            }
            if let Some(parent) = output.parent().filter(|p| !p.as_os_str().is_empty()) {
                std::fs::create_dir_all(parent)?;
            }
            let temp = output.with_extension("json.tmp");
            std::fs::write(&temp, serde_json::to_vec_pretty(&snapshot)?)?;
            std::fs::rename(&temp, &output)?;
            println!(
                "{}",
                serde_json::json!({"output":output,"mode":mode,"samples":count,"sources":snapshot.sources,"provider_errors":snapshot.provider_errors,"generated_at_utc":snapshot.generated_at_utc})
            );
            anyhow::ensure!(
                count > 0,
                "no fresh public samples; failure snapshot was written"
            );
        }
        Command::Serve {
            db,
            port,
            interval_seconds,
        } => {
            anyhow::ensure!(
                interval_seconds >= 60,
                "minimum scan interval is 60 seconds"
            );
            let store = Store::open(&db)?;
            let state = Arc::new(AppState::new(store, ProviderHub::new()?, rules));
            let worker = state.clone();
            let archive_dir = PathBuf::from(&db)
                .parent()
                .unwrap_or(std::path::Path::new("data"))
                .join("archives");
            let task = tokio::spawn(async move {
                let mut tick =
                    tokio::time::interval(std::time::Duration::from_secs(interval_seconds));
                tick.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
                let mut rounds = 0u64;
                loop {
                    tick.tick().await;
                    if let Err(e) = worker.scan().await {
                        tracing::error!(event="scan_error",error=%e);
                        worker
                            .scan_errors
                            .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                    }
                    rounds += 1;
                    if rounds.is_multiple_of(720) {
                        let filename = archive_dir.join(format!(
                            "samples-{}.json",
                            chrono::Utc::now().format("%Y%m%dT%H%M%SZ")
                        ));
                        if let Err(e) = worker.store.archive(&filename.to_string_lossy(), 30).await
                        {
                            tracing::error!(event="archive_error",error=%e);
                        }
                    }
                }
            });
            let listener =
                tokio::net::TcpListener::bind((std::net::Ipv4Addr::LOCALHOST, port)).await?;
            tracing::info!(event="server_listening",address=%listener.local_addr()?,mode="LOCAL_LIVE");
            let result = axum::serve(listener, api::router(state))
                .with_graceful_shutdown(async {
                    let _ = tokio::signal::ctrl_c().await;
                })
                .await;
            task.abort();
            result?;
        }
        Command::Doctor { db } => println!(
            "{}",
            serde_json::to_string_pretty(&Store::open(&db)?.doctor().await?)?
        ),
        Command::Archive {
            db,
            output,
            retention_days,
        } => println!(
            "archived_samples={}",
            Store::open(&db)?.archive(&output, retention_days).await?
        ),
        Command::Restore { db, input } => println!(
            "restored_samples={}",
            Store::open(&db)?.restore(&input).await?
        ),
    }
    Ok(())
}
