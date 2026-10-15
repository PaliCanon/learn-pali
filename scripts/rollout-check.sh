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
#   scripts/rollout-check.sh --via-worker URL
#       the public host (dev and prod, task 034): the Worker adds the header,
#       this script sends none (and refuses to be given the secret, as a
#       HEADER_FILE or as ORIGIN_SECRET). Same checks as `revision` that need
#       no secret: `root`, `wasm`, `token`; then `hydrate` (task 040, ui-lib
#       063 T3): headless Chrome loads `/` (the site's one page) and the app
#       must hydrate. Chrome is $CHROME, else the first of google-chrome,
#       google-chrome-stable, chromium, chromium-browser, chrome in PATH, else
#       macOS's Google Chrome.app; none is a usage error (64) before any
#       request. Chrome runs
#       `--headless=new --enable-logging=stderr --virtual-time-budget=15000
#       --dump-dom` (plus `--no-sandbox`: Ubuntu 24.04's AppArmor denies the
#       sandbox to a Chrome outside /opt) under ROLLOUT_CHROME_TIMEOUT seconds
#       (default 60). A `panicked at` or a `RuntimeError` on its stderr (the
#       console), or no `data-hydrated` on `<html>` in the dumped DOM (ui-lib's
#       `hydrate` sets it once done; a timeout or a crash leaves none) is a
#       failed check (1). Chrome cannot add the origin header: this check is
#       the public host's only.
#   scripts/rollout-check.sh --access URL
#       a public host with Cloudflare Access in front (an env that sets
#       `access` in infra/variables.tf; none does today, deploy.yml's step 6b
#       calls it for an env whose ACCESS_<env> is true): it must answer 302 to
#       *.cloudflareaccess.com
#       (scripts/access-redirect-pattern, the one rule infra/scripts/
#       origin-secret check reads too, with ACCESS=1). Sends no secret and
#       follows nothing.
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
#   --via-worker: 0 all good; 1 a check failed (named); 3 Cloudflare answered
#     with a challenge (cf-mitigated): the runner is not allowed, which says
#     nothing about the service; 4 the Worker itself answered 500, whose body
#     ends with `is not configured.` (infra/worker/proxy.js: "The origin
#     secret is not configured."; whitespace, CR and NUL trimmed, first 512
#     bytes): the service was not reached and the remedy is
#     `infra/scripts/origin-secret push <env>`, not a rollback; 6 the edge or
#     the Worker failed, which no rollback cures: no HTTP answer at all
#     (refused, closed, TLS, timeout: curl's 000), a Cloudflare 52x (520 to
#     527), or a 403 (through the Worker the guard refuses only when the
#     Worker's secret is wrong: `origin-secret push <env>`); 7 the host is
#     behind Cloudflare Access (a 301, 302, 303, 307 or 308 whose Location matches
#     scripts/access-redirect-pattern) while the workflow's ACCESS_<env> says
#     false: set it in deploy.yml to match infra, no rollback cures it. Any
#     other 500, 302 or status is the service's, relayed, and is a failed
#     check (1). The `hydrate` check exits 3 too when Chrome is served
#     Cloudflare's challenge page (`_cf_chl_opt`, "Just a moment"): the check
#     is skipped, never passed, and is run by hand before any prod tag; 64
#     usage, an empty or missing scripts/access-redirect-pattern or no Chrome
#     included (checked before any request).
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
chrome_time="${ROLLOUT_CHROME_TIMEOUT:-60}"
# The pages the hydrate check loads: the site has one (`/`, app.rs Routes).
hydrate_paths=(/)
work="$(mktemp -d)"
trap 'rm -rf "$work"' EXIT

usage() {
  echo "usage: rollout-check.sh revision URL HEADER_FILE | --refused URL | --via-worker URL | --access URL" >&2
  exit 64
}

case "$max_time" in '' | *[!0-9]*) echo "rollout-check: ROLLOUT_MAX_TIME must be whole seconds" >&2; exit 64 ;; esac
case "$chrome_time" in '' | *[!0-9]*) echo "rollout-check: ROLLOUT_CHROME_TIMEOUT must be whole seconds" >&2; exit 64 ;; esac

mode="${1:-}"
case "$mode" in
  revision | --refused | --access | --via-worker) shift ;;
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
elif [ "$mode" = --via-worker ] && [ -n "${ORIGIN_SECRET:-}" ]; then
  echo "rollout-check: usage: --via-worker never sends the origin secret: unset ORIGIN_SECRET" >&2
  exit 64
fi

# --via-worker judges a redirect against the Access pattern: a rule that cannot
# be read is a usage error up front, never a failed check (1) that would roll a
# good revision back.
if [ "$mode" = --via-worker ] && { [ ! -s "$access_pattern_file" ] || [ ! -r "$access_pattern_file" ]; }; then
  echo "rollout-check: usage: the pattern file $access_pattern_file is missing, empty or unreadable: --via-worker cannot tell an Access redirect from the service's own" >&2
  exit 64
fi

# --via-worker ends with the hydrate check: its browser is found up front, so
# a runner without one is a usage error (64), never a failed check (1) that
# would roll a good revision back.
chrome=""
if [ "$mode" = --via-worker ]; then
  if [ -n "${CHROME:-}" ]; then
    chrome="$CHROME"
  else
    for candidate in google-chrome google-chrome-stable chromium chromium-browser chrome; do
      if command -v "$candidate" >/dev/null 2>&1; then
        chrome="$(command -v "$candidate")"
        break
      fi
    done
    mac_chrome="/Applications/Google Chrome.app/Contents/MacOS/Google Chrome"
    if [ -z "$chrome" ] && [ -x "$mac_chrome" ]; then chrome="$mac_chrome"; fi
  fi
  if [ -z "$chrome" ] || [ -d "$chrome" ] || [ ! -x "$chrome" ]; then
    echo "rollout-check: usage: --via-worker needs Chrome for the hydrate check: set CHROME to its program, or put google-chrome or chromium in PATH" >&2
    exit 64
  fi
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

# challenged NAME: exit 3 when Cloudflare's challenge, not the service, answered
# (the public host only: a revision URL is not behind Cloudflare).
challenged() {
  [ "$mode" = --via-worker ] || return 0
  if grep -iq '^cf-mitigated:' "$work/$1.head" 2>/dev/null; then
    echo "rollout-check: challenge: Cloudflare challenged this runner (cf-mitigated); the service was not reached" >&2
    exit 3
  fi
}

# workers_500 NAME STATUS: exit 4 when the Worker's own 500 (not the service's)
# answered through the public host. Every Worker 500 body ends with
# `is not configured.` (proxy.js: misconfigured()): matched on that suffix of
# the first 512 bytes, CR and NUL dropped, leading and trailing whitespace
# trimmed, so a CRLF, a trailing newline or padding added on the way does not
# hide it.
workers_500() {
  [ "$mode" = --via-worker ] && [ "$2" = 500 ] || return 0
  local body
  body="$(head -c 512 "$work/$1.body" 2>/dev/null | tr -d '\r\0')"
  body="${body#"${body%%[![:space:]]*}"}"
  body="${body%"${body##*[![:space:]]}"}"
  if [ "${body%is not configured.}" != "$body" ]; then
    echo "rollout-check: worker-500: the Worker answered 500 (no origin secret or origin configured: run infra/scripts/origin-secret push); the service was not reached" >&2
    exit 4
  fi
}

# edge_failure NAME STATUS: exit 6 when the edge or the Worker failed, not the
# revision: no HTTP answer (000), a Cloudflare 52x (520 to 527), or a 403
# (called after `challenged`, so a cf-mitigated 403 is already exit 3).
edge_failure() {
  [ "$mode" = --via-worker ] || return 0
  case "$2" in
    000) die 6 edge "$1: the edge or the Worker failed, no HTTP answer (refused, closed, TLS or timeout); the revision was not judged, no rollback cures it" ;;
    52[0-7]) die 6 edge "$1: the edge or the Worker failed to reach the service, Cloudflare answered $2; the revision was not judged, no rollback cures it" ;;
    403) die 6 edge "$1: the edge or the Worker was refused (403): the Worker's origin secret may be wrong, run infra/scripts/origin-secret push <env>" ;;
  esac
}

# access_in_front NAME STATUS: exit 7 when the public host answers with the
# Access login redirect (a 301, 302, 303, 307 or 308 whose Location matches
# the pattern file the --access mode reads; the file is checked up front): the
# host is behind Cloudflare Access while the workflow says it is not. The
# revision is not at fault; a redirect elsewhere is the service's and stays a
# failed check (1).
access_in_front() {
  [ "$mode" = --via-worker ] || return 0
  case "$2" in 301 | 302 | 303 | 307 | 308) ;; *) return 0 ;; esac
  local location
  location="$(grep -i '^location:' "$work/$1.head" 2>/dev/null | head -n 1 | tr -d '\r' | cut -d' ' -f2-)"
  if grep -Eiq -f "$access_pattern_file" <<<"$location"; then
    die 7 access "the host is behind Cloudflare Access but the workflow's ACCESS setting for this env is false: set ACCESS_<env> in deploy.yml to match infra (infra/variables.tf envs.<env>.access); the revision was not judged, no rollback cures it"
  fi
}

# want_200 CHECK NAME LABEL STATUS: the answer must be 200; else a challenge
# (3), the Worker's own 500 (4), Access in front (7) or an edge failure (6) is
# named before the failed check (1).
want_200() {
  [ "$4" = 200 ] && return 0
  challenged "$2"
  workers_500 "$2" "$4"
  access_in_front "$2" "$4"
  edge_failure "$3" "$4"
  fail "$1" "$3 answered $4 (200 wanted)"
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

# mode revision, or --via-worker (the Worker adds the header: none is sent)
who=auth
[ "$mode" = --via-worker ] && who=anon
status="$(fetch / root "$who")"
want_200 root root / "$status"
grep -aFq 'PaliLearn' "$work/root.body" || fail root "/ does not name the page"
echo "rollout-check: root ok"

wasm="$(grep -aEo '/pkg/[^"'"'"' <>]*\.wasm' "$work/root.body" | head -n 1)"
[ -n "$wasm" ] || fail wasm "the page links no .wasm"
status="$(fetch "$wasm" wasm "$who")"
want_200 wasm wasm "$wasm" "$status"
echo "rollout-check: wasm ok"

for name in root wasm; do
  for part in body head; do
    if grep -aEq -- "$token_pattern" "$work/$name.$part"; then
      fail token "a token shape in the $part of the $name response"
    fi
  done
done
echo "rollout-check: token ok"

# limited SECONDS CMD...: CMD, killed after SECONDS (124 from timeout, 142
# from perl's alarm where there is no timeout: macOS).
limited() {
  local seconds=$1
  shift
  if command -v timeout >/dev/null 2>&1; then
    timeout "$seconds" "$@"
  else
    perl -e 'alarm shift; exec @ARGV or exit 127' "$seconds" "$@"
  fi
}

# check_hydrate PATH: Chrome loads the host's PATH; the app must hydrate
# (ui-lib's `hydrate` marks <html data-hydrated>) with no panic on the
# console. Its DOM and console go to files, never through a pipe.
hydrate_n=0
check_hydrate() {
  hydrate_n=$((hydrate_n + 1))
  local dom="$work/hydrate-$hydrate_n.dom" console="$work/hydrate-$hydrate_n.err" rc=0
  limited "$chrome_time" "$chrome" --headless=new --no-sandbox --disable-gpu --no-first-run \
    --no-default-browser-check --user-data-dir="$work/chrome-$hydrate_n" --enable-logging=stderr \
    --virtual-time-budget=15000 --dump-dom "$url$1" < /dev/null > "$dom" 2> "$console" || rc=$?
  if grep -aEq 'panicked at|RuntimeError' "$console"; then
    # Read apart from the message: bash 3.2 (macOS) mis-parses a quote
    # inside $( ) inside a double-quoted string.
    local first
    first="$(grep -aEo -m 1 '(panicked at|RuntimeError)[^"]{0,160}' "$console" | head -n 1)"
    fail hydrate "$1 panicked in the browser: $first"
  fi
  if grep -aEq '<html[^>]*[[:space:]]data-hydrated([=[:space:]/>]|$)' "$dom"; then
    return 0
  fi
  if grep -aEq '_cf_chl_opt|<title>Just a moment' "$dom"; then
    echo "rollout-check: challenge: Cloudflare challenged the browser on $1; the hydrate check was skipped, not passed: run it by hand before any prod tag" >&2
    exit 3
  fi
  case "$rc" in
    124 | 142) fail hydrate "Chrome did not finish $1 in $chrome_time s: no data-hydrated on <html>" ;;
  esac
  fail hydrate "$1 has no data-hydrated on <html> (Chrome exit $rc): the app did not hydrate"
}

if [ "$mode" = --via-worker ]; then
  for path in "${hydrate_paths[@]}"; do
    check_hydrate "$path"
  done
  echo "rollout-check: hydrate ok"
  exit 0
fi

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
