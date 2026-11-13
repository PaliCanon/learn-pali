#!/usr/bin/env bash
# The release a deploy prints for the stack (task 060, 083), from what GitHub
# Actions provides: GITHUB_REF_TYPE (tag | branch), GITHUB_REF_NAME and
# GITHUB_SHA. Two values, the two fields of the footer badge:
#
#   LEARN_PALI_ENV  the tag name (v0.1.8), main, or the branch (a dispatch of feat/x)
#   LEARN_PALI_SHA  empty for a tag, the first 7 characters of the commit otherwise
#
# deploy.yml passes data and parses nothing: it calls this script.
#
#   release-values.sh        LEARN_PALI_ENV=<env>,LEARN_PALI_SHA=<sha>
#   release-values.sh env    the bare env
#   release-values.sh sha    the bare sha (an empty line for a tag)
#
# Exit 1 with a message on stderr and nothing on stdout when the inputs are
# refused: a ref type that is neither tag nor branch, an empty ref name, a name
# with a comma (the separator of the default line) or whitespace, a branch whose
# commit is not 7 hex characters at least. Exit 64 for a usage error.
set -euo pipefail

mode="${1:-}"
if [ "$#" -gt 1 ]; then
  echo "usage: release-values.sh [env|sha]" >&2
  exit 64
fi
case "$mode" in
  "" | env | sha) ;;
  *)
    echo "release-values.sh: unknown mode '$mode' (env, sha, or none)" >&2
    exit 64
    ;;
esac

ref_type="${GITHUB_REF_TYPE:-}"
ref_name="${GITHUB_REF_NAME:-}"
commit="${GITHUB_SHA:-}"

fail() {
  echo "release-values.sh: $1" >&2
  exit 1
}

case "$ref_type" in
  tag | branch) ;;
  *) fail "GITHUB_REF_TYPE is '$ref_type': only a tag or a branch carries a release" ;;
esac
[ -n "$ref_name" ] || fail "GITHUB_REF_NAME is empty"
case "$ref_name" in
  *,*) fail "the ref name '$ref_name' holds a comma: it is the separator of the value line" ;;
esac
if [[ "$ref_name" =~ [[:space:]] ]]; then
  fail "the ref name holds whitespace"
fi

env_value="$ref_name"
sha_value=""
if [ "$ref_type" = branch ]; then
  head7="${commit:0:7}"
  [[ "$head7" =~ ^[0-9a-fA-F]{7}$ ]] || fail "GITHUB_SHA '$commit' is not at least 7 hex characters"
  sha_value="$head7"
fi

case "$mode" in
  "") printf 'LEARN_PALI_ENV=%s,LEARN_PALI_SHA=%s\n' "$env_value" "$sha_value" ;;
  env) printf '%s\n' "$env_value" ;;
  sha) printf '%s\n' "$sha_value" ;;
esac
