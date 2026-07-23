use std::collections::BTreeMap;
use std::env;
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail};
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct LocalState {
    pub version: u8,
    pub device_id: String,
    pub active_installation: Option<String>,
    pub installations: BTreeMap<String, LocalInstallation>,
}

impl Default for LocalState {
    fn default() -> Self {
        Self {
            version: 1,
            device_id: uuid::Uuid::new_v4().to_string(),
            active_installation: None,
            installations: BTreeMap::new(),
        }
    }
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct LocalInstallation {
    pub api_key_id: String,
    pub credential_storage: CredentialStorage,
    pub clients: Vec<String>,
    #[serde(default = "default_true")]
    pub mcp_configured: bool,
    pub cli_version: String,
    pub skill_version: Option<String>,
    pub api_base: String,
    pub mcp_url: String,
}

fn default_true() -> bool {
    true
}

#[derive(Clone, Copy, Debug, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum CredentialStorage {
    File,
}

pub fn home_dir() -> Result<PathBuf> {
    env::var_os("HOME")
        .filter(|value| !value.is_empty())
        .map(PathBuf::from)
        .context("HOME is not set; unable to select a user-level install location")
}

pub fn config_dir() -> Result<PathBuf> {
    if let Some(value) = env::var_os("XDG_CONFIG_HOME").filter(|value| !value.is_empty()) {
        return Ok(PathBuf::from(value).join("byrecc"));
    }
    Ok(home_dir()?.join(".config/byrecc"))
}

pub fn state_path() -> Result<PathBuf> {
    Ok(config_dir()?.join("config.json"))
}

pub fn credentials_path() -> Result<PathBuf> {
    Ok(config_dir()?.join("credentials.json"))
}

pub fn load() -> Result<LocalState> {
    let path = state_path()?;
    if !path.exists() {
        return Ok(LocalState::default());
    }
    let content = fs::read_to_string(&path)
        .with_context(|| format!("read local state {}", path.display()))?;
    serde_json::from_str(&content).with_context(|| format!("parse local state {}", path.display()))
}

pub fn save(state: &LocalState) -> Result<()> {
    let content = serde_json::to_vec_pretty(state).context("serialize local state")?;
    write_secret_file(&state_path()?, &content)
}

pub fn write_secret_file(path: &Path, content: &[u8]) -> Result<()> {
    let parent = path
        .parent()
        .with_context(|| format!("path has no parent: {}", path.display()))?;
    fs::create_dir_all(parent).with_context(|| format!("create {}", parent.display()))?;
    let temporary = parent.join(format!(
        ".{}-{}.tmp",
        path.file_name()
            .and_then(|name| name.to_str())
            .unwrap_or("byrecc"),
        uuid::Uuid::new_v4()
    ));
    let mut options = OpenOptions::new();
    options.create_new(true).write(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    let mut file = options
        .open(&temporary)
        .with_context(|| format!("create {}", temporary.display()))?;
    file.write_all(content)
        .with_context(|| format!("write {}", temporary.display()))?;
    file.sync_all()
        .with_context(|| format!("sync {}", temporary.display()))?;
    drop(file);
    fs::rename(&temporary, path)
        .with_context(|| format!("replace {} atomically", path.display()))?;
    enforce_private_permissions(path)
}

pub fn enforce_private_permissions(path: &Path) -> Result<()> {
    let metadata = fs::symlink_metadata(path)
        .with_context(|| format!("inspect file type for {}", path.display()))?;
    if metadata.file_type().is_symlink() {
        bail!(
            "refusing to use credential or state symlink {}",
            path.display()
        );
    }
    if !metadata.is_file() {
        bail!("private path is not a regular file: {}", path.display());
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::{MetadataExt, PermissionsExt};
        fs::set_permissions(path, fs::Permissions::from_mode(0o600))
            .with_context(|| format!("set 0600 permissions on {}", path.display()))?;
        let mode = fs::metadata(path)
            .with_context(|| format!("inspect permissions on {}", path.display()))?
            .mode()
            & 0o777;
        if mode != 0o600 {
            bail!(
                "refusing to use {} because its mode is {mode:o}, not 600",
                path.display()
            );
        }
    }
    Ok(())
}

pub fn has_private_permissions(path: &Path) -> Result<bool> {
    let metadata =
        fs::symlink_metadata(path).with_context(|| format!("inspect {}", path.display()))?;
    if metadata.file_type().is_symlink() || !metadata.is_file() {
        return Ok(false);
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        Ok(metadata.permissions().mode() & 0o777 == 0o600)
    }
    #[cfg(not(unix))]
    {
        Ok(true)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn writes_private_state_atomically() {
        let temporary = tempfile::tempdir().expect("tempdir");
        let path = temporary.path().join("state.json");
        write_secret_file(&path, br#"{"version":1}"#).expect("write state");
        assert_eq!(
            fs::read_to_string(&path).expect("read state"),
            r#"{"version":1}"#
        );
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            assert_eq!(
                fs::metadata(path).expect("metadata").permissions().mode() & 0o777,
                0o600
            );
        }
    }

    #[cfg(unix)]
    #[test]
    fn refuses_private_file_symlinks() {
        let temporary = tempfile::tempdir().expect("tempdir");
        let target = temporary.path().join("target.json");
        let link = temporary.path().join("credentials.json");
        fs::write(&target, b"secret").expect("write target");
        std::os::unix::fs::symlink(&target, &link).expect("create symlink");

        assert!(!has_private_permissions(&link).expect("inspect link"));
        assert!(enforce_private_permissions(&link).is_err());
    }
}
