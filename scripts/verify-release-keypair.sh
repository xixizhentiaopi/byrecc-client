#!/bin/sh

# Validate that the configured release private/public PEM values are a pair.
# The script intentionally prints no key material.

set -eu
umask 077

byrecc_die() {
    printf '%s\n' "Error: $*" >&2
    exit 1
}

[ -n "${SIGNING_KEY:-}" ] || byrecc_die "SIGNING_KEY is empty"
[ -n "${PUBLIC_KEY:-}" ] || byrecc_die "PUBLIC_KEY is empty"

byrecc_key_temp=$(mktemp -d "${TMPDIR:-/tmp}/byrecc-release-key.XXXXXX")
byrecc_cleanup() {
    rm -f \
        "$byrecc_key_temp/private.pem" \
        "$byrecc_key_temp/configured-public.pem" \
        "$byrecc_key_temp/derived-public.pem" \
        "$byrecc_key_temp/configured-public.der" \
        "$byrecc_key_temp/derived-public.der"
    rmdir "$byrecc_key_temp"
}
trap byrecc_cleanup EXIT HUP INT TERM

printf '%s\n' "$SIGNING_KEY" > "$byrecc_key_temp/private.pem"
printf '%s\n' "$PUBLIC_KEY" > "$byrecc_key_temp/configured-public.pem"

openssl pkey \
    -in "$byrecc_key_temp/private.pem" \
    -check -noout >/dev/null 2>&1 ||
    byrecc_die "release signing private key is invalid"
openssl pkey \
    -in "$byrecc_key_temp/private.pem" \
    -pubout \
    -out "$byrecc_key_temp/derived-public.pem" >/dev/null 2>&1 ||
    byrecc_die "unable to derive a public key from the release private key"
openssl pkey \
    -pubin \
    -in "$byrecc_key_temp/configured-public.pem" \
    -outform DER \
    -out "$byrecc_key_temp/configured-public.der" >/dev/null 2>&1 ||
    byrecc_die "configured release public key is invalid"
openssl pkey \
    -pubin \
    -in "$byrecc_key_temp/derived-public.pem" \
    -outform DER \
    -out "$byrecc_key_temp/derived-public.der" >/dev/null 2>&1 ||
    byrecc_die "derived release public key is invalid"

cmp -s \
    "$byrecc_key_temp/configured-public.der" \
    "$byrecc_key_temp/derived-public.der" ||
    byrecc_die "release signing private key does not match the configured public key"

printf '%s\n' "release signing key pair is valid"
