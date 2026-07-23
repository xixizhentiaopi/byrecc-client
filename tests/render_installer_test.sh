#!/bin/sh

set -eu
umask 077

byrecc_repo_dir="$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)"
byrecc_temp_dir="$(mktemp -d "${TMPDIR:-/tmp}/byrecc-render-test.XXXXXX")"

byrecc_cleanup() {
    rm -rf "$byrecc_temp_dir"
}
trap byrecc_cleanup EXIT HUP INT TERM

openssl genpkey -algorithm RSA -pkeyopt rsa_keygen_bits:2048 \
    -out "${byrecc_temp_dir}/private.pem" >/dev/null 2>&1
openssl pkey -in "${byrecc_temp_dir}/private.pem" -pubout \
    -out "${byrecc_temp_dir}/public.pem" >/dev/null 2>&1

"${byrecc_repo_dir}/scripts/render-installer.sh" \
    "${byrecc_temp_dir}/public.pem" \
    "${byrecc_temp_dir}/install.sh" >/dev/null

sh -n "${byrecc_temp_dir}/install.sh"
if grep -q '^BYRECC_RELEASE_PUBLIC_KEY_PEM="__BYRECC_RELEASE_PUBLIC_KEY_PEM__"$' \
    "${byrecc_temp_dir}/install.sh"; then
    printf '%s\n' "rendered installer still contains the public-key marker" >&2
    exit 1
fi

printf '%s\n' "installer rendering checks passed"
