#!/bin/sh

set -eu

byrecc_repo_dir="$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)"
byrecc_skill="${byrecc_repo_dir}/skills/byrecc/SKILL.md"

test -f "$byrecc_skill"
test "$(sed -n '1p' "$byrecc_skill")" = "---"
grep -q '^name: byrecc$' "$byrecc_skill"
grep -q '^description: ' "$byrecc_skill"
grep -q '^---$' "$byrecc_skill"

for reference in references/tools.md references/errors.md agents/openai.yaml version.txt; do
    test -s "${byrecc_repo_dir}/skills/byrecc/${reference}"
done

printf '%s\n' "skill structure checks passed"

