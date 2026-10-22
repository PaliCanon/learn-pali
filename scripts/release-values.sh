#!/usr/bin/env bash
# The release a deploy writes onto the Cloud Run revision (task 060), from
# what GitHub Actions provides: GITHUB_REF_TYPE (tag | branch), GITHUB_REF_NAME
# and GITHUB_SHA. Two values, the two fields of the footer badge:
#
#   LEARN_PALI_ENV  the tag name (v0.1.8), main, or the branch (a dispatch of feat/x)
#   LEARN_PALI_SHA  empty for a tag, the first 7 characters of the commit otherwise
#
# deploy.yml passes data and parses nothing: it calls this script.
#
#   release-values.sh        LEARN_PALI_ENV=<env>,LEARN_PALI_SHA=<sha>, the exact
#                            value of `gcloud run deploy --update-env-vars`
#   release-values.sh env    the bare env
#   release-values.sh sha    the bare sha (an empty line for a tag)
#   release-values.sh same   reads a revision's JSON (`gcloud run revisions
#                            describe --format json`) on stdin: exit 0 when its
#                            two env values equal the intended ones (absent ==
#                            empty), 1 with `differs: ...` on stderr otherwise,
#                            2 when the JSON does not parse or its env is not
#                            a list of entries (a string, an object, numbers)
#
# Exit 1 with a message on stderr and nothing on stdout when the inputs are
# refused: a ref type that is neither tag nor branch, an empty ref name, a name
# with a comma (gcloud's list separator) or whitespace, a branch whose commit is
# not 7 hex characters at least. Exit 64 for a usage error.
set -euo pipefail

mode="${1:-}"
if [ "$#" -gt 1 ]; then
  echo "usage: release-values.sh [env|sha|same]" >&2
  exit 64
fi
case "$mode" in
  "" | env | sha | same) ;;
  *)
    echo "release-values.sh: unknown mode '$mode' (env, sha, same, or none)" >&2
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
  *,*) fail "the ref name '$ref_name' holds a comma: it is gcloud's list separator" ;;
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

# value_of KEY: the value of that entry of the revision's first container, or
# the empty string when the entry (or the whole env array) is absent.
value_of() {
  jq -r --arg k "$1" '
    try ((.spec.containers[0].env // []) | map(select(.name == $k)) | (.[0].value // ""))
    catch ""' <<<"$revision"
}

case "$mode" in
  "") printf 'LEARN_PALI_ENV=%s,LEARN_PALI_SHA=%s\n' "$env_value" "$sha_value" ;;
  env) printf '%s\n' "$env_value" ;;
  sha) printf '%s\n' "$sha_value" ;;
  same)
    revision="$(cat)"
    if ! jq -e . >/dev/null 2>&1 <<<"$revision"; then
      echo "release-values.sh: the revision JSON on stdin does not parse" >&2
      exit 2
    fi
    # Valid JSON of the wrong shape is unreadable, not "another release": the
    # caller logs the two cases apart.
    if ! jq -e 'try (
        ((.spec.containers // []) | type == "array")
        and (((.spec.containers[0].env // []) | type == "array")
          and all((.spec.containers[0].env // [])[]; type == "object"))
      ) catch false' >/dev/null 2>&1 <<<"$revision"; then
      echo "release-values.sh: the revision's env is not a list of entries" >&2
      exit 2
    fi
    have_env="$(value_of LEARN_PALI_ENV)"
    have_sha="$(value_of LEARN_PALI_SHA)"
    if [ "$have_env" != "$env_value" ]; then
      echo "differs: LEARN_PALI_ENV is '$have_env' on the revision, '$env_value' intended" >&2
      exit 1
    fi
    if [ "$have_sha" != "$sha_value" ]; then
      echo "differs: LEARN_PALI_SHA is '$have_sha' on the revision, '$sha_value' intended" >&2
      exit 1
    fi
    ;;
esac
