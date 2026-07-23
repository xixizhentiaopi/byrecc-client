use std::env;
use std::fs::{self, File, OpenOptions};
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use anyhow::{Context, Result, bail};
use fs2::FileExt;
use serde_json::{Map, Value, json};
use toml_edit::{Array, DocumentMut, InlineTable, Item, Table, Value as TomlValue, value};

use crate::api::Endpoints;
use crate::state;

pub const SUPPORTED: [&str; 4] = ["claude-code", "claude-desktop", "codex", "cursor"];

pub enum McpMode<'a> {
    Proxy {
        executable: &'a Path,
        installation_id: &'a str,
    },
    Direct {
        api_key: &'a str,
    },
}

pub struct ConfigChange {
    pub path: PathBuf,
    backup: Option<PathBuf>,
}

pub fn validate_ids(ids: &[String]) -> Result<Vec<String>> {
    let mut result = Vec::new();
    for id in ids {
        if !SUPPORTED.contains(&id.as_str()) {
            bail!(
                "unsupported client {id}; supported clients: {}",
                SUPPORTED.join(", ")
            );
        }
        if !result.contains(id) {
            result.push(id.clone());
        }
    }
    result.sort();
    Ok(result)
}

pub fn detect() -> Result<Vec<String>> {
    let home = state::home_dir()?;
    let mut found = Vec::new();
    if home.join(".claude.json").exists() || command_exists("claude") {
        found.push("claude-code".to_owned());
    }
    if claude_desktop_path(&home).exists()
        || claude_desktop_path(&home)
            .parent()
            .is_some_and(Path::exists)
    {
        found.push("claude-desktop".to_owned());
    }
    if home.join(".codex/config.toml").exists() || command_exists("codex") {
        found.push("codex".to_owned());
    }
    if home.join(".cursor/mcp.json").exists() || command_exists("cursor") {
        found.push("cursor".to_owned());
    }
    Ok(found)
}

pub fn configure(client: &str, mode: &McpMode<'_>, endpoints: &Endpoints) -> Result<ConfigChange> {
    let home = state::home_dir()?;
    let path = match client {
        "claude-code" => home.join(".claude.json"),
        "claude-desktop" => claude_desktop_path(&home),
        "cursor" => home.join(".cursor/mcp.json"),
        "codex" => home.join(".codex/config.toml"),
        _ => bail!("unsupported client {client}"),
    };
    let _lock = ConfigLock::acquire(client)?;
    let backup = backup(client, &path)?;
    let result = if client == "codex" {
        write_codex(&path, mode, endpoints)
    } else {
        write_json_client(&path, mode, endpoints)
    };
    if let Err(error) = result {
        restore(&path, backup.as_deref())?;
        return Err(error);
    }
    Ok(ConfigChange { path, backup })
}

pub fn rollback(change: &ConfigChange) -> Result<()> {
    restore(&change.path, change.backup.as_deref())
}

pub fn install_skill() -> Result<PathBuf> {
    let home = state::home_dir()?;
    let universal = home.join(".agents/skills/byrecc");
    write_skill_tree(&universal)?;

    let claude = home.join(".claude/skills/byrecc");
    if fs::symlink_metadata(&claude).is_err() {
        if let Some(parent) = claude.parent() {
            fs::create_dir_all(parent)
                .with_context(|| format!("create skill directory {}", parent.display()))?;
        }
        #[cfg(unix)]
        std::os::unix::fs::symlink(&universal, &claude).with_context(|| {
            format!(
                "link Claude skill {} to {}",
                claude.display(),
                universal.display()
            )
        })?;
    } else if !claude.is_symlink() {
        write_skill_tree(&claude)?;
    }
    Ok(universal)
}

fn write_skill_tree(root: &Path) -> Result<()> {
    write_regular(
        &root.join("SKILL.md"),
        include_bytes!("../skills/byrecc/SKILL.md"),
    )?;
    write_regular(
        &root.join("agents/openai.yaml"),
        include_bytes!("../skills/byrecc/agents/openai.yaml"),
    )?;
    write_regular(
        &root.join("references/tools.md"),
        include_bytes!("../skills/byrecc/references/tools.md"),
    )?;
    write_regular(
        &root.join("references/errors.md"),
        include_bytes!("../skills/byrecc/references/errors.md"),
    )?;
    write_regular(
        &root.join("version.txt"),
        include_bytes!("../skills/byrecc/version.txt"),
    )
}

fn write_json_client(path: &Path, mode: &McpMode<'_>, endpoints: &Endpoints) -> Result<()> {
    let mut document = read_json_object(path)?;
    let servers = document
        .entry("mcpServers")
        .or_insert_with(|| Value::Object(Map::new()))
        .as_object_mut()
        .context("mcpServers exists but is not a JSON object")?;
    let entry = match mode {
        McpMode::Proxy {
            executable,
            installation_id,
        } => json!({
            "command": executable,
            "args": ["mcp", "proxy", "--installation", installation_id]
        }),
        McpMode::Direct { api_key } => json!({
            "type": "http",
            "url": endpoints.mcp_url,
            "headers": {"Authorization": format!("Bearer {api_key}")}
        }),
    };
    servers.insert("byrecc".to_owned(), entry);
    let content = serde_json::to_vec_pretty(&Value::Object(document))
        .context("serialize JSON MCP configuration")?;
    state::write_secret_file(path, &content)
}

fn write_codex(path: &Path, mode: &McpMode<'_>, endpoints: &Endpoints) -> Result<()> {
    let input = if path.exists() {
        fs::read_to_string(path).with_context(|| format!("read {}", path.display()))?
    } else {
        String::new()
    };
    let mut document = input
        .parse::<DocumentMut>()
        .with_context(|| format!("parse TOML configuration {}", path.display()))?;
    if !document.contains_key("mcp_servers") {
        document["mcp_servers"] = Item::Table(Table::new());
    }
    let servers = document["mcp_servers"]
        .as_table_mut()
        .context("mcp_servers exists but is not a TOML table")?;
    let mut byrecc = Table::new();
    match mode {
        McpMode::Proxy {
            executable,
            installation_id,
        } => {
            byrecc["command"] = value(executable.to_string_lossy().as_ref());
            let mut args = Array::new();
            for arg in ["mcp", "proxy", "--installation", installation_id] {
                args.push(arg);
            }
            byrecc["args"] = value(args);
        }
        McpMode::Direct { api_key } => {
            byrecc["url"] = value(endpoints.mcp_url);
            let mut headers = InlineTable::new();
            headers.insert(
                "Authorization",
                TomlValue::from(format!("Bearer {api_key}")),
            );
            byrecc["http_headers"] = value(headers);
        }
    }
    servers["byrecc"] = Item::Table(byrecc);
    state::write_secret_file(path, document.to_string().as_bytes())
}

fn read_json_object(path: &Path) -> Result<Map<String, Value>> {
    if !path.exists() {
        return Ok(Map::new());
    }
    let content = fs::read_to_string(path).with_context(|| format!("read {}", path.display()))?;
    if content.trim().is_empty() {
        return Ok(Map::new());
    }
    serde_json::from_str::<Value>(&content)
        .with_context(|| format!("parse JSON configuration {}", path.display()))?
        .as_object()
        .cloned()
        .context("client configuration root is not a JSON object")
}

fn backup(client: &str, path: &Path) -> Result<Option<PathBuf>> {
    if !path.exists() {
        return Ok(None);
    }
    let timestamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .context("system time is before Unix epoch")?
        .as_secs();
    let file_name = path
        .file_name()
        .and_then(|value| value.to_str())
        .unwrap_or("config");
    let destination = state::config_dir()?
        .join("backups")
        .join(timestamp.to_string())
        .join(client)
        .join(file_name);
    let parent = destination
        .parent()
        .context("backup destination has no parent")?;
    fs::create_dir_all(parent)
        .with_context(|| format!("create backup directory {}", parent.display()))?;
    fs::copy(path, &destination)
        .with_context(|| format!("back up {} to {}", path.display(), destination.display()))?;
    state::enforce_private_permissions(&destination)?;
    Ok(Some(destination))
}

fn restore(path: &Path, backup: Option<&Path>) -> Result<()> {
    if let Some(backup) = backup {
        let content = fs::read(backup)
            .with_context(|| format!("read configuration backup {}", backup.display()))?;
        state::write_secret_file(path, &content)
    } else if path.exists() {
        fs::remove_file(path)
            .with_context(|| format!("remove newly created configuration {}", path.display()))
    } else {
        Ok(())
    }
}

fn write_regular(path: &Path, content: &[u8]) -> Result<()> {
    let parent = path
        .parent()
        .with_context(|| format!("path has no parent: {}", path.display()))?;
    fs::create_dir_all(parent).with_context(|| format!("create {}", parent.display()))?;
    if fs::symlink_metadata(path)
        .map(|metadata| metadata.file_type().is_symlink())
        .unwrap_or(false)
    {
        bail!("refusing to overwrite skill symlink {}", path.display());
    }
    fs::write(path, content).with_context(|| format!("write {}", path.display()))
}

fn claude_desktop_path(home: &Path) -> PathBuf {
    #[cfg(target_os = "macos")]
    return home.join("Library/Application Support/Claude/claude_desktop_config.json");
    #[cfg(not(target_os = "macos"))]
    return home.join(".config/Claude/claude_desktop_config.json");
}

fn command_exists(name: &str) -> bool {
    env::var_os("PATH")
        .map(|paths| env::split_paths(&paths).any(|path| path.join(name).is_file()))
        .unwrap_or(false)
}

struct ConfigLock {
    file: File,
}

impl ConfigLock {
    fn acquire(client: &str) -> Result<Self> {
        let path = state::config_dir()?
            .join("locks")
            .join(format!("{client}.lock"));
        let parent = path.parent().context("lock path has no parent")?;
        fs::create_dir_all(parent)
            .with_context(|| format!("create lock directory {}", parent.display()))?;
        let file = OpenOptions::new()
            .create(true)
            .truncate(false)
            .read(true)
            .write(true)
            .open(&path)
            .with_context(|| format!("open configuration lock {}", path.display()))?;
        file.lock_exclusive()
            .with_context(|| format!("lock client configuration for {client}"))?;
        Ok(Self { file })
    }
}

impl Drop for ConfigLock {
    fn drop(&mut self) {
        let _ = self.file.unlock();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn json_writer_preserves_unrelated_servers() {
        let temporary = tempfile::tempdir().expect("tempdir");
        let path = temporary.path().join("mcp.json");
        fs::write(&path, r#"{"mcpServers":{"existing":{"command":"keep"}}}"#).expect("seed config");
        let executable = Path::new("/tmp/byrectl");
        configure_at_json_for_test(
            &path,
            &McpMode::Proxy {
                executable,
                installation_id: "ins_test",
            },
            &Endpoints::for_mode(false),
        )
        .expect("write config");
        let value: Value = serde_json::from_str(&fs::read_to_string(path).expect("read config"))
            .expect("parse config");
        assert_eq!(value["mcpServers"]["existing"]["command"], "keep");
        assert_eq!(value["mcpServers"]["byrecc"]["args"][3], "ins_test");
    }

    #[test]
    fn codex_writer_preserves_unrelated_tables() {
        let temporary = tempfile::tempdir().expect("tempdir");
        let path = temporary.path().join("config.toml");
        fs::write(&path, "model = \"test\"\n[projects.demo]\ntrusted = true\n")
            .expect("seed config");
        write_codex(
            &path,
            &McpMode::Proxy {
                executable: Path::new("/tmp/byrectl"),
                installation_id: "ins_test",
            },
            &Endpoints::for_mode(false),
        )
        .expect("write config");
        let output = fs::read_to_string(path).expect("read config");
        assert!(output.contains("model = \"test\""));
        assert!(output.contains("[projects.demo]"));
        assert!(output.contains("[mcp_servers.byrecc]"));
    }

    #[test]
    fn rollback_restores_the_exact_previous_file() {
        let temporary = tempfile::tempdir().expect("tempdir");
        let path = temporary.path().join("config.json");
        let backup = temporary.path().join("backup.json");
        fs::write(&path, b"new").expect("write new config");
        fs::write(&backup, b"original").expect("write backup");
        rollback(&ConfigChange {
            path: path.clone(),
            backup: Some(backup),
        })
        .expect("rollback");
        assert_eq!(fs::read(path).expect("read restored config"), b"original");
    }

    fn configure_at_json_for_test(
        path: &Path,
        mode: &McpMode<'_>,
        endpoints: &Endpoints,
    ) -> Result<()> {
        write_json_client(path, mode, endpoints)
    }
}
