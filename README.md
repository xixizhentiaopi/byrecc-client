# ByreCC Client

Public client-side distribution for [ByreCC](https://byre.cc), a read-only Chinese content gateway for AI agents.

This repository is designed to be separated from the private ByreCC service repository. It follows the useful public distribution pattern demonstrated by [AgentKey](https://github.com/chainbase-labs/Agentkey)—reviewable Skill files, installers, tests, and release automation—while keeping ByreCC credentials out of Agent configuration files by default.

## Current platforms

- RedNote / 小红书: search notes, trends, note details, comments
- Zhihu / 知乎: search, hot topics, articles, answers, comments

All capabilities are public read operations. User profiling, personalized feeds, writes, platform credentials, and arbitrary URL proxying are intentionally excluded.

## Repository layout

```text
byrecc-client/
├── Cargo.toml / Cargo.lock
├── install.sh
├── uninstall.sh
├── scripts/render-installer.sh
├── src/
│   ├── api.rs                  # Device Login and installation API client
│   ├── clients.rs              # Claude/Codex/Cursor config writers
│   ├── credentials.rs          # Dedicated plaintext 0600 credential file
│   ├── install.rs              # install/login/logout/uninstall/status workflow
│   └── proxy.rs                # local stdio ↔ Streamable HTTP MCP proxy
├── skills/byrecc/
│   ├── SKILL.md
│   ├── agents/openai.yaml
│   ├── references/
│   └── version.txt
├── tests/
└── .github/workflows/
```

The Rust CLI currently implements `install`, `login`, `logout`, `uninstall`, `status`, `doctor`, `clients`, and `mcp proxy`. It detects and safely merges configuration for Claude Code, Claude Desktop, Codex, and Cursor. JSON and TOML writes are locked, backed up, atomically replaced, and rolled back when a multi-client setup step fails.

Planned after the macOS/Linux release:

```text
├── install.ps1
├── uninstall.ps1
├── .codex-plugin/
└── .claude-plugin/
```

## Installer status

`install.sh` implements the security boundary in `docs/installation-design.md` from the private service repository:

- no `sudo`
- no system package installation
- pinned native CLI version
- signed checksum verification before extraction
- user-level installation into `~/.local/bin`
- all login, credential storage, backup, and MCP configuration delegated to `byrectl`

The signed CLI embeds the versioned Skill. In proxy mode, Agent configuration contains only the absolute `byrectl` path and installation ID—never the API Key. On macOS and Linux, the Key is stored in `~/.config/byrecc/credentials.json` as plaintext with enforced `0600` permissions. This avoids Keychain/Secret Service permission prompts while keeping the credential out of AI client configuration.

Logout revokes the active installation before removing its local credential:

```bash
byrectl logout
```

The public uninstaller delegates to the already installed, signed CLI. It revokes the server credential by default and removes only the `byrecc` entries and known ByreCC Skill files:

```bash
curl -fsSL https://byre.cc/uninstall.sh | sh
```

For offline recovery, `--local-only` skips server revocation and prints a warning. Unknown Skill files, unrelated client configuration, backups, and binaries outside `~/.local/bin/byrectl` are preserved.

After installation, use the diagnostics commands to inspect client registration and verify the complete authenticated path:

```bash
byrectl clients
byrectl doctor
```

`doctor` checks the local state, exact production/development endpoint, regular-file and `0600` credential permissions, embedded Skill version, recorded client configuration, authenticated server installation state, and MCP initialization.

The checked-in source installer intentionally fails closed until the release pipeline embeds the production release-signing public key. Do not publish it at `https://byre.cc/install.sh` before the CLI, signing key, signed release artifacts, and integration tests exist.

## Public repository

The reviewed public distribution lives at [xixizhentiaopi/byrecc-client](https://github.com/xixizhentiaopi/byrecc-client). It was created without the private service repository's Git history. Future synchronization remains an explicit reviewed export of this directory; `target/`, `.env`, credentials, and service code must remain absent.

This repository is licensed under the [MIT License](LICENSE).

Required release configuration:

- GitHub Actions secret `BYRECC_RELEASE_SIGNING_KEY_PEM`
- GitHub Actions variable `BYRECC_RELEASE_PUBLIC_KEY_PEM`
- DNS/CDN publication of release artifacts under `https://releases.byre.cc/byrectl/v<version>/`
- publication of the rendered `dist/install.sh` at `https://byre.cc/install.sh`

See [RELEASING.md](RELEASING.md) for the remaining human decisions and release gates.

## Public/private boundary

Keep public here:

- Skill instructions and tool contract snapshots
- installer and uninstaller source
- `byrectl` source and client configuration writers
- public plugin manifests
- release signing public key, checksums, CI, tests, security policy

Keep private in the service repository:

- Provider implementations and platform sessions
- anti-abuse and risk-control logic
- billing internals and operational configuration
- production infrastructure, secrets, private runbooks, account pools

The public tool reference must be regenerated or checked whenever the server MCP contract changes.

## Development checks

```bash
sh -n install.sh
sh -n uninstall.sh
sh install.sh --installer-help
cargo fmt --check
cargo clippy --all-targets -- -D warnings
cargo test --locked
sh tests/diagnostics_test.sh
sh tests/install_test.sh
sh tests/uninstall_test.sh
sh tests/render_installer_test.sh
sh tests/skill_test.sh
```

Safe local smoke test (does not log in or write files):

```bash
cargo run -- install --dry-run --clients codex,cursor
```
