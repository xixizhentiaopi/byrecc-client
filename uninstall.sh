#!/bin/sh

# ByreCC uninstaller bootstrap for macOS and Linux.
# The signed byrectl binary owns the removal policy and server-side revocation.

set -eu
umask 077

BYRECC_INSTALL_DIR="${BYRECC_INSTALL_DIR:-${HOME}/.local/bin}"
byrecc_cli_path="${BYRECC_INSTALL_DIR}/byrectl"

byrecc_die() {
    printf '%s\n' "  Error: $*" >&2
    exit 1
}

byrecc_help() {
    cat <<'EOF'
ByreCC uninstaller bootstrap

Usage:
  curl -fsSL https://byre.cc/uninstall.sh | sh
  curl -fsSL https://byre.cc/uninstall.sh | sh -s -- [BYRECTL UNINSTALL OPTIONS]

Bootstrap environment:
  BYRECC_INSTALL_DIR   Binary directory (default: ~/.local/bin)

Options are passed unchanged to `byrectl uninstall`.
Common options:
  --yes          Skip the interactive confirmation after reviewing its plan
  --local-only   Do not revoke the server credential (offline recovery only)
  --keep-skill   Keep the installed ByreCC Skill
  --keep-binary  Keep byrectl

This script does not download or execute any new code. The installed byrectl
binary removes only ByreCC-owned configuration and preserves unrelated entries.
EOF
}

case "${1:-}" in
    --help|--installer-help)
        byrecc_help
        exit 0
        ;;
esac

[ -f "$byrecc_cli_path" ] || \
    byrecc_die "byrectl was not found at ${byrecc_cli_path}; it may already be removed"
[ ! -L "$byrecc_cli_path" ] || \
    byrecc_die "refusing to execute a byrectl symlink at ${byrecc_cli_path}"
[ -x "$byrecc_cli_path" ] || \
    byrecc_die "byrectl is not executable at ${byrecc_cli_path}"

exec "$byrecc_cli_path" uninstall "$@"
