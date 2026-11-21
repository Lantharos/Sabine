mod bundle;
mod desktop;
mod dev;
mod handover;
mod info;
mod install;
mod project;
mod release;
mod runtime;
mod template;
mod ui;
mod update;
mod util;

use std::process::ExitCode;

use bundle::{BuildOptions, BundleOptions};
use clap::Parser;
mod cli;
use cli::{Cli, Command, RuntimeSubcommand};
use install::source::InstallOptions;
use runtime::RuntimeCommand;

fn main() -> ExitCode {
    handover::to_newer_cli();
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
        Command::New { name, template, id } => report(template::new_app(&name, &template, id)),
        Command::Info { source } => {
            info::run(&source);
            ExitCode::SUCCESS
        }
        Command::Icon { image, source, out } => {
            report(desktop::icons::export(image, &source, &out))
        }
        Command::Dev {
            source,
            release,
            no_install,
            web_only,
            no_runtime_prepare,
            command,
            args,
        } => report(dev::run_dev(dev::DevOptions {
            source,
            release,
            no_install,
            web_only,
            no_runtime_prepare,
            command,
            args,
            app_id: None,
        })),
        #[cfg(windows)]
        Command::Launch { id, args } => report(install::source::launch(&id, args)),
        Command::Bindings { source, check } => report(dev::run_bindings(&source, check)),
        Command::Runtime { command } => runtime::run_runtime(match command {
            RuntimeSubcommand::Prepare => RuntimeCommand::Prepare,
            #[cfg(target_os = "linux")]
            RuntimeSubcommand::SandboxProfile => RuntimeCommand::SandboxProfile,
            RuntimeSubcommand::List { json } => RuntimeCommand::List { json },
            RuntimeSubcommand::Install {
                system_wide: false, ..
            } => RuntimeCommand::Install,
            RuntimeSubcommand::Install {
                system_wide: true,
                root,
            } => RuntimeCommand::InstallSystemWide { root },
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
            system_wide,
            root,
        } => report(install::run(
            InstallOptions {
                source,
                id,
                name,
                command,
                autostart,
                desktop: !no_desktop,
            },
            match (bundle, system_wide) {
                (_, true) => install::InstallKind::SystemBundle { root },
                (true, false) => install::InstallKind::Bundle,
                (false, false) => install::InstallKind::Source,
            },
            env_file,
        )),
        Command::Uninstall {
            target,
            system,
            purge,
        } => report(install::uninstall::run(target, system, purge)),
        Command::Update { target, all, force } => report(update::run(target, all, force)),
        Command::Bundle {
            env_file,
            source,
            targets,
            out,
            debug,
            no_build,
            build_only,
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
        } => report(bundle::bundle(BundleOptions {
            build: BuildOptions {
                source,
                release: !debug,
                no_build,
                binary,
                no_web_build,
                web_build,
                web_root,
                web_dist,
                id,
                name,
                version,
                system_wide: false,
                env_files: env_file,
            },
            targets,
            out,
            build_only,
            json,
            offline,
        })),
        Command::ReleaseManifest {
            source,
            output,
            channel,
            artifact,
            executable,
            base_url,
            handover_from,
        } => report(
            release::write_manifest(release::ManifestRequest {
                source: &source,
                output: &output,
                channel: &channel,
                artifact_specs: &artifact,
                executable_specs: &executable,
                base_url: base_url.as_deref(),
                handover_from,
            })
            .map(|()| wrote(&output)),
        ),
        Command::SystemReleaseStage {
            service,
            host,
            output,
        } => report(
            release::stage_system_release(&service, &host, &output).map(|()| ExitCode::SUCCESS),
        ),
        Command::SystemReleaseManifest {
            version,
            directory,
            output,
        } => report(
            release::write_system_manifest(&version, &directory, &output).map(|()| wrote(&output)),
        ),
        Command::ReleaseKeygen { public_output } => {
            report(release::generate_signing_key(&public_output).map(|key| printed(&key)))
        }
        Command::ReleasePublicKey => report(release::signing_public_key().map(|key| printed(&key))),
        Command::ReleaseInit {
            repository,
            rotate_key,
        } => report(
            release::initialize_github_release(repository.as_deref(), rotate_key).map(|key| {
                println!("configured signed immutable releases (public key {key})");
                ExitCode::SUCCESS
            }),
        ),
    }
}

fn wrote(output: &std::path::Path) -> ExitCode {
    println!("wrote {}", output.display());
    ExitCode::SUCCESS
}

fn printed(value: &str) -> ExitCode {
    println!("{value}");
    ExitCode::SUCCESS
}

fn report(result: Result<ExitCode, String>) -> ExitCode {
    result.unwrap_or_else(|error| {
        ui::error(&error);
        ExitCode::FAILURE
    })
}
