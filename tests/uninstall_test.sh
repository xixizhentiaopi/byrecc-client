#!/bin/sh

set -eu

repo_root=$(CDPATH= cd -- "$(dirname "$0")/.." && pwd)
temporary=$(mktemp -d "${TMPDIR:-/tmp}/byrecc-uninstall-test.XXXXXX")
trap 'rm -rf "$temporary"' EXIT HUP INT TERM

BYRECC_INSTALL_DIR="$temporary/bin"
export BYRECC_INSTALL_DIR
mkdir -p "$BYRECC_INSTALL_DIR"

cat > "$BYRECC_INSTALL_DIR/byrectl" <<'EOF'
#!/bin/sh
printf '%s\n' "$@" > "$BYRECC_TEST_ARGS"
EOF
chmod 0755 "$BYRECC_INSTALL_DIR/byrectl"
BYRECC_TEST_ARGS="$temporary/args"
export BYRECC_TEST_ARGS

sh "$repo_root/uninstall.sh" --yes --local-only
expected=$(printf '%s\n' uninstall --yes --local-only)
actual=$(cat "$BYRECC_TEST_ARGS")
[ "$actual" = "$expected" ] || {
    printf '%s\n' "uninstall.sh did not preserve CLI arguments" >&2
    exit 1
}

sh "$repo_root/uninstall.sh" --help | grep -q "does not download"

test_home="$temporary/home"
test_rustup_home="${RUSTUP_HOME:-${HOME}/.rustup}"
test_cargo_home="${CARGO_HOME:-${HOME}/.cargo}"
mkdir -p "$test_home/.cursor" "$test_home/.codex" "$test_home/config"
printf '%s\n' \
    '{"mcpServers":{"keep":{"command":"keep"},"byrecc":{"command":"remove"}},"theme":"dark"}' \
    > "$test_home/.claude.json"
printf '%s\n' \
    '{"mcpServers":{"keep":{"command":"keep"},"byrecc":{"command":"remove"}}}' \
    > "$test_home/.cursor/mcp.json"
cat > "$test_home/.codex/config.toml" <<'EOF'
model = "keep"

[mcp_servers.keep]
command = "keep"

[mcp_servers.byrecc]
command = "remove"
EOF

HOME="$test_home" XDG_CONFIG_HOME="$test_home/config" \
    RUSTUP_HOME="$test_rustup_home" CARGO_HOME="$test_cargo_home" \
    cargo run --quiet --manifest-path "$repo_root/Cargo.toml" -- \
    uninstall --yes --keep-skill --keep-binary

grep -q '"keep"' "$test_home/.claude.json"
grep -q '"theme": "dark"' "$test_home/.claude.json"
! grep -q '"byrecc"' "$test_home/.claude.json"
grep -q '"keep"' "$test_home/.cursor/mcp.json"
! grep -q '"byrecc"' "$test_home/.cursor/mcp.json"
grep -q 'model = "keep"' "$test_home/.codex/config.toml"
grep -q '\[mcp_servers.keep\]' "$test_home/.codex/config.toml"
! grep -q '\[mcp_servers.byrecc\]' "$test_home/.codex/config.toml"

skill_root="$test_home/.agents/skills/byrecc"
mkdir -p "$skill_root/agents" "$skill_root/references" "$test_home/.claude/skills"
printf '%s\n' "known" > "$skill_root/SKILL.md"
printf '%s\n' "known" > "$skill_root/agents/openai.yaml"
printf '%s\n' "known" > "$skill_root/references/tools.md"
printf '%s\n' "known" > "$skill_root/references/errors.md"
printf '%s\n' "known" > "$skill_root/version.txt"
printf '%s\n' "preserve" > "$skill_root/user-note.txt"
ln -s "$skill_root" "$test_home/.claude/skills/byrecc"

HOME="$test_home" XDG_CONFIG_HOME="$test_home/config" \
    RUSTUP_HOME="$test_rustup_home" CARGO_HOME="$test_cargo_home" \
    cargo run --quiet --manifest-path "$repo_root/Cargo.toml" -- \
    uninstall --yes --keep-binary

[ ! -L "$test_home/.claude/skills/byrecc" ]
[ -f "$skill_root/user-note.txt" ]
[ ! -e "$skill_root/SKILL.md" ]
[ ! -e "$skill_root/agents/openai.yaml" ]
[ ! -e "$skill_root/references/tools.md" ]

printf '%s\n' "uninstall tests passed"
