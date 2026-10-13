#!/usr/bin/env bash
# The deployed service is right, whoever asks: run by deploy.yml on the tagged
# revision before it takes traffic, on the bare Cloud Run URL after, and on
# the public host last. Model: search-front's scripts/rollout-check.sh.
#
#   scripts/rollout-check.sh revision URL HEADER_FILE
#       a revision's own URL (the tagged *.run.app, origin guard on). The
#       secret comes from HEADER_FILE, one line `x-origin-auth: <secret>`,
#       handed to curl as `-H @file`: never on a command line, never printed.
#   scripts/rollout-check.sh --refused URL
#       the bare service URL (*.run.app), as anyone who learned it would call
#       it: 403 with an EMPTY body without the header and with a wrong one (a
#       403 with a body is Google's front end, not our guard). Sends no secret.
#   scripts/rollout-check.sh --access URL
#       the public host with Cloudflare Access in front (dev): it must answer
#       302 to *.cloudflareaccess.com (scripts/access-redirect-pattern, the
#       one rule infra/scripts/origin-secret check reads too). Sends no
#       secret and follows nothing.
#
# `revision` checks, each named in the output: `root` (/ is 200 and names
# PaliLearn), `wasm` (the .wasm the page links is served), `token` (no token
# shape in the bodies and headers of / and the wasm), `secret` (the origin
# secret itself, read from HEADER_FILE, is in none of them; its value is
# never printed), `guard` (without the header, with a wrong one, or with two
# x-origin-auth headers, the right one and a wrong one: 403 and an empty body).
#
# Exit codes
#   revision, --refused: 0 all good; 1 a check failed (named); 64 usage.
#   --access: 0 the Access redirect; 2 the host answered 200 (Access is
#     missing); 3 403 with cf-mitigated (Cloudflare challenged the runner:
#     inconclusive, says nothing about the service); 4 anything else, a
#     timeout included; 64 usage.
#
# Every curl has --max-time (ROLLOUT_MAX_TIME seconds, default 60).
set -uo pipefail
export LC_ALL=C

# The Access redirect: one rule, shared with infra/scripts/origin-secret.
access_pattern_file="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)/access-redirect-pattern"
token_pattern='pc_(dev|prod)_[0-9A-Za-z]{43}'
max_time="${ROLLOUT_MAX_TIME:-60}"
work="$(mktemp -d)"
trap 'rm -rf "$work"' EXIT

usage() {
  echo "usage: rollout-check.sh revision URL HEADER_FILE | --refused URL | --access URL" >&2
  exit 64
}

case "$max_time" in '' | *[!0-9]*) echo "rollout-check: ROLLOUT_MAX_TIME must be whole seconds" >&2; exit 64 ;; esac

mode="${1:-}"
case "$mode" in
  revision | --refused | --access) shift ;;
  *) usage ;;
esac
url="${1:-}"
header_file="${2:-}"
[ -n "$url" ] || usage
url="${url%/}"

if [ "$mode" = revision ]; then
  [ -n "$header_file" ] || usage
  [ "$#" -le 2 ] || usage
  [ -f "$header_file" ] || { echo "rollout-check: usage: the header file is not a file" >&2; exit 64; }
  if [ "$(grep -c '' "$header_file")" != 1 ] \
    || ! grep -Eq '^[Xx]-[Oo]rigin-[Aa]uth:[[:space:]]*[^[:space:]]+[[:space:]]*$' "$header_file"; then
    echo "rollout-check: usage: the header file must hold one line, x-origin-auth: <secret>" >&2
    exit 64
  fi
elif [ "$#" -gt 1 ]; then
  echo "rollout-check: usage: this mode never sends the origin secret: give none" >&2
  exit 64
fi

die() { # exit-code check message
  echo "rollout-check: FAIL $2: $3" >&2
  exit "$1"
}
fail() { die 1 "$1" "$2"; }

# fetch PATH NAME [auth|anon|wrong]: body in $work/NAME.body, headers in
# $work/NAME.head; prints the status ("000" when nothing answered). The
# redirects are not followed. The secret reaches curl only as `-H @file`.
fetch() {
  local path=$1 name=$2 who=${3:-anon} status
  local args=(-sS --max-time "$max_time" -o "$work/$name.body" -D "$work/$name.head" -w '%{http_code}')
  case "$who" in
    wrong) args+=(-H "x-origin-auth: not-the-secret") ;;
    auth) args+=(-H "@$header_file") ;;
    twice) args+=(-H "@$header_file" -H "x-origin-auth: not-the-secret") ;;
  esac
  status="$(curl "${args[@]}" "$url$path" 2>/dev/null)" || true
  [ -n "$status" ] || status=000
  printf '%s' "$status"
}

if [ "$mode" = --refused ]; then
  for probe in anon wrong; do
    for path in / '/?q=x'; do
      status="$(fetch "$path" "r-$probe" "$probe")"
      [ "$status" = 403 ] || fail refused "$path answered $status ($probe request; 403 wanted)"
      [ ! -s "$work/r-$probe.body" ] || fail refused "$path answered a 403 with a body ($probe request): a 403 with a body is not the guard's refusal"
    done
  done
  echo "rollout-check: refused ok"
  exit 0
fi

if [ "$mode" = --access ]; then
  # A rule that cannot be read must say so, not pass for "redirects elsewhere".
  [ -s "$access_pattern_file" ] && [ -r "$access_pattern_file" ] \
    || die 4 access "the pattern file $access_pattern_file is missing, empty or unreadable: the redirect cannot be judged"
  status="$(fetch / access anon)"
  location="$(grep -i '^location:' "$work/access.head" 2>/dev/null | head -n 1 | tr -d '\r' | cut -d' ' -f2-)"
  case "$status" in
    302)
      # A here-string, not a pipe: `grep -q` may close a pipe before the
      # writer is done, and pipefail would read that as a mismatch.
      if grep -Eiq -f "$access_pattern_file" <<<"$location"; then
        echo "rollout-check: access ok"
        exit 0
      fi
      die 4 access "the host redirected (302) to something that is not *.cloudflareaccess.com"
      ;;
    200) die 2 access "Access missing: the host answered 200 without a login (302 to cloudflareaccess.com wanted)" ;;
    403)
      if grep -iq '^cf-mitigated:' "$work/access.head" 2>/dev/null; then
        echo "rollout-check: challenge: Cloudflare challenged this runner (cf-mitigated); inconclusive, the service was not reached" >&2
        exit 3
      fi
      die 4 access "the host answered 403 (302 to cloudflareaccess.com wanted)"
      ;;
    *) die 4 access "the host answered $status (302 to cloudflareaccess.com wanted)" ;;
  esac
fi

# mode revision
status="$(fetch / root auth)"
[ "$status" = 200 ] || fail root "/ answered $status (200 wanted)"
grep -aFq 'PaliLearn' "$work/root.body" || fail root "/ does not name the page"
echo "rollout-check: root ok"

wasm="$(grep -aEo '/pkg/[^"'"'"' <>]*\.wasm' "$work/root.body" | head -n 1)"
[ -n "$wasm" ] || fail wasm "the page links no .wasm"
status="$(fetch "$wasm" wasm auth)"
[ "$status" = 200 ] || fail wasm "$wasm answered $status (200 wanted)"
echo "rollout-check: wasm ok"

for name in root wasm; do
  for part in body head; do
    if grep -aEq -- "$token_pattern" "$work/$name.$part"; then
      fail token "a token shape in the $part of the $name response"
    fi
  done
done
echo "rollout-check: token ok"

# What must never be served is the origin secret itself (learn-pali has no
# API token): grep -F with the value in a file, so it is on no command line
# and in no message.
sed -E 's/^[Xx]-[Oo]rigin-[Aa]uth:[[:space:]]*//; s/[[:space:]]*$//' "$header_file" > "$work/secret.value"
for name in root wasm; do
  for part in body head; do
    if grep -aFq -f "$work/secret.value" "$work/$name.$part"; then
      fail secret "the origin secret is in the $part of the $name response"
    fi
  done
done
echo "rollout-check: secret ok"

for probe in anon wrong twice; do
  status="$(fetch / "g-$probe" "$probe")"
  [ "$status" = 403 ] || fail guard "a request ($probe header) answered $status (403 wanted)"
  [ ! -s "$work/g-$probe.body" ] || fail guard "the 403 for a request ($probe header) has a body"
done
echo "rollout-check: guard ok"
