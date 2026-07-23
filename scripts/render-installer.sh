#!/bin/sh

set -eu
umask 077

byrecc_repo_dir="$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)"
byrecc_public_key="${1:-}"
byrecc_output="${2:-${byrecc_repo_dir}/dist/install.sh}"

if [ -z "$byrecc_public_key" ] || [ ! -f "$byrecc_public_key" ]; then
    printf '%s\n' "Usage: scripts/render-installer.sh RELEASE_PUBLIC_KEY_PEM [OUTPUT]" >&2
    exit 2
fi

if grep -q "'" "$byrecc_public_key"; then
    printf '%s\n' "Public key unexpectedly contains a single quote" >&2
    exit 1
fi

mkdir -p "$(dirname -- "$byrecc_output")"
awk -v key_file="$byrecc_public_key" '
    /^BYRECC_RELEASE_PUBLIC_KEY_PEM="__BYRECC_RELEASE_PUBLIC_KEY_PEM__"$/ {
        print "BYRECC_RELEASE_PUBLIC_KEY_PEM=\047"
        while ((getline line < key_file) > 0) print line
        close(key_file)
        print "\047"
        next
    }
    { print }
' "${byrecc_repo_dir}/install.sh" > "$byrecc_output"
chmod 0755 "$byrecc_output"

if grep -q '^BYRECC_RELEASE_PUBLIC_KEY_PEM="__BYRECC_RELEASE_PUBLIC_KEY_PEM__"$' "$byrecc_output"; then
    printf '%s\n' "Installer key marker was not replaced" >&2
    exit 1
fi

sh -n "$byrecc_output"
printf '%s\n' "Rendered signed-release installer: $byrecc_output"
