use clap::{Parser, Subcommand};
use std::path::PathBuf;

#[derive(Debug, Parser)]
#[command(name = "sabine", version = sabine_service::SABINE_VERSION, about = "Sabine web runtime tooling")]
pub(crate) struct Cli {
    #[command(subcommand)]
    pub(crate) command: Command,
}

#[derive(Debug, Subcommand)]
pub(crate) enum Command {
    New {
        name: String,
        #[arg(long, default_value = "app")]
        template: String,
    },
    Dev {
        #[arg(default_value = ".")]
        source: PathBuf,
        #[arg(long)]
        release: bool,
        #[arg(long)]
        no_install: bool,
        #[arg(long)]
        web_only: bool,
        #[arg(long)]
        no_runtime_prepare: bool,
        /// Run a custom native command alongside the development server.
        #[arg(long)]
        command: Option<String>,
        #[arg(last = true)]
        args: Vec<String>,
    },
    /// Write the app's TypeScript bindings, or check that they are current.
    Bindings {
        #[arg(default_value = ".")]
        source: PathBuf,
        /// Fail instead of writing when the bindings are out of date.
        #[arg(long)]
        check: bool,
    },
    Runtime {
        #[command(subcommand)]
        command: RuntimeSubcommand,
    },
    /// Install a source launcher, or a production bundle with --bundle.
    Install {
        #[arg(long, conflicts_with = "command")]
        bundle: bool,
        #[arg(long, requires = "bundle")]
        env_file: Vec<PathBuf>,
        #[arg(default_value = ".")]
        source: PathBuf,
        #[arg(long)]
        id: Option<String>,
        #[arg(long)]
        name: Option<String>,
        #[arg(long)]
        command: Option<String>,
        #[arg(long)]
        autostart: bool,
        #[arg(long)]
        no_desktop: bool,
        /// Install the bundle for every user of the machine.
        #[arg(long, requires = "bundle", conflicts_with_all = ["autostart", "no_desktop"])]
        system_wide: bool,
        /// Stage a system-wide install under this directory, such as an OS image.
        #[arg(long, requires = "system_wide")]
        root: Option<PathBuf>,
    },
    /// Remove an installed app, or Sabine itself with --system.
    Uninstall {
        #[arg(conflicts_with = "system")]
        target: Option<String>,
        #[arg(long)]
        system: bool,
        /// Also remove data owned by Sabine for this installation.
        #[arg(long)]
        purge: bool,
    },
    /// Update Sabine and CEF, or a named component/app.
    Update {
        target: Option<String>,
        #[arg(long, conflicts_with = "target")]
        all: bool,
        /// Bypass release soak time, retaining signature and version checks.
        #[arg(long)]
        force: bool,
    },
    Bundle {
        #[arg(long)]
        env_file: Vec<PathBuf>,
        #[arg(default_value = ".")]
        source: PathBuf,
        #[arg(long, default_value = "portable")]
        target: String,
        #[arg(long, default_value = "dist")]
        out: PathBuf,
        /// Package a debug build instead of Cargo's release profile.
        #[arg(long)]
        debug: bool,
        #[arg(long)]
        no_build: bool,
        #[arg(long)]
        binary: Option<PathBuf>,
        #[arg(long)]
        no_web_build: bool,
        #[arg(long)]
        web_build: Option<String>,
        #[arg(long)]
        web_root: Option<PathBuf>,
        #[arg(long)]
        web_dist: Option<PathBuf>,
        #[arg(long)]
        id: Option<String>,
        #[arg(long)]
        name: Option<String>,
        #[arg(long)]
        version: Option<String>,
        #[arg(long)]
        json: bool,
        #[arg(long)]
        offline: bool,
    },
    ReleaseManifest {
        #[arg(default_value = ".")]
        source: PathBuf,
        #[arg(long, default_value = "dist/sabine-update.json")]
        output: PathBuf,
        #[arg(long, default_value = "stable")]
        channel: String,
        #[arg(long, required = true)]
        artifact: Vec<String>,
        #[arg(long)]
        executable: Vec<String>,
        /// The HTTPS folder the packages are uploaded to; GitHub sources default to the release's downloads.
        #[arg(long)]
        base_url: Option<String>,
        /// The GitHub repository copies still update from, when an app moves from it to Schelf.
        #[arg(long, value_name = "OWNER/REPOSITORY", conflicts_with = "base_url")]
        handover_from: Option<String>,
    },
    /// Lay out the service, daemon, CEF host and CLI of a Sabine system release.
    #[command(hide = true)]
    SystemReleaseStage {
        #[arg(long)]
        service: PathBuf,
        #[arg(long)]
        host: PathBuf,
        #[arg(long)]
        output: PathBuf,
    },
    SystemReleaseManifest {
        #[arg(long)]
        version: String,
        #[arg(long)]
        directory: PathBuf,
        #[arg(long)]
        output: PathBuf,
    },
    ReleaseKeygen {
        #[arg(long)]
        public_output: PathBuf,
    },
    ReleasePublicKey,
    /// Set up signed GitHub releases and the release workflow for the app in this folder.
    ReleaseInit {
        #[arg(long)]
        repository: Option<String>,
        /// Replace an existing update signing key; copies built with the old one stop updating.
        #[arg(long)]
        rotate_key: bool,
    },
}

#[derive(Debug, Subcommand)]
pub(crate) enum RuntimeSubcommand {
    Prepare,
    /// Print an AppArmor profile that lets the CEF host create its sandbox.
    #[cfg(target_os = "linux")]
    SandboxProfile,
    List {
        #[arg(long)]
        json: bool,
    },
    Install {
        /// Install the Sabine service, host and CEF runtime for every user of the machine.
        #[arg(long)]
        system_wide: bool,
        /// Stage a system-wide install under this directory, such as an OS image.
        #[arg(long, requires = "system_wide")]
        root: Option<PathBuf>,
    },
    /// Remove an installed CEF runtime; `sabine runtime list` shows them.
    Remove {
        version: String,
    },
    Prune {
        #[arg(long, default_value_t = 2)]
        keep: usize,
    },
    Doctor {
        #[arg(long)]
        json: bool,
    },
}
