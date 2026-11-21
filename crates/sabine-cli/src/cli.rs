use clap::{
    Parser, Subcommand,
    builder::styling::{AnsiColor, Effects, Styles},
};
use std::path::PathBuf;

const STYLES: Styles = Styles::styled()
    .header(AnsiColor::Green.on_default().effects(Effects::BOLD))
    .usage(AnsiColor::Green.on_default().effects(Effects::BOLD))
    .literal(AnsiColor::Cyan.on_default().effects(Effects::BOLD))
    .placeholder(AnsiColor::Cyan.on_default())
    .valid(AnsiColor::Cyan.on_default())
    .invalid(AnsiColor::Yellow.on_default())
    .error(AnsiColor::Red.on_default().effects(Effects::BOLD));

#[derive(Debug, Parser)]
#[command(
    name = "sabine",
    version = sabine_service::SABINE_VERSION,
    about = "Build, run and ship desktop apps made with web pages and Rust",
    styles = STYLES
)]
pub(crate) struct Cli {
    #[command(subcommand)]
    pub(crate) command: Command,
}

#[derive(Debug, Subcommand)]
pub(crate) enum Command {
    /// Create an app.
    New {
        /// The app's folder and package name.
        name: String,
        /// `app`, `react`, `svelte`, `vue`, `solid` or `notes`.
        #[arg(long, default_value = "app")]
        template: String,
        /// The reverse-DNS identifier of the app, such as com.example.tern.
        #[arg(long)]
        id: Option<String>,
    },
    /// Show the Sabine, runtime, toolchain and project this machine builds with.
    Info {
        /// The app's folder.
        #[arg(default_value = ".")]
        source: PathBuf,
    },
    /// Generate the app's icons for every platform from one image.
    Icon {
        /// The source image; the project's `[app] icon` when absent.
        image: Option<PathBuf>,
        /// The app's folder.
        #[arg(long, default_value = ".")]
        source: PathBuf,
        /// Where the icons go.
        #[arg(long, default_value = "dist/icons")]
        out: PathBuf,
    },
    /// Run the app with its interface's dev server, rebuilding when its Rust changes.
    Dev {
        /// The app's folder.
        #[arg(default_value = ".")]
        source: PathBuf,
        /// Build with Cargo's release profile.
        #[arg(long)]
        release: bool,
        /// Skip installing the interface's JavaScript packages.
        #[arg(long)]
        no_install: bool,
        /// Run only the interface's dev server.
        #[arg(long)]
        web_only: bool,
        /// Skip installing the shared runtime when it's missing.
        #[arg(long)]
        no_runtime_prepare: bool,
        /// Run a custom native command alongside the development server.
        #[arg(long)]
        command: Option<String>,
        /// Arguments passed to the app.
        #[arg(last = true)]
        args: Vec<String>,
    },
    /// Run a development install, as its shortcuts do.
    #[cfg(windows)]
    #[command(hide = true)]
    Launch {
        id: String,
        #[arg(last = true)]
        args: Vec<String>,
    },
    /// Write the app's TypeScript bindings, or check that they are current.
    Bindings {
        /// The app's folder.
        #[arg(default_value = ".")]
        source: PathBuf,
        /// Fail instead of writing when the bindings are out of date.
        #[arg(long)]
        check: bool,
    },
    /// Install, inspect and clean up the CEF runtimes apps run on.
    Runtime {
        #[command(subcommand)]
        command: RuntimeSubcommand,
    },
    /// Install a source launcher, or a production bundle with --bundle.
    Install {
        /// Build and install a production bundle instead of a launcher for the source.
        #[arg(long, conflicts_with = "command")]
        bundle: bool,
        /// Environment files to build with, on top of the app's own.
        #[arg(long, requires = "bundle")]
        env_file: Vec<PathBuf>,
        /// The app's folder.
        #[arg(default_value = ".")]
        source: PathBuf,
        /// Install under this app id instead of the project's.
        #[arg(long)]
        id: Option<String>,
        /// Install under this name instead of the project's.
        #[arg(long)]
        name: Option<String>,
        /// The command the launcher runs instead of `sabine dev`.
        #[arg(long)]
        command: Option<String>,
        /// Start the app when you sign in.
        #[arg(long)]
        autostart: bool,
        /// Leave out the menu entry and shortcuts.
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
        /// The app's id, or its folder.
        #[arg(conflicts_with = "system")]
        target: Option<String>,
        /// Remove Sabine, its runtimes and its service.
        #[arg(long)]
        system: bool,
        /// Also remove data owned by Sabine for this installation.
        #[arg(long)]
        purge: bool,
    },
    /// Update Sabine and CEF, or a named component/app.
    Update {
        /// `sabine`, `cef`, an app's id or its folder.
        target: Option<String>,
        /// Also update every installed app.
        #[arg(long, conflicts_with = "target")]
        all: bool,
        /// Bypass release soak time, retaining signature and version checks.
        #[arg(long)]
        force: bool,
    },
    /// Build the app and package it for people to install.
    Bundle {
        /// Environment files to build with, on top of the app's own.
        #[arg(long)]
        env_file: Vec<PathBuf>,
        /// The app's folder.
        #[arg(default_value = ".")]
        source: PathBuf,
        /// The packages to build, all for one operating system; repeat it or separate them with commas.
        #[arg(long = "target", default_value = "portable", value_delimiter = ',')]
        targets: Vec<String>,
        /// Where the packages go.
        #[arg(long, default_value = "dist")]
        out: PathBuf,
        /// Package a debug build instead of Cargo's release profile.
        #[arg(long)]
        debug: bool,
        /// Package the executable Cargo already built instead of building it.
        #[arg(long)]
        no_build: bool,
        /// Build the web assets and the executable without packaging them.
        #[arg(long, conflicts_with_all = ["no_build", "binary", "no_web_build", "offline"])]
        build_only: bool,
        /// Package this executable instead of building one.
        #[arg(long)]
        binary: Option<PathBuf>,
        /// Package the interface as it was last built.
        #[arg(long)]
        no_web_build: bool,
        /// The command that builds the interface, instead of the project's.
        #[arg(long)]
        web_build: Option<String>,
        /// The interface's folder, instead of the project's.
        #[arg(long)]
        web_root: Option<PathBuf>,
        /// The folder the interface builds into, instead of the project's.
        #[arg(long)]
        web_dist: Option<PathBuf>,
        /// Package under this app id instead of the project's.
        #[arg(long)]
        id: Option<String>,
        /// Package under this name instead of the project's.
        #[arg(long)]
        name: Option<String>,
        /// Package as this version instead of the project's.
        #[arg(long)]
        version: Option<String>,
        /// Describe each package as a line of JSON on standard output.
        #[arg(long)]
        json: bool,
        /// Carry the runtime and service, for machines without a network connection.
        #[arg(long)]
        offline: bool,
    },
    /// Write the signed update manifest for a release's packages.
    ReleaseManifest {
        /// The app's folder.
        #[arg(default_value = ".")]
        source: PathBuf,
        /// Where the manifest goes.
        #[arg(long, default_value = "dist/sabine-update.json")]
        output: PathBuf,
        /// The channel the release is on, such as stable or beta.
        #[arg(long, default_value = "stable")]
        channel: String,
        /// A package of the release, as `platform=path`.
        #[arg(long, required = true)]
        artifact: Vec<String>,
        /// The program a portable package starts, as `platform=path`.
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
    /// Write Sabine's own release manifest.
    SystemReleaseManifest {
        #[arg(long)]
        version: String,
        #[arg(long)]
        directory: PathBuf,
        #[arg(long)]
        output: PathBuf,
    },
    /// Make a key that signs the app's updates.
    ReleaseKeygen {
        /// Where the public key goes, for the app to check updates with.
        #[arg(long)]
        public_output: PathBuf,
    },
    /// Print the public key of the signing key in `SABINE_UPDATE_SIGNING_KEY`.
    ReleasePublicKey,
    /// Set up signed GitHub releases and the release workflow for the app in this folder.
    ReleaseInit {
        /// The GitHub repository releases go to, as `owner/name`; the folder's own when absent.
        #[arg(long)]
        repository: Option<String>,
        /// Replace an existing update signing key; copies built with the old one stop updating.
        #[arg(long)]
        rotate_key: bool,
    },
}

#[derive(Debug, Subcommand)]
pub(crate) enum RuntimeSubcommand {
    /// Install the shared runtime if it's missing and print its CEF host.
    Prepare,
    /// Print an AppArmor profile that lets the CEF host create its sandbox.
    #[cfg(target_os = "linux")]
    SandboxProfile,
    /// List the installed CEF runtimes.
    List {
        /// Print them as JSON.
        #[arg(long)]
        json: bool,
    },
    /// Install the newest stable CEF runtime.
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
        /// The runtime's version.
        version: String,
    },
    /// Remove older runtimes nothing needs anymore.
    Prune {
        /// How many of the newest runtimes to keep.
        #[arg(long, default_value_t = 2)]
        keep: usize,
    },
    /// Check that the runtime and its host can start.
    Doctor {
        /// Print the result as JSON.
        #[arg(long)]
        json: bool,
    },
}
