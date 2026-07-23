use std::collections::BTreeMap;
use std::fs;
#[cfg(target_os = "linux")]
use std::io::Write;
#[cfg(target_os = "linux")]
use std::process::{Command, Stdio};

use anyhow::{Context, Result, bail};
use serde::{Deserialize, Serialize};
use zeroize::Zeroizing;

use crate::state::{self, CredentialStorage};

const SERVICE: &str = "byrecc";

#[derive(Default, Deserialize, Serialize)]
struct CredentialFile {
    version: u8,
    credentials: BTreeMap<String, FileEntry>,
}

#[derive(Deserialize, Serialize)]
struct FileEntry {
    api_key: String,
    api_key_id: String,
}

pub fn store(installation_id: &str, api_key_id: &str, api_key: &str) -> Result<CredentialStorage> {
    #[cfg(target_os = "macos")]
    {
        match security_framework::passwords::set_generic_password(
            SERVICE,
            &account(installation_id),
            api_key.as_bytes(),
        ) {
            Ok(()) => return Ok(CredentialStorage::Keychain),
            Err(error) => eprintln!(
                "  Warning: macOS Keychain was unavailable ({error}); using a private file."
            ),
        }
    }

    #[cfg(target_os = "linux")]
    if command_exists("secret-tool") && store_secret_service(installation_id, api_key).is_ok() {
        return Ok(CredentialStorage::SecretService);
    }

    store_file(installation_id, api_key_id, api_key)?;
    Ok(CredentialStorage::File)
}

pub fn load(installation_id: &str, storage: CredentialStorage) -> Result<Zeroizing<String>> {
    match storage {
        CredentialStorage::Keychain => load_keychain(installation_id),
        CredentialStorage::SecretService => load_secret_service(installation_id),
        CredentialStorage::File => load_file(installation_id),
    }
}

#[cfg(target_os = "macos")]
fn load_keychain(installation_id: &str) -> Result<Zeroizing<String>> {
    let bytes =
        security_framework::passwords::get_generic_password(SERVICE, &account(installation_id))
            .context("read API key from macOS Keychain")?;
    String::from_utf8(bytes)
        .map(Zeroizing::new)
        .context("macOS Keychain entry is not valid UTF-8")
}

#[cfg(not(target_os = "macos"))]
fn load_keychain(_installation_id: &str) -> Result<Zeroizing<String>> {
    bail!("this installation references macOS Keychain on a non-macOS host")
}

#[cfg(target_os = "linux")]
fn store_secret_service(installation_id: &str, api_key: &str) -> Result<()> {
    let mut child = Command::new("secret-tool")
        .args([
            "store",
            "--label=ByreCC API key",
            "service",
            SERVICE,
            "account",
            &account(installation_id),
        ])
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .context("start Secret Service client")?;
    child
        .stdin
        .take()
        .context("open Secret Service stdin")?
        .write_all(api_key.as_bytes())
        .context("send API key to Secret Service")?;
    let status = child.wait().context("wait for Secret Service")?;
    if !status.success() {
        bail!("Secret Service rejected the credential")
    }
    Ok(())
}

#[cfg(target_os = "linux")]
fn load_secret_service(installation_id: &str) -> Result<Zeroizing<String>> {
    let output = Command::new("secret-tool")
        .args([
            "lookup",
            "service",
            SERVICE,
            "account",
            &account(installation_id),
        ])
        .output()
        .context("read API key from Secret Service")?;
    if !output.status.success() {
        bail!("Secret Service does not contain this installation")
    }
    let value = String::from_utf8(output.stdout).context("Secret Service value is not UTF-8")?;
    Ok(Zeroizing::new(value.trim_end().to_owned()))
}

#[cfg(not(target_os = "linux"))]
fn load_secret_service(_installation_id: &str) -> Result<Zeroizing<String>> {
    bail!("this installation references Linux Secret Service on a non-Linux host")
}

fn store_file(installation_id: &str, api_key_id: &str, api_key: &str) -> Result<()> {
    let path = state::credentials_path()?;
    let mut file = if path.exists() {
        let content =
            fs::read_to_string(&path).with_context(|| format!("read {}", path.display()))?;
        serde_json::from_str::<CredentialFile>(&content)
            .with_context(|| format!("parse {}", path.display()))?
    } else {
        CredentialFile {
            version: 1,
            credentials: BTreeMap::new(),
        }
    };
    file.version = 1;
    file.credentials.insert(
        installation_id.to_owned(),
        FileEntry {
            api_key: api_key.to_owned(),
            api_key_id: api_key_id.to_owned(),
        },
    );
    let content = serde_json::to_vec_pretty(&file).context("serialize credential file")?;
    state::write_secret_file(&path, &content)
}

fn load_file(installation_id: &str) -> Result<Zeroizing<String>> {
    let path = state::credentials_path()?;
    state::enforce_private_permissions(&path)?;
    let content = fs::read_to_string(&path)
        .with_context(|| format!("read credentials {}", path.display()))?;
    let file: CredentialFile = serde_json::from_str(&content)
        .with_context(|| format!("parse credentials {}", path.display()))?;
    let entry = file
        .credentials
        .get(installation_id)
        .with_context(|| format!("no credential for installation {installation_id}"))?;
    Ok(Zeroizing::new(entry.api_key.clone()))
}

fn account(installation_id: &str) -> String {
    format!("installation:{installation_id}")
}

#[cfg(target_os = "linux")]
fn command_exists(name: &str) -> bool {
    std::env::var_os("PATH")
        .map(|paths| std::env::split_paths(&paths).any(|path| path.join(name).is_file()))
        .unwrap_or(false)
}
