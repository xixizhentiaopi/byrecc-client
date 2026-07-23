#!/bin/sh

set -eu
umask 077

repo_root=$(CDPATH= cd -- "$(dirname "$0")/.." && pwd)
temporary=$(mktemp -d "${TMPDIR:-/tmp}/byrecc-release-key-test.XXXXXX")
trap 'rm -rf "$temporary"' EXIT HUP INT TERM

openssl genpkey \
    -algorithm RSA \
    -pkeyopt rsa_keygen_bits:2048 \
    -out "$temporary/private.pem" >/dev/null 2>&1
openssl pkey \
    -in "$temporary/private.pem" \
    -pubout \
    -out "$temporary/public.pem" >/dev/null 2>&1
openssl genpkey \
    -algorithm RSA \
    -pkeyopt rsa_keygen_bits:2048 \
    -out "$temporary/wrong-private.pem" >/dev/null 2>&1
openssl pkey \
    -in "$temporary/wrong-private.pem" \
    -pubout \
    -out "$temporary/wrong-public.pem" >/dev/null 2>&1

SIGNING_KEY=$(cat "$temporary/private.pem")
PUBLIC_KEY=$(cat "$temporary/public.pem")
export SIGNING_KEY PUBLIC_KEY
sh "$repo_root/scripts/verify-release-keypair.sh"

PUBLIC_KEY=$(cat "$temporary/wrong-public.pem")
export PUBLIC_KEY
if sh "$repo_root/scripts/verify-release-keypair.sh" > "$temporary/mismatch.out" 2>&1; then
    printf '%s\n' "mismatched release key pair unexpectedly passed" >&2
    exit 1
fi
grep -q "does not match" "$temporary/mismatch.out"

printf '%s\n' "release key validation tests passed"
