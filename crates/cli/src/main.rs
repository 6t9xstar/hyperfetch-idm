use std::path::PathBuf;

use clap::{Parser, Subcommand};
use hyperfetch_core::{DownloadSpec, DownloadState, Engine, Settings};

#[derive(Parser)]
#[command(
    name = "hyperfetch",
    version,
    about = "HyperFetch - Open-Source Download Manager"
)]
struct Cli {
    /// Path to the HyperFetch database.
    #[arg(long, global = true, default_value = "hyperfetch.db")]
    db: PathBuf,

    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Inspect a URL without downloading it.
    Probe { url: String },
    /// Download a URL and wait for completion.
    Download {
        url: String,
        #[arg(long)]
        dir: Option<String>,
        #[arg(long)]
        filename: Option<String>,
        #[arg(long, default_value_t = 8)]
        connections: u32,
        /// Bytes per second (0 = unlimited).
        #[arg(long, default_value_t = 0)]
        speed_limit: u64,
        /// Override the range batch size in bytes.
        #[arg(long)]
        batch_size: Option<u64>,
        /// Override the minimum split size in bytes.
        #[arg(long)]
        min_split_size: Option<u64>,
        /// Override the minimum file size for segmented mode.
        #[arg(long)]
        min_segmented_size: Option<u64>,
        #[arg(long)]
        no_hash: bool,
        /// Suppress progress output.
        #[arg(long)]
        quiet: bool,
    },
    /// List downloads in the database.
    List,
}

#[tokio::main]
async fn main() -> anyhow_like::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "hyperfetch=info".into()),
        )
        .init();

    let cli = Cli::parse();

    let download_overrides = match &cli.command {
        Command::Download {
            no_hash,
            batch_size,
            min_split_size,
            min_segmented_size,
            ..
        } => Some((!no_hash, *batch_size, *min_split_size, *min_segmented_size)),
        _ => None,
    };
    let (compute_sha256, batch_size, min_split_size, min_segmented_size) =
        download_overrides.unwrap_or((true, None, None, None));
    let mut settings = Settings {
        auto_resume: false,
        compute_sha256,
        ..Default::default()
    };
    if let Some(value) = batch_size {
        settings.batch_size = value;
    }
    if let Some(value) = min_split_size {
        settings.min_split_size = value;
    }
    if let Some(value) = min_segmented_size {
        settings.min_segmented_size = value;
    }

    let engine = Engine::open(&cli.db, Some(settings)).await?;

    match cli.command {
        Command::Probe { url } => {
            let probe = engine.probe(&url, None, None).await?;
            println!("{}", serde_json::to_string_pretty(&probe)?);
        }
        Command::Download {
            url,
            dir,
            filename,
            connections,
            speed_limit,
            quiet,
            ..
        } => {
            let spec = DownloadSpec {
                url,
                dir,
                filename,
                connections: Some(connections),
                speed_limit,
                start_now: true,
                ..Default::default()
            };
            let snapshot = engine.add(spec).await?;
            println!("queued {} ({})", snapshot.filename, snapshot.id);

            let mut events = engine.subscribe();
            loop {
                let snapshot = engine.get(snapshot.id).await?;
                match snapshot.state {
                    DownloadState::Done => {
                        println!(
                            "done: {} ({} bytes){}",
                            snapshot.filename,
                            snapshot.received,
                            snapshot
                                .sha256
                                .map(|hash| format!(" sha256={hash}"))
                                .unwrap_or_default()
                        );
                        break;
                    }
                    DownloadState::Error => {
                        eprintln!("error: {}", snapshot.error.unwrap_or_default());
                        std::process::exit(1);
                    }
                    DownloadState::Canceled => {
                        eprintln!("canceled");
                        std::process::exit(1);
                    }
                    _ => {
                        if let Ok(hyperfetch_core::EngineEvent::Progress { progress }) =
                            events.recv().await
                        {
                            if !quiet && progress.total > 0 {
                                let percent =
                                    progress.received as f64 / progress.total as f64 * 100.0;
                                println!(
                                    "{:>6.1}%  {}/s  {}",
                                    percent,
                                    human_speed(progress.speed),
                                    progress
                                        .eta_secs
                                        .map(|eta| format!("ETA {eta}s"))
                                        .unwrap_or_default()
                                );
                            }
                        }
                    }
                }
            }
        }
        Command::List => {
            for download in engine.list().await? {
                println!(
                    "{:<38} {:<10} {:>10}  {}",
                    download.id, download.state, download.received, download.filename
                );
            }
        }
    }

    engine.shutdown().await;
    Ok(())
}

fn human_speed(bytes_per_second: u64) -> String {
    const UNITS: [&str; 5] = ["B", "KiB", "MiB", "GiB", "TiB"];
    let mut value = bytes_per_second as f64;
    let mut unit = 0;
    while value >= 1024.0 && unit < UNITS.len() - 1 {
        value /= 1024.0;
        unit += 1;
    }
    format!("{value:.1} {}/s", UNITS[unit])
}

mod anyhow_like {
    pub type Result<T> = std::result::Result<T, Box<dyn std::error::Error>>;
}
