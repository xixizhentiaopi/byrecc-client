#!/bin/sh

set -eu

repo_root=$(CDPATH= cd -- "$(dirname "$0")/.." && pwd)
temporary=$(mktemp -d "${TMPDIR:-/tmp}/byrecc-diagnostics-test.XXXXXX")
trap 'rm -rf "$temporary"' EXIT HUP INT TERM

test_home="$temporary/home"
test_rustup_home="${RUSTUP_HOME:-${HOME}/.rustup}"
test_cargo_home="${CARGO_HOME:-${HOME}/.cargo}"
mkdir -p "$test_home/config"

HOME="$test_home" XDG_CONFIG_HOME="$test_home/config" \
    RUSTUP_HOME="$test_rustup_home" CARGO_HOME="$test_cargo_home" \
    cargo run --quiet --manifest-path "$repo_root/Cargo.toml" -- clients \
    > "$temporary/clients.out"

for client in claude-code claude-desktop codex cursor; do
    grep -q "$client" "$temporary/clients.out"
done

if HOME="$test_home" XDG_CONFIG_HOME="$test_home/config" \
    RUSTUP_HOME="$test_rustup_home" CARGO_HOME="$test_cargo_home" \
    cargo run --quiet --manifest-path "$repo_root/Cargo.toml" -- doctor \
    > "$temporary/doctor.out" 2>&1; then
    printf '%s\n' "doctor unexpectedly succeeded without a login" >&2
    exit 1
fi
grep -q "not logged in" "$temporary/doctor.out"

printf '%s\n' "diagnostic command tests passed"
