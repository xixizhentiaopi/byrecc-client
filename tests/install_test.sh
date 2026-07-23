#!/bin/sh

set -eu

byrecc_repo_dir="$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)"
byrecc_test_output="$(mktemp "${TMPDIR:-/tmp}/byrecc-installer-test.XXXXXX")"

byrecc_cleanup() {
    rm -f "$byrecc_test_output"
}
trap byrecc_cleanup EXIT HUP INT TERM

sh -n "${byrecc_repo_dir}/install.sh"
byrecc_help_output="$(sh "${byrecc_repo_dir}/install.sh" --installer-help)"

case "$byrecc_help_output" in
    *"No sudo or system package manager"*)
        printf '%s\n' "installer syntax and help checks passed"
        ;;
    *)
        printf '%s\n' "installer help is missing the security boundary" >&2
        exit 1
        ;;
esac

if sh "${byrecc_repo_dir}/install.sh" >"$byrecc_test_output" 2>&1; then
    printf '%s\n' "unsigned source installer unexpectedly succeeded" >&2
    exit 1
fi

if ! grep -q "release signing key is not embedded" "$byrecc_test_output"; then
    printf '%s\n' "unsigned source installer did not fail closed" >&2
    exit 1
fi

printf '%s\n' "unsigned source installer fails closed as expected"
