use clap::Parser;
use std::path::PathBuf;
use tracing::info;
use tracing_subscriber::{fmt, EnvFilter};

/// deckd — headless Stream Deck daemon for Raspberry Pi
#[derive(Parser)]
#[command(name = "deckd", version, about)]
struct Cli {
    /// Path to the config file (TOML).
    #[arg(short, long, default_value = "/etc/deckd/config.toml")]
    config: PathBuf,

    /// Enable JSON log output (for journald).
    #[arg(long)]
    json: bool,

    /// Validate config and exit.
    #[arg(long)]
    check: bool,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();

    // Init tracing.
    let filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("deckd=info"));

    if cli.json {
        fmt().with_env_filter(filter).json().init();
    } else {
        fmt().with_env_filter(filter).init();
    }

    info!("deckd v{}", env!("CARGO_PKG_VERSION"));

    // Load config.
    let config_path = cli
        .config
        .canonicalize()
        .unwrap_or_else(|_| cli.config.clone());
    let config = deckd::config::load(&config_path)?;

    if cli.check {
        println!(
            "config OK: {} pages, {} total buttons",
            config.pages.len(),
            config
                .pages
                .values()
                .map(|p| p.buttons.len())
                .sum::<usize>(),
        );
        return Ok(());
    }

    info!("loaded config: {} pages", config.pages.len());

    // Run the daemon.
    deckd::daemon::run(config, config_path).await?;

    Ok(())
}
