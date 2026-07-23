use std::collections::BTreeMap;
use std::fs;

use anyhow::{Context, Result, bail};
use serde::{Deserialize, Serialize};
use zeroize::Zeroizing;

use crate::state::{self, CredentialStorage};

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
    store_file(installation_id, api_key_id, api_key)?;
    Ok(CredentialStorage::File)
}

pub fn load(installation_id: &str, storage: CredentialStorage) -> Result<Zeroizing<String>> {
    match storage {
        CredentialStorage::File => load_file(installation_id),
    }
}

pub fn delete(installation_id: &str, storage: CredentialStorage) -> Result<()> {
    match storage {
        CredentialStorage::File => delete_file(installation_id),
    }
}

fn store_file(installation_id: &str, api_key_id: &str, api_key: &str) -> Result<()> {
    let path = state::credentials_path()?;
    store_file_at(&path, installation_id, api_key_id, api_key)
}

fn store_file_at(
    path: &std::path::Path,
    installation_id: &str,
    api_key_id: &str,
    api_key: &str,
) -> Result<()> {
    let mut file = if path.exists() {
        let content =
            fs::read_to_string(path).with_context(|| format!("read {}", path.display()))?;
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
    state::write_secret_file(path, &content)
}

fn load_file(installation_id: &str) -> Result<Zeroizing<String>> {
    let path = state::credentials_path()?;
    load_file_at(&path, installation_id)
}

fn load_file_at(path: &std::path::Path, installation_id: &str) -> Result<Zeroizing<String>> {
    state::enforce_private_permissions(path)?;
    let content =
        fs::read_to_string(path).with_context(|| format!("read credentials {}", path.display()))?;
    let file: CredentialFile = serde_json::from_str(&content)
        .with_context(|| format!("parse credentials {}", path.display()))?;
    let entry = file
        .credentials
        .get(installation_id)
        .with_context(|| format!("no credential for installation {installation_id}"))?;
    Ok(Zeroizing::new(entry.api_key.clone()))
}

fn delete_file(installation_id: &str) -> Result<()> {
    let path = state::credentials_path()?;
    delete_file_at(&path, installation_id)
}

fn delete_file_at(path: &std::path::Path, installation_id: &str) -> Result<()> {
    state::enforce_private_permissions(path)?;
    let content =
        fs::read_to_string(path).with_context(|| format!("read credentials {}", path.display()))?;
    let mut file: CredentialFile = serde_json::from_str(&content)
        .with_context(|| format!("parse credentials {}", path.display()))?;
    if file.credentials.remove(installation_id).is_none() {
        bail!("credential file does not contain installation {installation_id}")
    }
    if file.credentials.is_empty() {
        fs::remove_file(path)
            .with_context(|| format!("remove empty credential file {}", path.display()))?;
        return Ok(());
    }
    let content = serde_json::to_vec_pretty(&file).context("serialize credential file")?;
    state::write_secret_file(path, &content)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn plaintext_file_is_private_and_removed_when_empty() {
        let temporary = tempfile::tempdir().expect("tempdir");
        let path = temporary.path().join("credentials.json");
        let api_key = "byre_test_example_plaintext_key";

        store_file_at(&path, "ins_test", "key_test", api_key).expect("store credential");
        let content = fs::read_to_string(&path).expect("read credential file");
        assert!(content.contains(api_key));
        assert_eq!(
            load_file_at(&path, "ins_test")
                .expect("load credential")
                .as_str(),
            api_key
        );
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            assert_eq!(
                fs::metadata(&path).expect("metadata").permissions().mode() & 0o777,
                0o600
            );
        }

        delete_file_at(&path, "ins_test").expect("delete credential");
        assert!(!path.exists());
    }
}
