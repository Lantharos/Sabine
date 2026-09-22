mod bundle;
mod commands;
mod desktop_types;
mod dev;
mod environment;
mod icon_assets;
mod install;
mod macos_bundle;
mod process_tree;
mod release;
mod runtime;
mod template;
mod update;
mod web_detect;

use std::process::ExitCode;

use bundle::BundleOptions;
use clap::Parser;
mod cli;
use cli::{Cli, Command, RuntimeSubcommand};
use install::source::InstallOptions;
use runtime::RuntimeCommand;

fn main() -> ExitCode {
    let cli = match Cli::try_parse() {
        Ok(cli) => cli,
        Err(error) if error.kind() == clap::error::ErrorKind::DisplayVersion => {
            println!("Sabine CLI: {}", sabine_service::SABINE_VERSION);
            println!(
                "Sabine service (installed): {}",
                sabine_service::installed_service_version()
                    .as_deref()
                    .unwrap_or("not installed")
            );
            println!(
                "Sabine daemon (running): {}",
                sabine_service::running_daemon_version()
                    .as_deref()
                    .unwrap_or("not running")
            );
            return ExitCode::SUCCESS;
        }
        Err(error) => error.exit(),
    };
    match cli.command {
        Command::New { name, template } => template::new_app(&name, &template),
        Command::Dev {
            source,
            release,
            no_install,
            web_only,
            no_runtime_prepare,
            command,
            args,
        } => match dev::run_dev(dev::DevOptions {
            source,
            release,
            no_install,
            web_only,
            no_runtime_prepare,
            command,
            args,
        }) {
            Ok(code) => code,
            Err(error) => {
                eprintln!("{error}");
                ExitCode::from(1)
            }
        },
        Command::Runtime { command } => runtime::run_runtime(match command {
            RuntimeSubcommand::Prepare => RuntimeCommand::Prepare,
            RuntimeSubcommand::SandboxProfile => RuntimeCommand::SandboxProfile,
            RuntimeSubcommand::List { json } => RuntimeCommand::List { json },
            RuntimeSubcommand::Install => RuntimeCommand::Install,
            RuntimeSubcommand::Remove { version } => RuntimeCommand::Remove { version },
            RuntimeSubcommand::Prune { keep } => RuntimeCommand::Prune { keep },
            RuntimeSubcommand::Doctor { json } => RuntimeCommand::Doctor { json },
        }),
        Command::Install {
            bundle,
            env_file,
            source,
            id,
            name,
            command,
            autostart,
            no_desktop,
        } => match install::run(
            InstallOptions {
                source,
                id,
                name,
                command,
                autostart,
                desktop: !no_desktop,
            },
            bundle,
            env_file,
        ) {
            Ok(code) => code,
            Err(error) => {
                eprintln!("{error}");
                ExitCode::from(1)
            }
        },
        Command::Uninstall {
            target,
            system,
            purge,
        } => match install::uninstall::run(target, system, purge) {
            Ok(code) => code,
            Err(error) => {
                eprintln!("{error}");
                ExitCode::FAILURE
            }
        },
        Command::Update { target, all, force } => match update::run(target, all, force) {
            Ok(code) => code,
            Err(error) => {
                eprintln!("{error}");
                ExitCode::from(1)
            }
        },
        Command::Bundle {
            env_file,
            source,
            target,
            out,
            release,
            no_build,
            binary,
            no_web_build,
            web_build,
            web_root,
            web_dist,
            id,
            name,
            version,
            json,
            offline,
        } => match bundle::bundle(BundleOptions {
            source,
            target,
            out,
            release,
            no_build,
            binary,
            no_web_build,
            web_build,
            web_root,
            web_dist,
            id,
            name,
            version,
            json,
            offline,
            env_files: env_file,
        }) {
            Ok(code) => code,
            Err(error) => {
                eprintln!("{error}");
                ExitCode::from(1)
            }
        },
        Command::ReleaseManifest {
            source,
            output,
            channel,
            artifact,
            executable,
        } => match release::write_manifest(&source, &output, &channel, &artifact, &executable) {
            Ok(()) => {
                println!("wrote {}", output.display());
                ExitCode::SUCCESS
            }
            Err(error) => {
                eprintln!("{error}");
                ExitCode::from(1)
            }
        },
        Command::SystemReleaseManifest {
            version,
            directory,
            output,
        } => match release::write_system_manifest(&version, &directory, &output) {
            Ok(()) => {
                println!("wrote {}", output.display());
                ExitCode::SUCCESS
            }
            Err(error) => {
                eprintln!("{error}");
                ExitCode::from(1)
            }
        },
        Command::ReleaseKeygen { public_output } => {
            match release::generate_signing_key(&public_output) {
                Ok(private_key) => {
                    println!("{private_key}");
                    ExitCode::SUCCESS
                }
                Err(error) => {
                    eprintln!("{error}");
                    ExitCode::from(1)
                }
            }
        }
        Command::ReleasePublicKey => match release::signing_public_key() {
            Ok(public_key) => {
                println!("{public_key}");
                ExitCode::SUCCESS
            }
            Err(error) => {
                eprintln!("{error}");
                ExitCode::from(1)
            }
        },
        Command::ReleaseInit { repository } => {
            match release::initialize_github_release(repository.as_deref()) {
                Ok(public_key) => {
                    println!("configured signed immutable releases (public key {public_key})");
                    ExitCode::SUCCESS
                }
                Err(error) => {
                    eprintln!("{error}");
                    ExitCode::from(1)
                }
            }
        }
    }
}
