#!/usr/bin/env bash
# Tests for scripts/manifest-ref.sh (task 007, slice S5): the git ref a
# manifest names for a dependency (a tag, a rev or a branch), which the
# `siblings` action checks the sibling repository out at.
# Run: bash scripts/manifest-ref.test.sh
set -uo pipefail
here=$(cd "$(dirname "$0")" && pwd)
script="$here/manifest-ref.sh"
work=$(mktemp -d)
trap 'rm -rf "$work"' EXIT
pass=0
fail=0
expect() { # expect NAME WANT MANIFEST_TEXT CRATE
  local name=$1 want=$2 text=$3 crate=$4 got
  printf '%s\n' "$text" >"$work/Cargo.toml"
  got=$(bash "$script" "$work/Cargo.toml" "$crate" 2>/dev/null) || got="<failed>"
  if [ "$got" = "$want" ]; then pass=$((pass + 1)); echo "ok   $name"; else fail=$((fail + 1)); echo "FAIL $name: want '$want', got '$got'"; fi
}

expect "a branch" main 'palicanon-ui-lib = { git = "ssh://git@github.com/PaliCanon/ui-lib.git", branch = "main" }' palicanon-ui-lib
expect "a tag" v0.2.0 'palicanon-ui-lib = { git = "ssh://git@github.com/PaliCanon/ui-lib.git", tag = "v0.2.0" }' palicanon-ui-lib
expect "a rev" 0123abc 'palicanon-api-client = { git = "ssh://git@github.com/PaliCanon/api.git", rev = "0123abc", optional = true }' palicanon-api-client
expect "optional after the branch" main 'palicanon-api-client = { git = "ssh://git@github.com/PaliCanon/api.git", branch = "main", optional = true }' palicanon-api-client
expect "the right crate among several" feat-x 'palicanon-ui-lib = { git = "x", branch = "main" }
palicanon-api-client = { git = "y", branch = "feat-x" }' palicanon-api-client
expect "a tag wins over a branch" v1 'palicanon-ui-lib = { git = "x", branch = "main", tag = "v1" }' palicanon-ui-lib
expect "no ref is a failure" "<failed>" 'palicanon-ui-lib = { git = "x" }' palicanon-ui-lib
expect "an absent crate is a failure" "<failed>" 'leptos = "0.8"' palicanon-ui-lib
expect "a path dependency is a failure" "<failed>" 'palicanon-ui-lib = { path = "../ui-lib" }' palicanon-ui-lib
expect "a prefix of another crate is not it" "<failed>" 'palicanon-ui-lib-extra = { git = "x", branch = "main" }' palicanon-ui-lib

echo
echo "manifest-ref.test.sh: $pass passed, $fail failed"
test "$fail" -eq 0
