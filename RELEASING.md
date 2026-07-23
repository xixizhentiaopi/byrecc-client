# Releasing ByreCC Client

The source, installers, and CI are prepared for a signed macOS/Linux release. A release must not be tagged or promoted until every gate below is complete.

## Human decisions

- Choose and commit the public repository license.
- Generate the release-signing key in an approved offline or managed key system.
- Decide who can approve a production release and rotate the signing key.
- Confirm ownership and TLS configuration for `byre.cc`, `api.byre.cc`, and `releases.byre.cc`.

Never commit the release private key or place it in a build log.

## GitHub configuration

- Store the private PEM as the Actions secret `BYRECC_RELEASE_SIGNING_KEY_PEM`.
- Store the matching public PEM as the Actions variable `BYRECC_RELEASE_PUBLIC_KEY_PEM`.
- Require the `CI` workflow on `main`.
- Enable private vulnerability reporting for the public repository.
- Protect `v*` tags or restrict release creation to approved maintainers.

## Pre-release gates

Run from a clean clone:

```bash
cargo fmt --check
cargo clippy --all-targets -- -D warnings
cargo test --locked
sh tests/diagnostics_test.sh
sh tests/install_test.sh
sh tests/uninstall_test.sh
sh tests/render_installer_test.sh
sh tests/skill_test.sh
```

Before the first production tag:

- Deploy the matching server migration and API version.
- Verify `GET` and `DELETE /v1/installations/{id}` with a test installation.
- Verify Device Login, repeated login, logout, and uninstall on all four release targets.
- Confirm `byrectl doctor` passes against production.
- Confirm the API Key appears only in the `0600` ByreCC credential file in default proxy mode.
- Review the public export for service code, `.env` files, credentials, private keys, and build output.

## Release and promotion

1. Create an annotated `v<version>` tag from a green `main` commit.
2. Let the release workflow build all four native archives and create signed checksums.
3. Verify the GitHub release contains the four archives, `checksums.txt`, `checksums.sig`, `install.sh`, and `uninstall.sh`.
4. Independently verify the checksum signature with the configured public key.
5. Promote immutable artifacts to `https://releases.byre.cc/byrectl/v<version>/`.
6. Publish the rendered installer and uninstaller at `https://byre.cc/install.sh` and `https://byre.cc/uninstall.sh`.
7. Test both one-line commands on clean macOS and Linux users without `sudo`.

Do not overwrite an existing versioned artifact. If validation fails, fix the issue and publish a new version.

## Post-release checks

- Run `byrectl status`, `byrectl clients`, and `byrectl doctor`.
- Restart each configured AI client and execute one read-only ByreCC MCP tool.
- Verify logout revokes the server Key.
- Verify uninstall preserves unrelated MCP entries and unknown Skill files.
- Record artifact hashes, workflow URL, approver, and smoke-test results in the release notes.
