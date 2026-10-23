#!/usr/bin/env bash
# Tests for scripts/release-values.sh and its wiring in deploy.yml (task 060).
# Run: bash scripts/release-values.test.sh
#
# The script turns what Actions provides (GITHUB_REF_TYPE, GITHUB_REF_NAME,
# GITHUB_SHA) into the two values deploy.yml writes onto the Cloud Run
# revision: LEARN_PALI_ENV (the tag, main, or the branch) and LEARN_PALI_SHA
# (empty for a tag, sha7 otherwise), and compares them with a serving
# revision's env (`same`). The expected values below are written by hand, never
# computed with the script's own rules.
#
# What the wiring tests (the last section) do NOT prove: they read deploy.yml's
# text. A wrong script is caught by the literal-value tests above them, and a
# wrong workflow (a step that never runs, a value that never reaches the
# revision) only by the gate against the served page (rollout-check.sh
# revision, ROLLOUT_EXPECT_ENV / ROLLOUT_EXPECT_SHA). The step scripts run
# against a stub gcloud in rollout-check.test.sh.
#
# Needs bash, jq, python3 with PyYAML. Exit 0 when every assertion held.
set -uo pipefail

here="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
root="$(cd "$here/.." && pwd)"
script="$here/release-values.sh"
workflow="$root/.github/workflows/deploy.yml"
# A hook is a local mutation tool: set in CI it would make this suite test a
# copy, so it is refused (060 fix 3, H5).
for hook in RELEASE_TEST_README; do
  if [ -n "${CI:-}" ] && [ -n "${!hook:-}" ]; then
    echo "$(basename "$0"): the test hook $hook is refused when CI is set (it points the suite at a copy)" >&2
    exit 64
  fi
done
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

echo "== release-values.sh same: the serving revision against the intended release"
# same TYPE NAME SHA JSON -> $code, $err
same() {
  local t=$1 n=$2 s=$3 json=$4
  out="$(printf '%s' "$json" | env -i PATH="$PATH" GITHUB_REF_TYPE="$t" GITHUB_REF_NAME="$n" GITHUB_SHA="$s" bash "$script" same 2>"$work/err")"
  code=$?
  err="$(cat "$work/err")"
}
rev() { # ENV-ARRAY-JSON -> a revision document holding it (hand-written shape of `gcloud run revisions describe --format json`)
  printf '{"metadata":{"name":"learn-pali-dev-00001-aaa"},"spec":{"containers":[{"image":"registry/learn-pali:h-1","env":%s}]}}' "$1"
}
NOENV='{"metadata":{"name":"r"},"spec":{"containers":[{"image":"registry/learn-pali:h-1"}]}}'
same branch main "$SHA" "$NOENV"
assert_eq "no env array at all vs a push to main: differs" "1" "$code"
assert_match "no env array vs main: 'differs:' on stderr" "^differs:" "$err"
assert_eq "no env array vs main: nothing on stdout" "" "$out"
same tag v0.1.8 "$SHA" "$NOENV"
assert_eq "no env array at all vs a tag: LEARN_PALI_ENV is absent, differs" "1" "$code"
same tag v0.1.8 "$SHA" "$(rev '[{"name":"LEARN_PALI_ENV","value":"v0.1.8"}]')"
assert_eq "env v0.1.8, sha absent vs tag v0.1.8: same" "0" "$code"
same tag v0.1.8 "$SHA" "$(rev '[{"name":"LEARN_PALI_ENV","value":"v0.1.8"},{"name":"LEARN_PALI_SHA","value":""}]')"
assert_eq "env v0.1.8, sha empty vs tag v0.1.8: same" "0" "$code"
same tag v0.1.8 "$SHA" "$(rev '[{"name":"LEARN_PALI_SHA","value":""},{"name":"LEARN_PALI_ENV","value":"v0.1.8"}]')"
assert_eq "the order of the entries does not matter" "0" "$code"
same tag v0.1.8 "$SHA" "$(rev '[{"name":"LEARN_PALI_ENV","value":"v0.1.8"},{"name":"LEARN_PALI_SHA"}]')"
assert_eq "an entry with no value key is an empty value: same" "0" "$code"
same tag v0.1.8 "$SHA" "$(rev '[{"name":"LEARN_PALI_ENV","value":"v0.1.8"},{"name":"LEARN_PALI_SHA","value":"34c489c"}]')"
assert_eq "tag intended, sha present on the revision: differs" "1" "$code"
same branch main "$SHA" "$(rev '[{"name":"LEARN_PALI_ENV","value":"main"},{"name":"LEARN_PALI_SHA","value":"34c489c"}]')"
assert_eq "env main sha 34c489c vs the intended main 34c489c: same" "0" "$code"
same branch main 5555555aaaa "$(rev '[{"name":"LEARN_PALI_ENV","value":"main"},{"name":"LEARN_PALI_SHA","value":"34c489c"}]')"
assert_eq "env main sha 34c489c vs a new push 5555555: differs" "1" "$code"
assert_match "a new push: the message names the sha" "LEARN_PALI_SHA.*34c489c.*5555555" "$err"
same tag v0.1.9 "$SHA" "$(rev '[{"name":"LEARN_PALI_ENV","value":"main"},{"name":"LEARN_PALI_SHA","value":"34c489c"}]')"
assert_eq "env main vs a tag: differs" "1" "$code"
same tag v0.1.9 "$SHA" "$(rev '[{"name":"LEARN_PALI_ENV","value":"v0.1.8"},{"name":"LEARN_PALI_SHA","value":""}]')"
assert_eq "tag v0.1.8 on the revision vs v0.1.9 intended: differs" "1" "$code"
same branch main "$SHA" "$(rev '[{"name":"RUST_LOG","value":"info"}]')"
assert_eq "an unrelated env array (RUST_LOG only) vs a branch: differs" "1" "$code"
same branch main "$SHA" "$(rev '[{"name":"RUST_LOG","value":"info"},{"name":"LEARN_PALI_ENV","value":"main"},{"name":"LEARN_PALI_SHA","value":"34c489c"}]')"
assert_eq "other entries beside the two do not matter: same" "0" "$code"
same branch main "$SHA" "$(rev '[{"name":"LEARN_PALI_ENV","value":"main"},{"name":"LEARN_PALI_SHA","value":"34c489c"}]' | sed 's/"env"/"nv"/')"
assert_eq "a document with no env key: differs, not an error" "1" "$code"
same branch main "$SHA" '{"spec":{"containers":[]}}'
assert_eq "a revision with no container: differs, not an error" "1" "$code"
# 060 fix 3 (H1): JSON that parses but whose env is not a list of entries (a
# string, an object, numbers) is a revision this script cannot read: exit 2
# with a message, never "differs" (deploy.yml logs the two cases apart).
for bad in '"oops"' '{"name":"LEARN_PALI_ENV"}' '[1,2]'; do
  same branch main "$SHA" "$(rev "$bad")"
  assert_eq "env $bad (not a list of entries): exit 2" "2" "$code"
  assert_match "env $bad: the message says the env is not a list" "env .*not a list" "$err"
  assert_eq "env $bad: nothing on stdout" "" "$out"
done
same branch main "$SHA" '{"spec":{"containers":"oops"}}'
assert_eq "containers that is not a list: exit 2" "2" "$code"
same branch main "$SHA" 'not json {'
assert_eq "invalid JSON: exit 2" "2" "$code"
assert_eq "invalid JSON: nothing on stdout" "" "$out"
same branch main "$SHA" ''
assert_eq "empty stdin: exit 2" "2" "$code"
same branch main "" "$(rev '[{"name":"LEARN_PALI_ENV","value":"main"}]')"
assert_eq "same: bad inputs are refused, exit 1" "1" "$code"

echo "== deploy.yml: the wiring (a text read, see the header for what it does not prove)"
assert_eq "release-values.sh is executable" "yes" "$([ -x "$script" ] && echo yes || echo no)"
assert_eq "release-values.sh starts with the bash shebang" "#!/usr/bin/env bash" "$(head -n 1 "$script")"
step_run() { # ID -> the step's script
  python3 - "$workflow" "$1" <<'PY'
import sys, yaml
for s in yaml.safe_load(open(sys.argv[1]))["jobs"]["site"]["steps"]:
    if s.get("id") == sys.argv[2]:
        print(s.get("run", ""))
        break
else:
    sys.exit(3)
PY
}
for id in current deploy check; do
  step_run "$id" > "$work/step-$id.sh" 2>/dev/null || { flunk "deploy.yml has the step '$id'"; : > "$work/step-$id.sh"; }
done
assert_file_has "the deploy step writes the release with --update-env-vars" "$work/step-deploy.sh" '--update-env-vars "\$\(scripts/release-values\.sh\)[^"]*"'
assert_file_has "the deploy step keeps --no-traffic and the tag" "$work/step-deploy.sh" '--no-traffic --tag'
assert_file_lacks "the set variant of the flag appears nowhere (it would wipe RUST_LOG and the secret reference)" "$workflow" '--set-env-vars'
assert_file_lacks "no LEARN_PALI_GIT build arg is left" "$workflow" 'LEARN_PALI_GIT'
assert_file_lacks "no build-args is left" "$workflow" 'build-args'
assert_file_has "the current step pipes the serving revision to 'release-values.sh same'" "$work/step-current.sh" 'release-values\.sh same <<<"\$REVISION_JSON"'
assert_file_has "the current step reads the revision as JSON" "$work/step-current.sh" 'revisions describe "\$SERVING".*--format json'
assert_file_has "the check step expects the env" "$work/step-check.sh" 'ROLLOUT_EXPECT_ENV="\$\(scripts/release-values\.sh env\)"'
assert_file_has "the check step expects the sha" "$work/step-check.sh" 'ROLLOUT_EXPECT_SHA="\$\(scripts/release-values\.sh sha\)"'
assert_file_has "the check step has its own message for exit 8" "$work/step-check.sh" '= 8|= "8"|8\)'

# The README says what the flag and the gate's inputs are (060 fix round 1, F3).
# RELEASE_TEST_README points the suite at a README copy (a mutation run).
readme="${RELEASE_TEST_README:-$root/README.md}"
infra_readme="$root/infra/README.md"
assert_file_has "README: says --set-env-vars is never used" "$readme" '[Nn]ever .*--set-env-vars'
assert_file_has "README: says ROLLOUT_EXPECT_SHA may be empty (a tag has no sha)" "$readme" 'may be empty'
assert_file_has "README: says ROLLOUT_EXPECT_ENV is non-empty" "$readme" '`ROLLOUT_EXPECT_ENV` (must be|is) non-empty|non-empty .*ROLLOUT_EXPECT_ENV'
assert_file_has "infra/README: says --set-env-vars is never used" "$infra_readme" '[Nn]ever .*--set-env-vars'

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
assert_file_has "ci.yml runs lifecycle.test.sh" "$work/ci-runs" '^bash infra/scripts/lifecycle\.test\.sh$'
assert_file_has "ci.yml runs the release_document browser test" "$work/ci-runs" '--test release_document$'

echo "== the test hooks are refused when CI is set (060 fix 3, H5)"
# A hook points a suite at a mutated copy (a mutation run). A leftover in the
# environment of a CI job would make the suite test the copy, not the code:
# every suite exits 64 and names the hook instead. Each hook is set to the real
# file it replaces, so the only reason to refuse is CI. RELEASE_TEST_NESTED
# stops the inner run of this very suite from running this section again.
if [ -z "${RELEASE_TEST_NESTED:-}" ]; then
  hook_refused() { # SUITE HOOK VALUE
    local suite=$1 hook=$2 value=$3 hout hcode
    hout="$(env CI=1 RELEASE_TEST_NESTED=1 "$hook=$value" bash "$suite" 2>&1 </dev/null)"
    hcode=$?
    assert_eq "CI=1 and $hook set: $(basename "$suite") exits 64" "64" "$hcode"
    assert_match "CI=1 and $hook set: the message names the hook" "$hook" "$hout"
    assert_eq "CI=1 and $hook set: the suite ran no check" "0" "$(grep -c '^ok ' <<<"$hout")"
  }
  hook_refused "$here/rollout-check.test.sh" ROLLOUT_TEST_SCRIPT "$here/rollout-check.sh"
  hook_refused "$here/rollout-check.test.sh" ROLLOUT_TEST_WORKFLOW "$workflow"
  hook_refused "$here/release-values.test.sh" RELEASE_TEST_README "$root/README.md"
  hook_refused "$root/infra/scripts/lifecycle.test.sh" GCP_TF "$root/infra/gcp.tf"
  hook_refused "$root/infra/scripts/lifecycle.test.sh" README_MD "$root/infra/README.md"
  # And the pair: with no CI, a hook is still honoured (a mutation run works).
  hout="$(env -u CI RELEASE_TEST_NESTED=1 GCP_TF="$root/infra/gcp.tf" bash "$root/infra/scripts/lifecycle.test.sh" 2>&1 </dev/null)"
  assert_eq "no CI and GCP_TF set: lifecycle.test.sh runs and passes" "0" "$?"
  assert_match "no CI and GCP_TF set: the suite ran its checks" "^ok " "$hout"
fi

echo "== $checks checks, $failed failed"
[ "$failed" = 0 ]
