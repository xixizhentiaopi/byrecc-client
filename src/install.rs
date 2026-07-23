use std::fs::{self, OpenOptions};
use std::io::{self, BufRead, Write};
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::thread;
use std::time::{Duration, Instant};

use anyhow::{Context, Result, bail};
use clap::{Args, ValueEnum};
use zeroize::Zeroizing;

use crate::api::{ActivateRequest, ApiClient, DevicePoll, Endpoints};
use crate::clients::{self, McpMode};
use crate::credentials;
use crate::state::{self, LocalInstallation};

const SKILL_VERSION: &str = include_str!("../skills/byrecc/version.txt");

#[derive(Clone, Copy, Debug, ValueEnum)]
pub enum Channel {
    Stable,
    Beta,
}

#[derive(Debug, Args)]
pub struct InstallArgs {
    /// Apply the displayed plan without a confirmation prompt.
    #[arg(long)]
    yes: bool,

    /// Comma-separated client IDs to configure.
    #[arg(long, value_delimiter = ',', conflicts_with = "all_clients")]
    clients: Vec<String>,

    /// Configure every detected supported client.
    #[arg(long, conflicts_with = "clients")]
    all_clients: bool,

    /// Do not install the ByreCC Skill.
    #[arg(long)]
    skip_skill: bool,

    /// Do not modify MCP client configuration.
    #[arg(long)]
    skip_mcp: bool,

    /// Store the Bearer key directly in each client config.
    #[arg(long)]
    direct: bool,

    /// Explicitly opt in to anonymous installation telemetry.
    #[arg(long)]
    telemetry: bool,

    /// Release channel selected by the bootstrap installer.
    #[arg(long, value_enum, default_value_t = Channel::Stable)]
    channel: Channel,

    /// Confirm the CLI version selected by the bootstrap installer.
    #[arg(long)]
    version: Option<String>,

    /// Print the plan without login, file writes, or key creation.
    #[arg(long)]
    dry_run: bool,
}

#[derive(Debug, Args)]
pub struct LoginArgs {
    /// Apply the displayed plan without a confirmation prompt.
    #[arg(long)]
    yes: bool,

    /// Override the clients recorded by the existing installation.
    #[arg(long, value_delimiter = ',')]
    clients: Vec<String>,

    /// Store the Bearer key directly in each client config.
    #[arg(long)]
    direct: bool,

    /// Print the plan without login, file writes, or key creation.
    #[arg(long)]
    dry_run: bool,
}

#[derive(Debug, Args)]
pub struct LogoutArgs {
    /// Apply the displayed plan without a confirmation prompt.
    #[arg(long)]
    yes: bool,

    /// Remove the local credential without revoking it on the ByreCC service.
    #[arg(long)]
    local_only: bool,
}

#[derive(Debug, Args)]
pub struct UninstallArgs {
    /// Apply the displayed plan without a confirmation prompt.
    #[arg(long)]
    yes: bool,

    /// Remove local files without revoking the credential on the ByreCC service.
    #[arg(long)]
    local_only: bool,

    /// Leave the ByreCC Skill installed.
    #[arg(long)]
    keep_skill: bool,

    /// Leave the byrectl executable installed.
    #[arg(long)]
    keep_binary: bool,
}

struct Plan {
    yes: bool,
    clients: Vec<String>,
    skip_skill: bool,
    skip_mcp: bool,
    direct: bool,
    dry_run: bool,
}

pub fn run_install(args: InstallArgs, endpoints: &Endpoints) -> Result<()> {
    if let Some(version) = &args.version
        && version != env!("CARGO_PKG_VERSION")
    {
        bail!(
            "bootstrap selected CLI version {version}, but this binary is {}",
            env!("CARGO_PKG_VERSION")
        );
    }
    if matches!(args.channel, Channel::Beta) {
        println!("  Release channel: beta (selected by bootstrap)");
    }
    if args.telemetry {
        println!("  Telemetry: opted in; no event is transmitted by this CLI version.");
    } else {
        println!("  Telemetry: disabled by default.");
    }
    let selected = select_clients(&args.clients, args.all_clients, None)?;
    run_plan(
        Plan {
            yes: args.yes,
            clients: selected,
            skip_skill: args.skip_skill,
            skip_mcp: args.skip_mcp,
            direct: args.direct,
            dry_run: args.dry_run,
        },
        endpoints,
    )
}

pub fn run_login(args: LoginArgs, endpoints: &Endpoints) -> Result<()> {
    let state = state::load()?;
    let existing_clients = state
        .active_installation
        .as_ref()
        .and_then(|id| state.installations.get(id))
        .map(|installation| installation.clients.as_slice());
    let selected = select_clients(&args.clients, false, existing_clients)?;
    run_plan(
        Plan {
            yes: args.yes,
            clients: selected,
            skip_skill: false,
            skip_mcp: false,
            direct: args.direct,
            dry_run: args.dry_run,
        },
        endpoints,
    )
}

pub fn show_status() -> Result<()> {
    let state = state::load()?;
    println!("ByreCC local status");
    println!("  CLI: {}", env!("CARGO_PKG_VERSION"));
    println!(
        "  Skill: {}",
        if state::home_dir()?
            .join(".agents/skills/byrecc/SKILL.md")
            .exists()
        {
            SKILL_VERSION.trim()
        } else {
            "not installed"
        }
    );
    match state.active_installation {
        Some(id) => {
            let installation = state
                .installations
                .get(&id)
                .with_context(|| format!("active installation {id} is missing from local state"))?;
            println!("  Installation: {id}");
            println!("  API key ID: {}", installation.api_key_id);
            println!(
                "  Credential storage: {:?}",
                installation.credential_storage
            );
            println!("  Clients: {}", installation.clients.join(", "));
            println!("  MCP endpoint: {}", installation.mcp_url);
        }
        None => println!("  Installation: not logged in"),
    }
    Ok(())
}

pub fn run_logout(args: LogoutArgs, endpoints: &Endpoints) -> Result<()> {
    let mut local = state::load()?;
    let Some(installation_id) = local.active_installation.clone() else {
        println!("ByreCC is already logged out on this device.");
        return Ok(());
    };
    let installation = local
        .installations
        .get(&installation_id)
        .cloned()
        .with_context(|| format!("active installation {installation_id} is missing from state"))?;
    ensure_matching_endpoint(&installation, endpoints)?;

    println!("\nByreCC logout plan\n");
    println!("  Installation: {installation_id}");
    println!(
        "  Server credential: {}",
        if args.local_only {
            "leave active (--local-only)"
        } else {
            "revoke"
        }
    );
    println!("  Local credential: remove");
    println!("  MCP client configuration: unchanged");
    println!("  Skill and byrectl binary: unchanged\n");
    warn_local_only(args.local_only);
    if !args.yes && !confirm_from_tty("Continue? [y/N] ")? {
        bail!("logout cancelled")
    }

    revoke_installation(&installation_id, &installation, endpoints, args.local_only)?;
    credentials::delete(&installation_id, installation.credential_storage)
        .context("remove local installation credential")?;
    local.installations.remove(&installation_id);
    local.active_installation = None;
    state::save(&local)?;

    println!("\nLogout complete. The local credential was removed.");
    if args.local_only {
        println!("The server credential remains active; revoke it from the ByreCC console.");
    }
    Ok(())
}

pub fn run_uninstall(args: UninstallArgs, endpoints: &Endpoints) -> Result<()> {
    let mut local = state::load()?;
    let active = local.active_installation.as_ref().and_then(|id| {
        local
            .installations
            .get(id)
            .map(|item| (id.clone(), item.clone()))
    });
    if let Some((_, installation)) = &active {
        ensure_matching_endpoint(installation, endpoints)?;
    }

    println!("\nByreCC uninstall plan\n");
    println!(
        "  Server credential: {}",
        match (&active, args.local_only) {
            (None, _) => "no active local installation",
            (Some(_), true) => "leave active (--local-only)",
            (Some(_), false) => "revoke",
        }
    );
    println!("  Local credential and state: remove active installation");
    println!("  MCP config: remove only the `byrecc` entry from supported clients");
    println!(
        "  Skill: {}",
        if args.keep_skill {
            "keep"
        } else {
            "remove known ByreCC files"
        }
    );
    println!(
        "  CLI: {}",
        if args.keep_binary {
            "keep"
        } else {
            "remove only from ~/.local/bin/byrectl"
        }
    );
    println!("  Unrelated client configuration and unknown Skill files are preserved.\n");
    warn_local_only(args.local_only && active.is_some());
    if !args.yes && !confirm_from_tty("Continue? [y/N] ")? {
        bail!("uninstall cancelled")
    }

    if let Some((installation_id, installation)) = &active {
        revoke_installation(installation_id, installation, endpoints, args.local_only)?;
    }

    let mut config_changes = Vec::new();
    for client in clients::SUPPORTED {
        match clients::remove(client) {
            Ok(change) => {
                println!("  {client} checked: {}", change.path.display());
                config_changes.push(change);
            }
            Err(error) => {
                rollback_configs(&config_changes);
                return Err(error).with_context(|| format!("remove {client} configuration"));
            }
        }
    }

    if let Some((installation_id, installation)) = &active {
        if let Err(error) = credentials::delete(installation_id, installation.credential_storage) {
            rollback_configs(&config_changes);
            return Err(error).context("remove local installation credential");
        }
        local.installations.remove(installation_id);
        local.active_installation = None;
        if let Err(error) = state::save(&local) {
            rollback_configs(&config_changes);
            return Err(error).context("save local state after uninstall");
        }
    }

    if !args.keep_skill {
        clients::uninstall_skill().context("remove ByreCC Skill")?;
        println!("  ByreCC Skill files removed.");
    }

    if !args.keep_binary {
        remove_managed_binary()?;
    }

    println!("\nUninstall complete.");
    if args.local_only && active.is_some() {
        println!("The server credential remains active; revoke it from the ByreCC console.");
    }
    Ok(())
}

fn ensure_matching_endpoint(installation: &LocalInstallation, endpoints: &Endpoints) -> Result<()> {
    if installation.api_base != endpoints.api_base {
        bail!(
            "installation belongs to {}, but this command targets {}; use the same environment used during installation",
            installation.api_base,
            endpoints.api_base
        );
    }
    Ok(())
}

fn warn_local_only(local_only: bool) {
    if local_only {
        println!(
            "  WARNING: --local-only does not revoke the server credential.\n\
             Use it only for offline recovery, then revoke the key in the ByreCC console.\n"
        );
    }
}

fn revoke_installation(
    installation_id: &str,
    installation: &LocalInstallation,
    endpoints: &Endpoints,
    local_only: bool,
) -> Result<()> {
    if local_only {
        return Ok(());
    }
    let api_key = credentials::load(installation_id, installation.credential_storage)
        .context("load credential required for server revocation")?;
    ApiClient::new(endpoints)?
        .revoke(installation_id, &api_key)
        .context("revoke server installation")
}

fn remove_managed_binary() -> Result<()> {
    let executable = std::env::current_exe().context("resolve current byrectl executable")?;
    let expected = state::home_dir()?.join(".local/bin/byrectl");
    if executable != expected {
        println!(
            "  CLI kept: running executable {} is outside the managed path {}.",
            executable.display(),
            expected.display()
        );
        return Ok(());
    }
    fs::remove_file(&expected)
        .with_context(|| format!("remove managed CLI {}", expected.display()))?;
    println!("  CLI removed: {}", expected.display());
    Ok(())
}

fn select_clients(
    requested: &[String],
    _all_clients: bool,
    existing: Option<&[String]>,
) -> Result<Vec<String>> {
    let selected = if !requested.is_empty() {
        clients::validate_ids(requested)?
    } else if let Some(existing) = existing {
        clients::validate_ids(existing)?
    } else {
        clients::detect()?
    };
    if selected.is_empty() {
        bail!(
            "no supported client was detected; pass --clients with one or more of: {}",
            clients::SUPPORTED.join(", ")
        );
    }
    Ok(selected)
}

fn run_plan(plan: Plan, endpoints: &Endpoints) -> Result<()> {
    println!("\nByreCC installation plan\n");
    println!("  Clients: {}", plan.clients.join(", "));
    println!(
        "  Skill: {}",
        if plan.skip_skill {
            "unchanged"
        } else {
            "install/update"
        }
    );
    println!(
        "  MCP config: {}",
        if plan.skip_mcp {
            "unchanged"
        } else if plan.direct {
            "write remote URL and plaintext Bearer key (DIRECT MODE)"
        } else {
            "write local byrectl proxy command (no key in Agent config)"
        }
    );
    println!(
        "  Credential: {} (plaintext, file mode 0600)",
        state::credentials_path()?.display()
    );
    println!("  API: {}", endpoints.api_base);
    println!("  No sudo, system packages, or shell startup files will be modified.\n");
    if plan.direct {
        println!(
            "  WARNING: --direct stores the API key in every selected client config.\n\
             The default proxy mode is safer."
        );
    }
    if plan.dry_run {
        println!("Dry run complete; no login, key creation, or file write occurred.");
        return Ok(());
    }
    if !plan.yes && !confirm_from_tty("Continue? [y/N] ")? {
        bail!("installation cancelled")
    }

    let api = ApiClient::new(endpoints)?;
    let device = api.create_device_code()?;
    println!("\nDevice login");
    println!("  Code: {}", device.user_code);
    println!("  Open: {}", device.verification_uri_complete);
    if open_browser(&device.verification_uri_complete) {
        println!("  Browser opened. Waiting for authorization ...");
    } else {
        println!(
            "  Browser could not be opened automatically; visit {} and enter {}.",
            device.verification_uri, device.user_code
        );
    }

    let installation_token = wait_for_authorization(&api, &device)?;
    let mut local = state::load()?;
    let activation = api.activate(
        &installation_token,
        &ActivateRequest {
            device_id: &local.device_id,
            clients: &plan.clients,
            cli_version: env!("CARGO_PKG_VERSION"),
            skill_version: (!plan.skip_skill).then_some(SKILL_VERSION.trim()),
        },
    )?;
    let api_key = Zeroizing::new(activation.api_key);
    println!("  Authorized. Installation: {}", activation.installation_id);
    println!(
        "  Permissions: scopes [{}], platforms [{}]",
        activation.scopes.join(", "),
        activation.platforms.join(", ")
    );
    println!(
        "  Activation key expires at {} unless setup completes.",
        activation.expires_at
    );

    let executable = std::env::current_exe().context("resolve current byrectl executable")?;
    if !plan.skip_skill {
        let path = clients::install_skill()?;
        println!("  Skill installed: {}", path.display());
    }
    let mut config_changes = Vec::new();
    if !plan.skip_mcp {
        for client in &plan.clients {
            let mode = if plan.direct {
                McpMode::Direct { api_key: &api_key }
            } else {
                McpMode::Proxy {
                    executable: &executable,
                    installation_id: &activation.installation_id,
                }
            };
            match clients::configure(client, &mode, endpoints) {
                Ok(change) => {
                    println!("  {client} configured: {}", change.path.display());
                    config_changes.push(change);
                }
                Err(error) => {
                    rollback_configs(&config_changes);
                    return Err(error).with_context(|| format!("configure {client}"));
                }
            }
        }
    }

    if let Err(error) = api.verify_mcp(&api_key) {
        rollback_configs(&config_changes);
        return Err(error).context("verify authenticated MCP connectivity");
    }
    println!("  Authenticated MCP connectivity verified.");

    let storage = match credentials::store(
        &activation.installation_id,
        &activation.api_key_id,
        &api_key,
    ) {
        Ok(storage) => storage,
        Err(error) => {
            rollback_configs(&config_changes);
            return Err(error).context("store installation credential");
        }
    };
    println!(
        "  Credential saved: {} (plaintext, file mode 0600)",
        state::credentials_path()?.display()
    );
    local.active_installation = Some(activation.installation_id.clone());
    local.installations.insert(
        activation.installation_id.clone(),
        LocalInstallation {
            api_key_id: activation.api_key_id,
            credential_storage: storage,
            clients: plan.clients,
            cli_version: env!("CARGO_PKG_VERSION").to_owned(),
            skill_version: (!plan.skip_skill).then(|| SKILL_VERSION.trim().to_owned()),
            api_base: endpoints.api_base.to_owned(),
            mcp_url: endpoints.mcp_url.to_owned(),
        },
    );
    state::save(&local)?;
    api.complete(&activation.installation_id, &api_key)?;

    println!("\nInstallation complete. Restart configured AI clients to apply changes.");
    Ok(())
}

fn rollback_configs(changes: &[clients::ConfigChange]) {
    for change in changes.iter().rev() {
        if let Err(error) = clients::rollback(change) {
            eprintln!(
                "  Warning: failed to restore {}: {error:#}",
                change.path.display()
            );
        }
    }
}

fn wait_for_authorization(
    api: &ApiClient,
    device: &crate::api::DeviceCodeResponse,
) -> Result<Zeroizing<String>> {
    let deadline = Instant::now() + Duration::from_secs(device.expires_in);
    let mut interval = Duration::from_secs(device.interval.max(1));
    loop {
        if Instant::now() >= deadline {
            bail!("device authorization expired")
        }
        thread::sleep(interval);
        match api.poll_device_token(&device.device_code)? {
            DevicePoll::Pending => {}
            DevicePoll::SlowDown => interval += Duration::from_secs(5),
            DevicePoll::Denied => bail!("device authorization was denied"),
            DevicePoll::Expired => bail!("device authorization expired"),
            DevicePoll::Authorized(response) => {
                return Ok(Zeroizing::new(response.installation_token));
            }
        }
    }
}

fn open_browser(url: &str) -> bool {
    #[cfg(target_os = "macos")]
    let program = "open";
    #[cfg(target_os = "linux")]
    let program = "xdg-open";
    Command::new(program)
        .arg(url)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .is_ok()
}

fn confirm_from_tty(prompt: &str) -> Result<bool> {
    print!("{prompt}");
    io::stdout().flush().context("flush confirmation prompt")?;
    let tty_path = PathBuf::from("/dev/tty");
    let tty = OpenOptions::new()
        .read(true)
        .open(&tty_path)
        .context("interactive terminal unavailable; rerun with --yes after reviewing the plan")?;
    let mut answer = String::new();
    io::BufReader::new(tty)
        .read_line(&mut answer)
        .context("read confirmation")?;
    Ok(matches!(
        answer.trim().to_ascii_lowercase().as_str(),
        "y" | "yes"
    ))
}
