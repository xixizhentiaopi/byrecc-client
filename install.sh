#!/bin/sh

# ByreCC installer bootstrap for macOS and Linux.
# It downloads one pinned byrectl release, verifies its signed checksum,
# installs it without sudo, and delegates all client configuration to byrectl.

set -eu
umask 077

BYRECC_PRODUCT="ByreCC"
BYRECC_CLI_VERSION="${BYRECC_CLI_VERSION:-0.1.1}"
BYRECC_RELEASE_BASE="${BYRECC_RELEASE_BASE:-https://releases.byre.cc/byrectl}"
BYRECC_INSTALL_DIR="${BYRECC_INSTALL_DIR:-${HOME}/.local/bin}"

# Replaced by the release pipeline. Keeping this fail-closed marker prevents
# an unsigned development installer from being presented as production-ready.
BYRECC_RELEASE_PUBLIC_KEY_PEM="__BYRECC_RELEASE_PUBLIC_KEY_PEM__"

byrecc_info() {
    printf '%s\n' "  $*"
}

byrecc_die() {
    printf '%s\n' "  Error: $*" >&2
    exit 1
}

byrecc_help() {
    cat <<'EOF'
ByreCC installer bootstrap

Usage:
  curl -fsSL https://byre.cc/install.sh | sh
  curl -fsSL https://byre.cc/install.sh | sh -s -- [BYRECTL INSTALL OPTIONS]

Bootstrap environment:
  BYRECC_CLI_VERSION   Pinned byrectl version (default: 0.1.1)
  BYRECC_INSTALL_DIR   Binary directory (default: ~/.local/bin)
  BYRECC_RELEASE_BASE  Release origin (maintainer/testing use)

Installer options are passed unchanged to `byrectl install`.
Use `--installer-help` to print this message without downloading anything.

Security: No sudo or system package manager is used.
EOF
}

if [ "${1:-}" = "--installer-help" ]; then
    byrecc_help
    exit 0
fi

byrecc_need_command() {
    command -v "$1" >/dev/null 2>&1 || byrecc_die "required command not found: $1"
}

byrecc_fetch() {
    byrecc_source_url="$1"
    byrecc_destination="$2"
    curl --proto '=https' --tlsv1.2 --fail --silent --show-error --location \
        "$byrecc_source_url" --output "$byrecc_destination"
}

byrecc_sha256() {
    if command -v shasum >/dev/null 2>&1; then
        shasum -a 256 "$1" | awk '{print $1}'
    elif command -v sha256sum >/dev/null 2>&1; then
        sha256sum "$1" | awk '{print $1}'
    else
        byrecc_die "neither shasum nor sha256sum is available"
    fi
}

byrecc_need_command curl
byrecc_need_command awk
byrecc_need_command openssl
byrecc_need_command tar
byrecc_need_command mktemp

case "$(uname -s)" in
    Darwin) byrecc_os="darwin" ;;
    Linux) byrecc_os="linux" ;;
    *) byrecc_die "unsupported operating system; use macOS or Linux" ;;
esac

case "$(uname -m)" in
    arm64|aarch64) byrecc_arch="arm64" ;;
    x86_64|amd64) byrecc_arch="amd64" ;;
    *) byrecc_die "unsupported CPU architecture: $(uname -m)" ;;
esac

byrecc_asset="byrectl-${byrecc_os}-${byrecc_arch}.tar.gz"
byrecc_release_url="${BYRECC_RELEASE_BASE}/v${BYRECC_CLI_VERSION}"
byrecc_temp_dir="$(mktemp -d "${TMPDIR:-/tmp}/byrecc-install.XXXXXX")"
byrecc_cli_path=""
byrecc_previous_path=""
byrecc_had_previous="false"
byrecc_install_committed="false"

byrecc_cleanup() {
    if [ "$byrecc_install_committed" != "true" ] && [ -n "$byrecc_cli_path" ]; then
        if [ "$byrecc_had_previous" = "true" ] && [ -e "$byrecc_previous_path" ]; then
            rm -f "$byrecc_cli_path"
            mv "$byrecc_previous_path" "$byrecc_cli_path"
        elif [ -e "$byrecc_cli_path" ]; then
            rm -f "$byrecc_cli_path"
        fi
    elif [ -n "$byrecc_previous_path" ] && [ -e "$byrecc_previous_path" ]; then
        rm -f "$byrecc_previous_path"
    fi
    rm -rf "$byrecc_temp_dir"
}
trap byrecc_cleanup EXIT HUP INT TERM

byrecc_archive="${byrecc_temp_dir}/${byrecc_asset}"
byrecc_checksums="${byrecc_temp_dir}/checksums.txt"
byrecc_signature="${byrecc_temp_dir}/checksums.sig"
byrecc_public_key="${byrecc_temp_dir}/release-public.pem"

printf '\n%s installer\n\n' "$BYRECC_PRODUCT"
byrecc_info "Platform: ${byrecc_os}/${byrecc_arch}"
byrecc_info "CLI version: ${BYRECC_CLI_VERSION}"
byrecc_info "Install directory: ${BYRECC_INSTALL_DIR}"
byrecc_info "No sudo or system package manager will be used."

if [ "$BYRECC_RELEASE_PUBLIC_KEY_PEM" = "__BYRECC_RELEASE_PUBLIC_KEY_PEM__" ]; then
    byrecc_die "release signing key is not embedded; this source installer is not publishable yet"
fi

byrecc_info "Downloading signed release metadata..."
byrecc_fetch "${byrecc_release_url}/checksums.txt" "$byrecc_checksums"
byrecc_fetch "${byrecc_release_url}/checksums.sig" "$byrecc_signature"
printf '%s\n' "$BYRECC_RELEASE_PUBLIC_KEY_PEM" > "$byrecc_public_key"

if ! openssl dgst -sha256 -verify "$byrecc_public_key" \
    -signature "$byrecc_signature" "$byrecc_checksums" >/dev/null 2>&1; then
    byrecc_die "release signature verification failed"
fi

byrecc_expected_hash="$(awk -v asset="$byrecc_asset" '$2 == asset {print $1}' "$byrecc_checksums")"
[ "${#byrecc_expected_hash}" -eq 64 ] || \
    byrecc_die "release checksum does not contain one valid entry for ${byrecc_asset}"
case "$byrecc_expected_hash" in
    *[!0-9a-fA-F]*) byrecc_die "release checksum for ${byrecc_asset} is invalid" ;;
esac

byrecc_info "Downloading ${byrecc_asset}..."
byrecc_fetch "${byrecc_release_url}/${byrecc_asset}" "$byrecc_archive"
byrecc_actual_hash="$(byrecc_sha256 "$byrecc_archive")"

if [ "$byrecc_actual_hash" != "$byrecc_expected_hash" ]; then
    byrecc_die "release checksum verification failed"
fi

mkdir -p "${byrecc_temp_dir}/extract"
byrecc_archive_members="$(tar -tzf "$byrecc_archive")"
[ "$byrecc_archive_members" = "byrectl" ] || \
    byrecc_die "release archive must contain exactly one top-level byrectl file"
tar -xzf "$byrecc_archive" -C "${byrecc_temp_dir}/extract"
byrecc_downloaded_cli="${byrecc_temp_dir}/extract/byrectl"
[ -f "$byrecc_downloaded_cli" ] || byrecc_die "release archive does not contain byrectl"
[ ! -L "$byrecc_downloaded_cli" ] || byrecc_die "release archive contains an invalid byrectl symlink"

mkdir -p "$BYRECC_INSTALL_DIR"
byrecc_cli_path="${BYRECC_INSTALL_DIR}/byrectl"
byrecc_staged_path="${BYRECC_INSTALL_DIR}/.byrectl-install-$$"
[ ! -e "$byrecc_staged_path" ] || byrecc_die "staged install path already exists"
cp "$byrecc_downloaded_cli" "$byrecc_staged_path"
chmod 0755 "$byrecc_staged_path"
byrecc_previous_path="${BYRECC_INSTALL_DIR}/.byrectl-previous-$$"
byrecc_had_previous="false"
if [ -e "$byrecc_cli_path" ]; then
    [ ! -L "$byrecc_cli_path" ] || byrecc_die "refusing to replace a byrectl symlink"
    mv "$byrecc_cli_path" "$byrecc_previous_path"
    byrecc_had_previous="true"
fi
mv "$byrecc_staged_path" "$byrecc_cli_path"

byrecc_info "Installed ${byrecc_cli_path}"
byrecc_info "Starting secure device login and client configuration..."
if ! "$byrecc_cli_path" install "$@"; then
    byrecc_die "byrectl setup failed; the CLI change will be rolled back"
fi
byrecc_install_committed="true"
