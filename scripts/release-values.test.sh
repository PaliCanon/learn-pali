#!/usr/bin/env bash
# Tests for scripts/release-values.sh (task 060, trimmed by 083).
# Run: bash scripts/release-values.test.sh
#
# The script turns what Actions provides (GITHUB_REF_TYPE, GITHUB_REF_NAME,
# GITHUB_SHA) into the two values a deploy prints for the stack: LEARN_PALI_ENV
# (the tag, main, or the branch) and LEARN_PALI_SHA (empty for a tag, sha7
# otherwise). The expected values below are written by hand, never computed
# with the script's own rules. The workflow that calls the script is covered by
# deploy-workflow.test.sh.
#
# Needs bash, python3 with PyYAML. Exit 0 when every assertion held.
set -uo pipefail

here="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
root="$(cd "$here/.." && pwd)"
script="$here/release-values.sh"
work="$(mktemp -d)"
trap 'rm -rf "$work"' EXIT

checks=0
failed=0
pass() { checks=$((checks + 1)); echo "ok   - $1"; }
flunk() { checks=$((checks + 1)); failed=$((failed + 1)); echo "FAIL - $1"; }
assert_eq() { # name want got
  if [ "$2" = "$3" ]; then pass "$1"; else flunk "$1 (want '$2', got '$3')"; fi
}
assert_match() { # name pattern text
  if grep -Eq -- "$2" <<<"$3"; then pass "$1"; else flunk "$1 (no match for /$2/ in: $3)"; fi
}
assert_file_has() { # name file pattern
  if grep -Eq -- "$3" "$2" 2>/dev/null; then pass "$1"; else flunk "$1 (/$3/ not in $2)"; fi
}
assert_file_lacks() { # name file pattern
  if grep -Eq -- "$3" "$2" 2>/dev/null; then flunk "$1 (/$3/ found in $2)"; else pass "$1"; fi
}

# rv TYPE NAME SHA [ARGS...] -> $out (stdout), $err (stderr), $code. The
# environment holds only those three variables, as an Actions step would.
rv() {
  local t=$1 n=$2 s=$3
  shift 3
  out="$(env -i PATH="$PATH" GITHUB_REF_TYPE="$t" GITHUB_REF_NAME="$n" GITHUB_SHA="$s" bash "$script" "$@" 2>"$work/err" </dev/null)"
  code=$?
  err="$(cat "$work/err")"
}
# refused NAME TYPE REF SHA: exit 1, a message on stderr, nothing on stdout, in
# every mode that reads the inputs.
refused() {
  local name=$1
  shift
  local mode
  for mode in "" env sha; do
    rv "$@" ${mode:+"$mode"}
    assert_eq "$name: exit 1 (mode '${mode:-default}')" "1" "$code"
    assert_eq "$name: nothing on stdout (mode '${mode:-default}')" "" "$out"
    if [ -n "$err" ]; then pass "$name: a message on stderr (mode '${mode:-default}')"; else flunk "$name: a message on stderr (mode '${mode:-default}')"; fi
  done
}

SHA=34c489c0123456789

echo "== release-values.sh: the values of the three events"
rv branch main "$SHA"
assert_eq "push to main: the exit is 0" "0" "$code"
assert_eq "push to main: the value line" "LEARN_PALI_ENV=main,LEARN_PALI_SHA=34c489c" "$out"
# 060 fix 3 (H3): a tag run by hand (`gh workflow run deploy.yml --ref v0.1.8`,
# the recovery line of the READMEs) is a workflow_dispatch event whose ref type
# is still `tag`: the event name must not change the values.
out="$(env -i PATH="$PATH" GITHUB_EVENT_NAME=workflow_dispatch GITHUB_REF_TYPE=tag GITHUB_REF_NAME=v0.1.8 GITHUB_SHA="$SHA" bash "$script" 2>"$work/err" </dev/null)"
code=$?
assert_eq "a tag dispatched by hand: the exit is 0" "0" "$code"
assert_eq "a tag dispatched by hand: the value line (the tag, no sha)" "LEARN_PALI_ENV=v0.1.8,LEARN_PALI_SHA=" "$out"
out="$(env -i PATH="$PATH" GITHUB_EVENT_NAME=workflow_dispatch GITHUB_REF_TYPE=branch GITHUB_REF_NAME=feat/x GITHUB_SHA="$SHA" bash "$script" 2>"$work/err" </dev/null)"
assert_eq "a branch dispatched by hand: the value line (the branch and the sha7)" "LEARN_PALI_ENV=feat/x,LEARN_PALI_SHA=34c489c" "$out"
rv tag v0.1.8 "$SHA"
assert_eq "tag v0.1.8: the exit is 0" "0" "$code"
assert_eq "tag v0.1.8: the value line (no sha)" "LEARN_PALI_ENV=v0.1.8,LEARN_PALI_SHA=" "$out"
rv branch feat/x "$SHA"
assert_eq "dispatch of feat/x: the value line" "LEARN_PALI_ENV=feat/x,LEARN_PALI_SHA=34c489c" "$out"
rv tag v0.1.8 ""
assert_eq "tag with a blank GITHUB_SHA: still ok" "0" "$code"
assert_eq "tag with a blank GITHUB_SHA: the sha stays empty" "LEARN_PALI_ENV=v0.1.8,LEARN_PALI_SHA=" "$out"
rv tag v0.1.8 "zz"
assert_eq "tag: GITHUB_SHA is ignored even when it is garbage" "LEARN_PALI_ENV=v0.1.8,LEARN_PALI_SHA=" "$out"
rv branch main 34c489c
assert_eq "a branch with exactly 7 characters of sha" "LEARN_PALI_ENV=main,LEARN_PALI_SHA=34c489c" "$out"
rv branch main 34C489C0123
assert_eq "an upper-case hex sha is kept as given" "LEARN_PALI_ENV=main,LEARN_PALI_SHA=34C489C" "$out"

echo "== release-values.sh: the env and sha modes"
rv branch main "$SHA" env
assert_eq "env, main" "main" "$out"
rv branch main "$SHA" sha
assert_eq "sha, main" "34c489c" "$out"
rv tag v0.1.8 "$SHA" env
assert_eq "env, tag" "v0.1.8" "$out"
rv tag v0.1.8 "$SHA" sha
assert_eq "sha, tag: an empty value" "" "$out"
assert_eq "sha, tag: the exit is 0" "0" "$code"
assert_eq "sha, tag: one empty line" "1" "$(env -i PATH="$PATH" GITHUB_REF_TYPE=tag GITHUB_REF_NAME=v0.1.8 GITHUB_SHA="$SHA" bash "$script" sha | grep -c '^$')"
rv branch feat/x "$SHA" env
assert_eq "env, dispatch" "feat/x" "$out"
rv branch feat/x "$SHA" sha
assert_eq "sha, dispatch" "34c489c" "$out"

echo "== release-values.sh: refusals (exit 1, stderr, empty stdout)"
refused "a pull_request ref type" pull_request main "$SHA"
refused "an empty ref type" "" main "$SHA"
refused "an empty ref name" branch "" "$SHA"
refused "an empty tag name" tag "" "$SHA"
refused "a name with a comma" branch "a,b" "$SHA"
refused "a tag name with a comma" tag "v1,2" "$SHA"
refused "a name with a space" branch "a b" "$SHA"
refused "a name with a tab" branch "$(printf 'a\tb')" "$SHA"
refused "a name with a newline" branch "$(printf 'a\nb')" "$SHA"
refused "a branch with a blank sha" branch main ""
refused "a branch with a 6-character sha" branch main 34c489
refused "a branch with a sha that is not hex" branch main zzzzzzzzz
out="$(env -i PATH="$PATH" bash "$script" 2>"$work/err" </dev/null)"
code=$?
assert_eq "unset variables: exit 1" "1" "$code"
assert_eq "unset variables: nothing on stdout" "" "$out"
assert_match "unset variables: a message" "." "$(cat "$work/err")"
out="$(env -i PATH="$PATH" GITHUB_REF_TYPE=tag bash "$script" 2>"$work/err" </dev/null)"
code=$?
assert_eq "a tag with no ref name variable: exit 1" "1" "$code"
out="$(env -i PATH="$PATH" GITHUB_REF_TYPE=branch GITHUB_REF_NAME=main bash "$script" 2>"$work/err" </dev/null)"
code=$?
assert_eq "a branch with no sha variable: exit 1" "1" "$code"
assert_eq "a branch with no sha variable: nothing on stdout" "" "$out"

echo "== release-values.sh: an unknown mode"
rv branch main "$SHA" nope
assert_eq "unknown mode: exit 64" "64" "$code"
assert_eq "unknown mode: nothing on stdout" "" "$out"
rv branch main "$SHA" env extra
assert_eq "two arguments: exit 64" "64" "$code"

echo "== release-values.sh same: gone with Cloud Run, an unknown mode now"
out="$(printf '{}' | env -i PATH="$PATH" GITHUB_REF_TYPE=branch GITHUB_REF_NAME=main GITHUB_SHA="$SHA" bash "$script" same 2>"$work/err")"
code=$?
assert_eq "same: exit 64 (unknown mode)" "64" "$code"
assert_eq "same: nothing on stdout" "" "$out"

echo "== the script file"
assert_eq "release-values.sh is executable" "yes" "$([ -x "$script" ] && echo yes || echo no)"
assert_eq "release-values.sh starts with the bash shebang" "#!/usr/bin/env bash" "$(head -n 1 "$script")"

echo "== ci.yml: every test this repository holds for the release runs in CI"
ci_runs() { # -> the run: lines of ci.yml
  python3 -c '
import sys, yaml
for job in yaml.safe_load(open(sys.argv[1]))["jobs"].values():
    for step in job.get("steps", []):
        if "run" in step:
            print(step["run"])
' "$root/.github/workflows/ci.yml"
}
ci_runs > "$work/ci-runs"
assert_file_has "ci.yml runs release-values.test.sh" "$work/ci-runs" '^bash scripts/release-values\.test\.sh$'
assert_file_has "ci.yml runs the release_document browser test" "$work/ci-runs" '--test release_document$'

echo "== $checks checks, $failed failed"
[ "$failed" = 0 ]
