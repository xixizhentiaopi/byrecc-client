mod api;
mod clients;
mod credentials;
mod install;
mod proxy;
mod state;

use std::process::ExitCode;

use anyhow::Result;
use clap::{Parser, Subcommand};

#[derive(Debug, Parser)]
#[command(
    name = "byrectl",
    version,
    about = "Secure ByreCC client installer and MCP proxy"
)]
struct Cli {
    /// Use fixed localhost endpoints for ByreCC service development.
    #[arg(long, global = true, hide = true)]
    development: bool,

    #[command(subcommand)]
    command: Command,
}

#[derive(Debug, Subcommand)]
enum Command {
    /// Log in, install the Skill, and configure detected AI clients.
    Install(install::InstallArgs),
    /// Rotate the credential for the existing local installation.
    Login(install::LoginArgs),
    /// Revoke the active installation and remove its local credential.
    Logout(install::LogoutArgs),
    /// Remove ByreCC client configuration, Skill files, credentials, and CLI.
    Uninstall(install::UninstallArgs),
    /// Show local installation and client configuration status.
    Status,
    /// Check local files, server installation state, and MCP connectivity.
    Doctor,
    /// List supported, detected, configured, and recorded AI clients.
    Clients,
    /// Run MCP transport commands used by configured AI clients.
    Mcp {
        #[command(subcommand)]
        command: McpCommand,
    },
}

#[derive(Debug, Subcommand)]
enum McpCommand {
    /// Proxy MCP stdio requests to the authenticated ByreCC HTTP endpoint.
    Proxy {
        #[arg(long)]
        installation: String,
    },
}

fn run() -> Result<()> {
    let cli = Cli::parse();
    let endpoints = api::Endpoints::for_mode(cli.development);
    match cli.command {
        Command::Install(args) => install::run_install(args, &endpoints),
        Command::Login(args) => install::run_login(args, &endpoints),
        Command::Logout(args) => install::run_logout(args, &endpoints),
        Command::Uninstall(args) => install::run_uninstall(args, &endpoints),
        Command::Status => install::show_status(),
        Command::Doctor => install::run_doctor(&endpoints),
        Command::Clients => install::show_clients(),
        Command::Mcp {
            command: McpCommand::Proxy { installation },
        } => proxy::run(&installation, &endpoints),
    }
}

fn main() -> ExitCode {
    match run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("Error: {error:#}");
            ExitCode::FAILURE
        }
    }
}
