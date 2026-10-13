#!/usr/bin/env bash
# Tests for infra/scripts/origin-secret (task 031; 034: no Access on dev): `gcloud` and `curl` are
# stubs first on PATH that log their argv and stdin. The contract under
# test: the secret value travels on stdin only (never in an argument, never
# on screen), `create` is idempotent, `push` checks the HTTP code, `check`
# prints OK or "not OK" with the name of the failing check.
# Run: bash infra/scripts/origin-secret.test.sh
set -uo pipefail
unset ACCESS

here=$(cd "$(dirname "$0")" && pwd)
script="$here/origin-secret"
work=$(mktemp -d)
trap 'rm -rf "$work"' EXIT
bin="$work/bin"
log="$work/log"
mkdir -p "$bin" "$log"

VALUE="stubbed-origin-secret-value-0123456789abcdef"
CFTOKEN="cf-token-for-tests-fedcba9876543210"

# gcloud: `versions list` prints $STUB_VERSIONS; `versions access` prints
# the known value; `versions add` records its stdin.
cat >"$bin/gcloud" <<STUB
#!/usr/bin/env bash
printf '%s\n' "\$*" >>"$log/gcloud.argv"
case "\$*" in
  *"versions list"*) printf '%s' "\${STUB_VERSIONS:-}" ;;
  *"versions access"*) printf '%s' "$VALUE" ;;
  *"versions add"*) cat >"$log/gcloud.add.stdin" ;;
esac
STUB
# curl: logs argv; a `--config -` call logs its stdin config and answers by
# its method (PUT -> the status code, GET -> a body). Any other call is the
# host check: it answers "<status> <redirect url>" (the script asks for
# `%{http_code} %{redirect_url}`).
# A GET answers its body, a newline and the status (`STUB_GET_CODE`, 200),
# as the script's `write-out = "\n%{http_code}"` asks.
export STUB_LOGDIR="$log"
cat >"$bin/curl" <<'STUB'
#!/usr/bin/env bash
printf '%s\n' "$*" >>"$STUB_LOGDIR/curl.argv"
default='{"result":[{"name":"ORIGIN_AUTH","type":"secret_text"}],"success":true}'
if [ "${1:-}" = "--config" ] && [ "${2:-}" = "-" ]; then
  cfg=$(cat)
  printf '%s\n---\n' "$cfg" >>"$STUB_LOGDIR/curl.stdin"
  case "$cfg" in
    *'request = "PUT"'*) printf '%s' "${STUB_PUT_CODE:-200}" ;;
    *)
      # A big answer comes from a file (an env value is capped near 128 KB).
      if [ -n "${STUB_WORKER_SECRETS_FILE:-}" ]; then cat "$STUB_WORKER_SECRETS_FILE"; printf '\n%s' "${STUB_GET_CODE:-200}"
      else printf '%s\n%s' "${STUB_WORKER_SECRETS:-$default}" "${STUB_GET_CODE:-200}"; fi ;;
  esac
else
  printf '%s %s' "${STUB_HOST_CODE:-302}" "${STUB_HOST_LOCATION-https://lucky-cake-cc7c.cloudflareaccess.com/cdn-cgi/access/login/learn.dev.palicanon.app?kid=abc}"
fi
STUB
real_grep=$(command -v grep)
cat >"$bin/grep" <<STUB
#!/usr/bin/env bash
# With STUB_EARLY_GREP set, a grep that reads standard input stops after its
# first 4096 bytes, as GNU grep -q stops at its first match: a writer still
# sending then dies of SIGPIPE (macOS's grep drains the pipe and hides it).
# Without it, the real grep. A grep given a file (the last argument, unless
# it follows -f) is untouched.
args=("\$@")
n=\${#args[@]}
last="\${args[\$((n - 1))]}"
prev="\${args[\$((n - 2))]:-}"
if [ -n "\${STUB_EARLY_GREP:-}" ] && { [ ! -e "\$last" ] || [ "\$prev" = -f ]; }; then
  head -c 4096 | "$real_grep" "\$@"
  exit \$?
fi
exec "$real_grep" "\$@"
STUB
chmod +x "$bin/gcloud" "$bin/curl" "$bin/grep"

pass=0
fail=0
ok() { pass=$((pass + 1)); echo "ok   $1"; }
ko() { fail=$((fail + 1)); echo "FAIL $1"; }
check() { # check "name" command...   (passes when the command succeeds)
  local name=$1
  shift
  if "$@"; then ok "$name"; else ko "$name"; fi
}

reset() { rm -f "$log"/*; : >"$log/gcloud.argv"; : >"$log/curl.argv"; }
# run <args...> -> $out (stdout+stderr), $code
run() {
  out=$(env PATH="$bin:$PATH" CLOUDFLARE_API_TOKEN="${TOKEN-$CFTOKEN}" "$script" "$@" 2>&1)
  code=$?
}
out_has() { printf '%s' "$out" | grep -q -- "$1"; }
out_is_ok() { [ "$out" = "OK" ]; }
refused() { [ "$code" -ne 0 ] && out_has "not OK" && { [ -z "${1:-}" ] || out_has "$1"; }; }
no_value_anywhere() {
  ! grep -qF "$VALUE" "$log/gcloud.argv" "$log/curl.argv" 2>/dev/null && ! printf '%s' "$out" | grep -qF "$VALUE"
}
no_cf_token_in_argv() { ! grep -qF "$CFTOKEN" "$log/gcloud.argv" "$log/curl.argv" 2>/dev/null && ! printf '%s' "$out" | grep -qF "$CFTOKEN"; }

# ---------------------------------------------------------------- create
reset
STUB_VERSIONS="" run create dev
check "create: exits 0" test "$code" -eq 0
check "create: adds exactly one version" test "$(grep -c 'versions add' "$log/gcloud.argv")" -eq 1
check "create: on the right secret and project" grep -q 'versions add learn-pali-origin-secret-dev --project palicanon-app --data-file=-' "$log/gcloud.argv"
check "create: the value is 64 hex characters on stdin" bash -c "grep -Eqx '[0-9a-f]{64}' '$log/gcloud.add.stdin'"
generated=$(tr -d '\n' <"$log/gcloud.add.stdin")
check "create: the generated value is on no argv" bash -c "! grep -qF '$generated' '$log/gcloud.argv'"
check "create: the generated value is not on screen" bash -c "! printf '%s' '$out' | grep -qF '$generated'"

reset
STUB_VERSIONS="1" run create dev
check "create: does nothing when a version exists (exit 0)" test "$code" -eq 0
check "create: no version added when one exists" test "$(grep -c 'versions add' "$log/gcloud.argv")" -eq 0

reset
STUB_VERSIONS="" run create prod
check "create prod: its own secret" grep -q 'versions add learn-pali-origin-secret-prod ' "$log/gcloud.argv"

# ---------------------------------------------------------------- push
reset
run push dev
check "push: exits 0 on HTTP 200" test "$code" -eq 0
check "push: reads the latest version" grep -q 'versions access latest --secret learn-pali-origin-secret-dev --project palicanon-app' "$log/gcloud.argv"
check "push: PUTs to the dev Worker's secrets" grep -q 'workers/scripts/learn-pali-dev/secrets' "$log/curl.stdin"
check "push: names the binding ORIGIN_AUTH" grep -q 'ORIGIN_AUTH' "$log/curl.stdin"
check "push: the value reaches curl on stdin" grep -qF "$VALUE" "$log/curl.stdin"
check "push: curl is called with --config - and nothing else" bash -c "test -s '$log/curl.argv' && test \$(grep -vc -- '^--config -\$' '$log/curl.argv') -eq 0"
check "push: the call has a time limit" grep -q 'max-time' "$log/curl.stdin"
check "push: the value is on no argv and not on screen" no_value_anywhere
check "push: the Cloudflare token is on no argv and not on screen" no_cf_token_in_argv

reset
STUB_PUT_CODE=201 run push dev
check "push: any 2xx is a success" test "$code" -eq 0

for refusal in 403 500; do
  reset
  STUB_PUT_CODE=$refusal run push dev
  check "push: exits non-zero when Cloudflare answers $refusal" test "$code" -ne 0
  check "push: $refusal is named, not the value" out_has "$refusal"
  check "push: $refusal output holds no value" no_value_anywhere
done

reset
TOKEN="" run push dev
check "push: no Cloudflare token -> non-zero" test "$code" -ne 0
check "push: no Cloudflare token -> no call at all" bash -c "! test -s '$log/curl.argv' && ! grep -q 'versions access' '$log/gcloud.argv'"
unset TOKEN

reset
run push prod
check "push prod: the prod Worker" grep -q 'workers/scripts/learn-pali-prod/secrets' "$log/curl.stdin"

# ---------------------------------------------------------------- check (dev)
reset
STUB_VERSIONS=1 ACCESS=1 run check dev
check "check: OK when the version, the Worker's secret and the Access redirect agree" test "$code" -eq 0
check "check: prints OK and nothing else" out_is_ok
check "check: prints no value" no_value_anywhere
check "check: the Cloudflare token is on no argv and not on screen" no_cf_token_in_argv
check "check: asks the dev host" grep -q 'https://learn.dev.palicanon.app/' "$log/curl.argv"
check "check: the host call has a time limit" grep -q -- '--max-time' "$log/curl.argv"
check "check: lists the Worker's secrets (names)" grep -q 'workers/scripts/learn-pali-dev/secrets' "$log/curl.stdin"

reset
STUB_VERSIONS="" ACCESS=1 run check dev
check "check: not OK without a version, named 'version'" refused version
reset
STUB_VERSIONS="1" STUB_WORKER_SECRETS='{"result":[],"success":true}' ACCESS=1 run check dev
check "check: not OK when the Worker has no ORIGIN_AUTH, named 'worker-secret'" refused worker-secret
reset
STUB_VERSIONS="1" STUB_WORKER_SECRETS='{"result":[{"name":"OTHER","type":"secret_text"}],"success":true}' ACCESS=1 run check dev
check "check: another secret name is not ORIGIN_AUTH" refused worker-secret
reset
STUB_VERSIONS="1" STUB_HOST_CODE=200 STUB_HOST_LOCATION="" ACCESS=1 run check dev
check "check: 200 -> not OK (Access missing), named 'access-redirect'" refused access-redirect
check "check: 200 says Access is missing" out_has "Access"
reset
STUB_VERSIONS="1" STUB_HOST_CODE=500 STUB_HOST_LOCATION="" ACCESS=1 run check dev
check "check: 500 -> not OK, named 'access-redirect', 500 quoted" refused 500
check "check: 500 is named 'access-redirect'" out_has access-redirect
reset
STUB_VERSIONS="1" STUB_HOST_CODE=522 STUB_HOST_LOCATION="" ACCESS=1 run check dev
check "check: 522 -> not OK" refused 522
reset
STUB_VERSIONS="1" STUB_HOST_CODE=000 STUB_HOST_LOCATION="" ACCESS=1 run check dev
check "check: no answer at all -> not OK" refused access-redirect
reset
STUB_VERSIONS="1" STUB_HOST_CODE=302 STUB_HOST_LOCATION="https://evil.example/cloudflareaccess.com" ACCESS=1 run check dev
check "check: a 302 elsewhere than *.cloudflareaccess.com -> not OK" refused access-redirect
reset
STUB_VERSIONS="1" STUB_HOST_CODE=302 STUB_HOST_LOCATION="https://cloudflareaccess.com.evil.example/x" ACCESS=1 run check dev
check "check: a lookalike host -> not OK" refused access-redirect
reset
STUB_VERSIONS="1" STUB_HOST_CODE=302 STUB_HOST_LOCATION="https://evil.example/x.cloudflareaccess.com/y" ACCESS=1 run check dev
check "check: cloudflareaccess.com in the path of another host -> not OK" refused access-redirect
reset
STUB_VERSIONS="1" STUB_HOST_CODE=302 STUB_HOST_LOCATION="http://lucky-cake-cc7c.cloudflareaccess.com/x" ACCESS=1 run check dev
check "check: a non-https redirect -> not OK" refused access-redirect
reset
STUB_VERSIONS="1" STUB_HOST_CODE=403 STUB_HOST_LOCATION="" ACCESS=1 run check dev
check "check: 403 -> not OK" refused 403
reset
STUB_VERSIONS="1" STUB_HOST_CODE=200 STUB_HOST_LOCATION="" STUB_WORKER_SECRETS='{"result":[],"success":true}' ACCESS=1 run check dev
check "check: the first failing check is the one named" refused worker-secret
check "check: not OK output holds no value" no_value_anywhere

reset
STUB_VERSIONS="1" STUB_HOST_CODE=302 STUB_HOST_LOCATION="https://evilcloudflareaccess.com/x" ACCESS=1 run check dev
check "check: evilcloudflareaccess.com is not *.cloudflareaccess.com -> not OK" refused access-redirect
reset
STUB_VERSIONS="1" STUB_HOST_CODE=302 STUB_HOST_LOCATION="https://cloudflareaccess.com/x" ACCESS=1 run check dev
check "check: the apex without a team name -> not OK" refused access-redirect
reset
STUB_VERSIONS="1" STUB_HOST_CODE=302 STUB_HOST_LOCATION="https://X.CloudflareAccess.com/cdn-cgi/access/login/learn.dev.palicanon.app?kid=abc" ACCESS=1 run check dev
check "check: an uppercase host and path pass" out_is_ok
reset
STUB_VERSIONS="1" STUB_HOST_CODE=302 STUB_HOST_LOCATION="https://team.cloudflareaccess.com?kid=1" ACCESS=1 run check dev
check "check: a query right after the host passes" out_is_ok

# The pattern is the one file scripts/access-redirect-pattern, shared with
# rollout-check.sh --access: a copy of the tree with another pattern follows it.
check "pattern: the shared file is one line" test "$(grep -c '' "$here/../../scripts/access-redirect-pattern" 2>/dev/null)" -eq 1
mkdir -p "$work/alt/infra/scripts" "$work/alt/scripts"
cp "$script" "$work/alt/infra/scripts/origin-secret"
printf '%s\n' '^https://never\.example/' >"$work/alt/scripts/access-redirect-pattern"
reset
out=$(env PATH="$bin:$PATH" CLOUDFLARE_API_TOKEN="$CFTOKEN" STUB_VERSIONS=1 ACCESS=1 "$work/alt/infra/scripts/origin-secret" check dev 2>&1)
code=$?
check "pattern: origin-secret reads the file two folders up, scripts/access-redirect-pattern" refused access-redirect

reset
STUB_VERSIONS=1 STUB_GET_CODE=403 STUB_WORKER_SECRETS='{"success":false,"errors":[{"code":10000,"message":"Authentication error"}],"result":null}' ACCESS=1 run check dev
check "check: a Cloudflare 403 -> not OK" refused "Cloudflare API error 403"
check "check: and it is not blamed on a missing ORIGIN_AUTH" bash -c "! printf '%s' '$out' | grep -q 'has no ORIGIN_AUTH'"
check "check: and it is still named 'worker-secret'" out_has worker-secret
reset
STUB_VERSIONS=1 STUB_GET_CODE=500 STUB_WORKER_SECRETS='' ACCESS=1 run check dev
check "check: a Cloudflare 500 -> not OK, the code named" refused "Cloudflare API error 500"
reset
STUB_VERSIONS=1 STUB_GET_CODE=200 STUB_WORKER_SECRETS='{"success": false,"errors":[],"result":[]}' ACCESS=1 run check dev
check "check: HTTP 200 with success false -> not OK as a Cloudflare error" refused "Cloudflare API error 200"
reset
STUB_VERSIONS=1 STUB_GET_CODE=200 STUB_WORKER_SECRETS='{"result":[],"success":true}' ACCESS=1 run check dev
check "check: a clean answer without ORIGIN_AUTH is still 'no ORIGIN_AUTH'" refused "has no ORIGIN_AUTH"

# DT2: a pattern file that cannot be read is named, never "redirects elsewhere".
alt_run() { # alt_run DIR -> $out, $code (the script copy under DIR/infra/scripts)
  out=$(env PATH="$bin:$PATH" CLOUDFLARE_API_TOKEN="$CFTOKEN" STUB_VERSIONS=1 ACCESS=1 "$1/infra/scripts/origin-secret" check dev 2>&1)
  code=$?
}
mkdir -p "$work/nopat/infra/scripts"
cp "$script" "$work/nopat/infra/scripts/origin-secret"
reset
alt_run "$work/nopat"
check "pattern: a missing file -> not OK, named 'access-redirect'" refused access-redirect
check "pattern: a missing file is named" out_has "access-redirect-pattern"
check "pattern: a missing file is not blamed on the redirect" bash -c "! printf '%s' '$out' | grep -q 'redirects elsewhere'"
mkdir -p "$work/nopat/scripts"
: >"$work/nopat/scripts/access-redirect-pattern"
reset
alt_run "$work/nopat"
check "pattern: an empty file -> not OK, the file named" refused access-redirect-pattern
if [ "$(id -u)" != 0 ]; then
  printf '%s\n' '^https://' >"$work/nopat/scripts/access-redirect-pattern"
  chmod 000 "$work/nopat/scripts/access-redirect-pattern"
  reset
  alt_run "$work/nopat"
  check "pattern: an unreadable file -> not OK, the file named" refused access-redirect-pattern
  check "pattern: an unreadable file is not blamed on the redirect" bash -c "! printf '%s' '$out' | grep -q 'redirects elsewhere'"
  chmod 600 "$work/nopat/scripts/access-redirect-pattern"
fi
# Prod is public and needs no pattern: the tree WITHOUT the pattern file (the
# empty/unreadable file is removed first) answers OK for `check prod`.
rm -f "$work/nopat/scripts/access-redirect-pattern"
check "pattern: the tree for the prod check holds no pattern file" test ! -e "$work/nopat/scripts/access-redirect-pattern"
reset
out=$(env PATH="$bin:$PATH" CLOUDFLARE_API_TOKEN="$CFTOKEN" STUB_VERSIONS=1 STUB_HOST_CODE=200 STUB_HOST_LOCATION="" "$work/nopat/infra/scripts/origin-secret" check prod 2>&1)
code=$?
check "pattern: prod does not need the pattern file (exit 0)" test "$code" -eq 0
check "pattern: prod does not need the pattern file" out_is_ok

# DT4: an answer larger than the pipe buffer, ORIGIN_AUTH early in it: a
# `printf | grep -q` under pipefail dies of SIGPIPE when grep stops at the
# match. The grep shim (STUB_EARLY_GREP) stops reading as GNU grep does.
big="$work/big-answer.json"
python3 - "$big" <<'PY'
import sys
head = '{"result":[{"name":"ORIGIN_AUTH","type":"secret_text"},'
pad = ",".join('{"name":"PAD_%06d","type":"secret_text"}' % i for i in range(6000))
open(sys.argv[1], "w").write(head + pad + '],"success":true}')
PY
check "big answer: the stub answer is over 200 KB" test "$(wc -c <"$big")" -gt 200000
for round in 1 2 3 4 5; do
  reset
  STUB_EARLY_GREP=1 STUB_VERSIONS=1 STUB_WORKER_SECRETS_FILE="$big" ACCESS=1 run check dev
  check "big answer ($round): OK, no false Cloudflare error or missing ORIGIN_AUTH" out_is_ok
done
reset
{ printf '{"result":['; python3 -c 'print(",".join("{\"name\":\"PAD_%06d\"}" % i for i in range(6000)), end="")'; printf '],"success":true}'; } >"$work/big-without.json"
STUB_EARLY_GREP=1 STUB_VERSIONS=1 STUB_WORKER_SECRETS_FILE="$work/big-without.json" ACCESS=1 run check dev
check "big answer without ORIGIN_AUTH: still 'no ORIGIN_AUTH'" refused "has no ORIGIN_AUTH"
reset
{ printf '{"success":false,"errors":[{"message":"x"}],"result":['; python3 -c 'print(",".join("{\"name\":\"PAD_%06d\"}" % i for i in range(6000)), end="")'; printf ']}'; } >"$work/big-false.json"
STUB_EARLY_GREP=1 STUB_VERSIONS=1 STUB_WORKER_SECRETS_FILE="$work/big-false.json" ACCESS=1 run check dev
check "big answer with success false: a Cloudflare error" refused "Cloudflare API error 200"
check "pipes: no printf piped into grep -q in the script" bash -c "! grep -En 'printf .*[|] *grep -[A-Za-z]*q' '$script'"

reset
TOKEN="" STUB_VERSIONS=1 ACCESS=1 run check dev
check "check: no Cloudflare token -> non-zero, no Cloudflare call" bash -c "test $code -ne 0 && ! test -s '$log/curl.argv'"
unset TOKEN

# ---------------------------------------------------------------- check (dev, public)
# 034: dev has no Access (public behind the origin guard, as search-dev): the
# host answers 200 through the Worker; the 302-to-Access branch above only
# runs with ACCESS=1.
reset
STUB_VERSIONS=1 STUB_HOST_CODE=200 STUB_HOST_LOCATION="" run check dev
check "public dev: 200 through the Worker is OK" out_is_ok
check "public dev: asks the dev host" grep -q 'https://learn.dev.palicanon.app/' "$log/curl.argv"
reset
STUB_VERSIONS=1 run check dev
check "public dev: the default stub answer (302 to Access) is not OK, named 'host'" refused "check 'host' failed"
check "public dev: the 302 is quoted" out_has "HTTP 302"
check "public dev: and ACCESS=1 is named as the way to expect Access" out_has "ACCESS=1"
check "public dev: and it is not the Access check" bash -c "! printf '%s' '$out' | grep -q access-redirect"
reset
STUB_VERSIONS=1 STUB_HOST_CODE=500 STUB_HOST_LOCATION="" run check dev
check "public dev: 500 -> not OK, named 'host', 500 quoted" refused "check 'host' failed.*500"
reset
STUB_VERSIONS=1 STUB_HOST_CODE=403 STUB_HOST_LOCATION="" run check dev
check "public dev: 403 -> not OK, named 'host'" refused "check 'host' failed.*403"
reset
STUB_VERSIONS=1 STUB_HOST_CODE=000 STUB_HOST_LOCATION="" run check dev
check "public dev: no answer -> not OK, named 'host'" refused "check 'host' failed"
reset
STUB_VERSIONS="" STUB_HOST_CODE=200 STUB_HOST_LOCATION="" run check dev
check "public dev: the version check still comes first" refused version
reset
STUB_VERSIONS=1 STUB_HOST_CODE=200 STUB_HOST_LOCATION="" STUB_WORKER_SECRETS='{"result":[],"success":true}' run check dev
check "public dev: the worker-secret check still comes before the host" refused worker-secret
reset
STUB_VERSIONS=1 STUB_HOST_CODE=200 STUB_HOST_LOCATION="" ACCESS=1 run check dev
check "ACCESS=1 on dev: a 200 is Access missing, named 'access-redirect'" refused access-redirect
reset
STUB_VERSIONS=1 STUB_HOST_CODE=200 STUB_HOST_LOCATION="" ACCESS=0 run check dev
check "ACCESS=0 is not ACCESS=1: the public check (200 is OK)" out_is_ok
# The public check reads no pattern file: the tree without it answers OK.
mkdir -p "$work/pub/infra/scripts"
cp "$script" "$work/pub/infra/scripts/origin-secret"
check "public dev: the tree holds no pattern file" test ! -e "$work/pub/scripts/access-redirect-pattern"
reset
out=$(env PATH="$bin:$PATH" CLOUDFLARE_API_TOKEN="$CFTOKEN" STUB_VERSIONS=1 STUB_HOST_CODE=200 STUB_HOST_LOCATION="" "$work/pub/infra/scripts/origin-secret" check dev 2>&1)
code=$?
check "public dev: does not need the pattern file (exit 0)" test "$code" -eq 0
check "public dev: does not need the pattern file (OK)" out_is_ok

# ---------------------------------------------------------------- check (prod: public, no Access)
reset
STUB_VERSIONS=1 STUB_HOST_CODE=200 STUB_HOST_LOCATION="" run check prod
check "check prod: asks the prod host" grep -q 'https://learn.palicanon.app/' "$log/curl.argv"
check "check prod: public, so 200 is OK" out_is_ok
reset
STUB_VERSIONS=1 STUB_HOST_CODE=500 STUB_HOST_LOCATION="" run check prod
check "check prod: 500 -> not OK" refused 500

# ---------------------------------------------------------------- usage
reset
run create staging
check "usage: an unknown environment is refused" test "$code" -eq 2
run frobnicate dev
check "usage: an unknown command is refused" test "$code" -eq 2
run
check "usage: no argument is refused" test "$code" -eq 2

# ---------------------------------------------------------------- the comment
# The rotation order is written in the script's header and in infra/README.md:
# the Worker's new value first (`push`), then a new revision.
rotation=$(sed -n '/Rotation/,/^set /p' "$script" | tr -d '#\n' | tr -s ' ')
check "comment: the rotation paragraph is found" test -n "$rotation"
check "comment: the rotation pushes to the Worker before the new revision" \
  bash -c '[[ "$1" =~ push.*(new|start).*revision ]]' _ "$rotation"
check "comment: the rotation does not roll the revision before the push" \
  bash -c '! [[ "$1" =~ (roll|start).*revision.*then.*push ]]' _ "$rotation"

echo
echo "origin-secret.test.sh: $pass passed, $fail failed"
test "$fail" -eq 0
