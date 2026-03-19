mod commands;
mod exit_codes;
mod output;

use clap::{Parser, Subcommand};

#[derive(Parser)]
#[command(
    name = "uniclipboard-cli",
    version,
    about = "UniClipboard command-line interface"
)]
struct Cli {
    /// Output in JSON format
    #[arg(long, global = true)]
    json: bool,

    /// Enable verbose tracing output (shows debug logs on console)
    #[arg(long, short, global = true)]
    verbose: bool,

    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Show daemon status
    Status,
    /// List paired devices (direct mode, no daemon required)
    Devices,
    /// Show space and encryption status (direct mode, no daemon required)
    SpaceStatus,
    /// Clipboard history commands (direct mode, no daemon required)
    Clipboard {
        #[command(subcommand)]
        subcommand: ClipboardCommands,
    },
}

#[derive(Subcommand)]
enum ClipboardCommands {
    /// List clipboard history entries
    List {
        /// Maximum number of entries to return (default: 50)
        #[arg(long, default_value_t = 50)]
        limit: usize,
        /// Number of entries to skip (default: 0)
        #[arg(long, default_value_t = 0)]
        offset: usize,
    },
    /// Get full detail for a single clipboard entry
    Get {
        /// Entry ID
        id: String,
    },
    /// Clear all clipboard history
    Clear,
}

fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();

    let rt = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()?;

    let exit_code = rt.block_on(async {
        match cli.command {
            Commands::Status => commands::status::run(cli.json, cli.verbose).await,
            Commands::Devices => commands::devices::run(cli.json, cli.verbose).await,
            Commands::SpaceStatus => commands::space_status::run(cli.json, cli.verbose).await,
            Commands::Clipboard { subcommand } => match subcommand {
                ClipboardCommands::List { limit, offset } => {
                    commands::clipboard::run_list(cli.json, cli.verbose, limit, offset).await
                }
                ClipboardCommands::Get { id } => {
                    commands::clipboard::run_get(cli.json, cli.verbose, id).await
                }
                ClipboardCommands::Clear => {
                    commands::clipboard::run_clear(cli.json, cli.verbose).await
                }
            },
        }
    });

    std::process::exit(exit_code);
}
