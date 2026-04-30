use std::{fs, path::PathBuf, sync::Arc};

use anyhow::{Context, Result};
use clap::{Parser, Subcommand};
use llm_proxy_core::{config::default_config_json, Config};
use llm_proxy_dashboard::DashboardState;
use llm_proxy_db::Database;
use llm_proxy_proxy::ProxyState;
use tracing::info;
use tracing_subscriber::{fmt, EnvFilter};

#[derive(Debug, Parser)]
#[command(version, about = "OpenAI-compatible LLM monitoring proxy")]
struct Cli {
    #[arg(long, global = true)]
    config: Option<PathBuf>,
    #[command(subcommand)]
    command: Option<Command>,
}

#[derive(Debug, Subcommand)]
enum Command {
    Config {
        #[command(subcommand)]
        command: ConfigCommand,
    },
    Admin {
        #[command(subcommand)]
        command: AdminCommand,
    },
}

#[derive(Debug, Subcommand)]
enum ConfigCommand {
    Validate,
    PrintDefault,
}

#[derive(Debug, Subcommand)]
enum AdminCommand {
    ResetPassword,
}

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();

    match cli.command {
        Some(Command::Config {
            command: ConfigCommand::PrintDefault,
        }) => {
            println!("{}", default_config_json()?);
            Ok(())
        }
        Some(Command::Config {
            command: ConfigCommand::Validate,
        }) => {
            let (config, paths) = Config::load_or_create(cli.config.as_deref())?;
            config.validate()?;
            println!("valid: {}", paths.config.display());
            Ok(())
        }
        Some(Command::Admin {
            command: AdminCommand::ResetPassword,
        }) => {
            setup_logging("info")?;
            anyhow::bail!("admin reset-password is not implemented yet");
        }
        None => run(cli.config).await,
    }
}

async fn run(config_path: Option<PathBuf>) -> Result<()> {
    let (config, paths) = Config::load_or_create(config_path.as_deref())?;
    setup_logging(&config.logging.level)?;

    info!(
        config_path = %paths.config.display(),
        created = paths.created,
        "config loaded"
    );

    create_runtime_directories(&config)?;
    let database = Database::connect(&config.database).await.with_context(|| {
        format!(
            "failed to initialize database {}",
            config.database.display()
        )
    })?;

    let config = Arc::new(config);
    let proxy_state = ProxyState::new(Arc::clone(&config), database.clone());
    let dashboard_state = DashboardState::new(Arc::clone(&config), database);

    let proxy_addr = config.proxy_listen;
    let admin_addr = config.admin_listen;

    tokio::try_join!(
        llm_proxy_proxy::serve(proxy_addr, proxy_state),
        llm_proxy_dashboard::serve(admin_addr, dashboard_state)
    )?;

    Ok(())
}

fn create_runtime_directories(config: &Config) -> Result<()> {
    if let Some(parent) = config.database.parent() {
        fs::create_dir_all(parent)
            .with_context(|| format!("failed to create database directory {}", parent.display()))?;
    }
    fs::create_dir_all(&config.payload_dir).with_context(|| {
        format!(
            "failed to create payload directory {}",
            config.payload_dir.display()
        )
    })?;
    Ok(())
}

fn setup_logging(level: &str) -> Result<()> {
    let filter = EnvFilter::try_new(level).context("invalid log level")?;
    fmt().json().with_env_filter(filter).init();
    Ok(())
}
