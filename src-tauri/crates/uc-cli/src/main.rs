mod autostop;
mod commands;
mod exit_codes;
mod local_daemon;
mod output;
mod ui;

use clap::{Parser, Subcommand};

/// Initialise AppKit enough for headless macOS CLI invocations.
///
/// `clipboard-rs` eagerly calls `+[NSPasteboard generalPasteboard]` during
/// `wire_dependencies`, which returns NULL and panics when the process
/// has not loaded AppKit (typical for a CLI launched from a shell that
/// does not carry a proper Cocoa context). `NSApplicationLoad` is the
/// documented way to bootstrap AppKit in non-`.app` processes.
#[cfg(target_os = "macos")]
fn init_macos_appkit() {
    extern "C" {
        fn NSApplicationLoad() -> bool;
    }
    unsafe {
        let _ = NSApplicationLoad();
    }
}
#[cfg(not(target_os = "macos"))]
fn init_macos_appkit() {}

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

    /// Run in development mode (use file-based secure storage instead of system keychain)
    #[cfg_attr(not(debug_assertions), arg(long, global = true, hide = true))]
    #[cfg_attr(debug_assertions, arg(long, global = true))]
    dev: bool,

    /// Override the active profile (equivalent to `UC_PROFILE`). Isolates
    /// data dir, keychain, and iroh identity — needed to run two CLI
    /// instances on the same machine for end-to-end pairing testing.
    #[arg(long, global = true, value_name = "NAME")]
    profile: Option<String>,

    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Start the daemon (background by default, use --foreground for log streaming)
    Start {
        /// Run daemon in foreground (log output to terminal)
        #[arg(
            long,
            short = 'f',
            help = "Run daemon in foreground (log output to terminal)"
        )]
        foreground: bool,
    },
    /// Stop the running daemon
    Stop,
    /// Show daemon status
    Status,
    /// Initialize a new encrypted space on this profile (Slice 1 A1).
    ///
    /// Runs directly against the core facade — no daemon in the loop.
    Init {
        /// Space passphrase. If omitted, prompts interactively with
        /// confirmation. Pass this flag only in non-interactive contexts
        /// such as the single-machine e2e test script.
        #[arg(long)]
        passphrase: Option<String>,
        /// Display name advertised to paired peers. Defaults to the OS
        /// hostname (plus `(profile)` suffix when `--profile` is set).
        #[arg(long)]
        device_name: Option<String>,
    },
    /// Issue a pairing invitation and wait for a joiner (sponsor side).
    ///
    /// Silently resumes the local session from the KEK cached in
    /// keychain (or `--dev`'s file secure storage) by a prior `init` /
    /// `unlock` — no passphrase re-entry needed. Fails if the profile
    /// has not been initialized yet.
    Invite,
    /// Redeem an invitation and join an existing space (joiner side).
    Join {
        /// Invitation code printed by the sponsor's `invite`. Prompted
        /// interactively when omitted.
        #[arg(long)]
        code: Option<String>,
        /// Space passphrase the sponsor chose during `init`. Prompted
        /// interactively when omitted.
        #[arg(long)]
        passphrase: Option<String>,
        /// Display name advertised to the sponsor as this device's name.
        /// Defaults to the OS hostname (plus `(profile)` suffix when
        /// `--profile` is set). Persisted to settings before dialing so
        /// the B2 handshake can read it back.
        #[arg(long)]
        device_name: Option<String>,
    },
    /// Drive daemon-owned setup flows (interactive guide when no subcommand given)
    Setup {
        #[command(subcommand)]
        subcommand: Option<SetupCommands>,
    },
    /// List paired devices via the daemon API
    Devices,
    /// List members of this space with presence (online / offline / unknown).
    ///
    /// Self-contained direct mode (Slice 2 Phase 1): runs a one-off probe of
    /// all paired peers so states are fresh on every call. No daemon
    /// required. Prints `{name} ({state}) [local]` per device.
    Members,
    /// Show space and encryption status (direct mode, no daemon required)
    SpaceStatus,
    /// Dispatch one clipboard payload to every online paired peer.
    ///
    /// Self-contained direct mode. Reads text from the positional
    /// argument, or — when omitted — from stdin until EOF. Wraps the
    /// text into a single-representation `SystemClipboardSnapshot`,
    /// encodes it as a V3 envelope, and fans it out via the iroh
    /// clipboard ALPN — same wire format the daemon uses.
    Send {
        /// Plaintext to send. Omit to read from stdin until EOF.
        text: Option<String>,
    },
    /// Watch inbound clipboard payloads from paired peers and print each
    /// delivery as it lands. Press Ctrl-C to stop.
    ///
    /// Self-contained direct mode. Decodes the V3 envelope and shows the
    /// first text representation (or a per-rep summary for image-only
    /// envelopes). Does NOT write the system clipboard — that's the
    /// daemon's job; the CLI watch is purely a diagnostic observer.
    Watch,
    /// 发布或拉取加密的大 payload blob(直连模式)。
    Blob {
        #[command(subcommand)]
        subcommand: commands::blob::BlobCommands,
    },
    /// Search clipboard history (query or inspect search availability)
    Search {
        #[command(subcommand)]
        subcommand: commands::search::SearchCommands,
    },
    /// Run the daemon process inline (used internally by `start`)
    #[command(hide = true)]
    Daemon {
        /// Launched by a GUI parent that keeps stdin open for lifecycle detection
        #[arg(long)]
        gui_managed: bool,
    },
}

#[derive(Subcommand)]
enum SetupCommands {
    /// [DEPRECATED] Use `uniclipboard-cli invite` instead. Retained until libp2p setup flow is retired.
    Pair,
    /// [DEPRECATED] Use `uniclipboard-cli join <code>` instead. Retained until libp2p setup flow is retired.
    Connect,
    /// Inspect daemon-owned setup state
    Status,
    /// Reset daemon-owned setup state for repeatable local reruns
    Reset,
}

fn main() -> anyhow::Result<()> {
    init_macos_appkit();

    // rustls 0.23+ requires a process-wide `CryptoProvider` before any
    // `ClientConfig` is built. iroh-quinn installs its own on bind, and
    // if reqwest sees that provider first it hits
    // `CertificateError::BadEncoding` on any plain HTTPS handshake
    // (symptom: "bad certificate format" from `rendezvous.uniclipboard.app`).
    // Setting ring as the explicit default up front keeps both stacks on
    // the same provider. The call is idempotent and safe if something
    // already installed a provider.
    let _ = rustls::crypto::ring::default_provider().install_default();

    let cli = Cli::parse();

    if cli.dev {
        std::env::set_var("UNICLIPBOARD_ENV", "development");
    }

    // `--profile <name>` must land as `UC_PROFILE` BEFORE any bootstrap
    // code runs, because `wire_dependencies` + `apply_profile_suffix`
    // branch on that env var at path-resolution time.
    if let Some(profile) = &cli.profile {
        std::env::set_var("UC_PROFILE", profile);
    }

    // Handle `daemon` subcommand before creating the tokio runtime —
    // the daemon entrypoint creates its own runtime internally.
    if let Commands::Daemon { gui_managed } = cli.command {
        return uc_daemon::entrypoint::run(gui_managed);
    }

    let rt = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()?;

    let exit_code = rt.block_on(async {
        match cli.command {
            Commands::Start { foreground } => {
                commands::start::run(foreground, cli.json, cli.verbose).await
            }
            Commands::Stop => commands::stop::run(cli.json, cli.verbose).await,
            Commands::Status => commands::status::run(cli.json, cli.verbose).await,
            Commands::Init {
                passphrase,
                device_name,
            } => {
                commands::init::run(
                    commands::init::InitArgs {
                        passphrase,
                        device_name,
                    },
                    cli.verbose,
                )
                .await
            }
            Commands::Invite => commands::invite::run(cli.verbose).await,
            Commands::Join {
                code,
                passphrase,
                device_name,
            } => {
                commands::join::run(
                    commands::join::JoinArgs {
                        code,
                        passphrase,
                        device_name,
                    },
                    cli.verbose,
                )
                .await
            }
            Commands::Setup { subcommand } => match subcommand {
                None => commands::setup::run_interactive(cli.json, cli.verbose).await,
                Some(SetupCommands::Pair) => commands::setup::run_pair(cli.json, cli.verbose).await,
                Some(SetupCommands::Connect) => {
                    commands::setup::run_connect(cli.json, cli.verbose).await
                }
                Some(SetupCommands::Status) => {
                    commands::setup::run_status(cli.json, cli.verbose).await
                }
                Some(SetupCommands::Reset) => {
                    commands::setup::run_reset(cli.json, cli.verbose).await
                }
            },
            Commands::Devices => commands::devices::run(cli.json, cli.verbose).await,
            Commands::Members => commands::members::run(cli.json, cli.verbose).await,
            Commands::SpaceStatus => commands::space_status::run(cli.json, cli.verbose).await,
            Commands::Send { text } => {
                commands::send::run(commands::send::SendArgs { text }, cli.json, cli.verbose).await
            }
            Commands::Watch => commands::watch::run(cli.json, cli.verbose).await,
            Commands::Blob { subcommand } => {
                commands::blob::run(subcommand, cli.json, cli.verbose).await
            }
            Commands::Search { subcommand } => {
                commands::search::run(subcommand, cli.json, cli.verbose).await
            }
            Commands::Daemon { .. } => unreachable!("handled above"),
        }
    });

    std::process::exit(exit_code);
}
