#!/usr/bin/env bash
# Tests for scripts/rollout-check.sh, app/Dockerfile and .github/workflows/
# deploy.yml (task 031, slice R; 034: --via-worker). Run: scripts/rollout-check.test.sh
#
#   1. rollout-check.sh against a python3 fake server that plays every way a
#      deployment can be wrong: each check has a server that must fail it, the
#      failure names the check, and the origin secret is in no output and no
#      curl argument (a curl shim records them).
#   2. the Dockerfile: no ssh mount, no known_hosts, the git-source guard
#      (run against lockfiles), non-root, LEPTOS_ENV=PROD.
#   3. deploy.yml: parses; its steps are run with a stub gcloud and a stub
#      rollout-check to prove PREV, the skip, the no-traffic deploy, the
#      secret step, the rollback (exit 5) and the host check (--via-worker for
#      every env: retries on 1, a warning on 3, no rollback on 4 or 6, one on 1;
#      an env with Access runs --access: never a rollback), and ci.yml's `infra`
#      job (034 round 1: it runs the script tests, the Worker tests, tofu).
#
# Needs bash, curl, python3 with PyYAML, jq. Exit 0 when every assertion held.
set -uo pipefail

here="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
root="$(cd "$here/.." && pwd)"
script="$here/rollout-check.sh"
dockerfile="$root/app/Dockerfile"
workflow="$root/.github/workflows/deploy.yml"

work="$(mktemp -d)"
SRV_PID=""
cleanup() { [ -z "$SRV_PID" ] || kill "$SRV_PID" 2>/dev/null; rm -rf "$work"; }
trap cleanup EXIT

SECRET="origin_canary_rollout_9d41c7"
TOKEN_HEAD="pc_dev_A1b2C3d4E5f6"

checks=0
failed=0
pass() { checks=$((checks + 1)); echo "ok   - $1"; }
flunk() { checks=$((checks + 1)); failed=$((failed + 1)); echo "FAIL - $1"; }
assert_eq() { # name want got
  if [ "$2" = "$3" ]; then pass "$1"; else flunk "$1 (want '$2', got '$3')"; fi
}
assert_match() { # name pattern text
  if grep -Eq -- "$2" <<<"$3"; then pass "$1"; else flunk "$1 (no match for /$2/ in: $(printf '%s' "$3" | head -c 300))"; fi
}
assert_absent() { # name needle text
  if grep -Fq -- "$2" <<<"$3"; then flunk "$1 (found '$2')"; else pass "$1"; fi
}
assert_file_has() { # name file pattern
  if grep -Eq -- "$3" "$2" 2>/dev/null; then pass "$1"; else flunk "$1 (/$3/ not in $2)"; fi
}
assert_file_lacks() { # name file pattern
  if grep -Eq -- "$3" "$2" 2>/dev/null; then flunk "$1 (/$3/ found in $2)"; else pass "$1"; fi
}

# ------------------------------------------------------------ the fake server
cat > "$work/fake.py" <<'PY'
import http.server, socket, socketserver, sys, time

scenario, portfile, logfile = sys.argv[1:4]
SECRET = "origin_canary_rollout_9d41c7"
TOKEN = "pc_dev_A1b2C3d4E5f6G7h8I9j0K1l2M3n4O5p6Q7r8S9t0U1v"
assert len(TOKEN) == 7 + 43
WASM = "/pkg/learn-pali.4f2a9c.wasm"
ACCESS = "https://team.cloudflareaccess.com/cdn-cgi/access/login/learn.dev.palicanon.app?kid=1"


def page(name, link, extra):
    return ('<!DOCTYPE html><html><head><title>%s</title>%s</head><body>%s</body></html>'
            % (name, '<link rel="preload" href="%s" as="fetch" type="application/wasm">' % link if link else "", extra)).encode()


WORKER500 = b"The origin secret is not configured."


def via(path, who, s):
    """The public host through the Worker (--via-worker): the Worker adds the
    header, so a request that carries one is the bug under test."""
    if s == "via_hang": time.sleep(30)
    if who != "none": return 500, {}, b"a request came with an x-origin-auth header"
    worker500 = {"via_worker500": WORKER500, "via_worker500_crlf": WORKER500 + b"\r\n",
                 "via_worker500_pad": b"  \n" + WORKER500 + b" \r\n\n",
                 "via_worker500_origin": b"The origin is not configured.",
                 "via_worker500_public": b"The public origin is not configured."}
    if s.startswith("via_edge_") and path == "/": return int(s[len("via_edge_"):]), {}, b""
    if s.startswith("via_edgewasm_") and path == WASM: return int(s[len("via_edgewasm_"):]), {}, b""
    if s == "via_403wasm" and path == WASM: return 403, {}, b""
    if path == "/":
        if s in worker500: return 500, {"content-type": "text/plain"}, worker500[s]
        if s == "via_root500": return 500, {}, b""
        if s == "via_root500_service": return 500, {}, b"Internal Server Error"
        if s == "via_500_suffix_middle": return 500, {}, WORKER500 + b" Try again later."
        if s == "via_challenge": return 403, {"cf-mitigated": "challenge"}, b"Just a moment..."
        if s == "via_403": return 403, {}, b""
        if s == "via_302": return 302, {"location": ACCESS}, b""
        if s == "via_302_other": return 302, {"location": "https://evil.example/login"}, b""
        if s in ("via_301", "via_303", "via_307", "via_308"): return int(s[4:]), {"location": ACCESS}, b""
        if s == "via_307_other": return 307, {"location": "https://evil.example/login"}, b""
        if s == "via_root404": return 404, {}, b""
        name = "Something else" if s == "via_noname" else "PaliLearn"
        link = "" if s == "via_nowasm" else WASM
        extra = TOKEN if s == "via_root_token" else ""
        head = {"content-type": "text/html"}
        # a decoy: the check must read the failing request's own head
        if s == "via_wasm_302_other": head["location"] = ACCESS
        if s == "via_wasm_302": head["location"] = "https://evil.example/root"
        if s == "via_root_token_head": head["x-leak"] = TOKEN
        return 200, head, page(name, link, extra)
    if path == WASM:
        if s == "via_wasm_302": return 302, {"location": ACCESS}, b""
        if s == "via_wasm_302_other": return 302, {"location": "https://evil.example/login"}, b""
        if s == "via_wasm404": return 404, {}, b""
        if s == "via_wasm_worker500": return 500, {}, WORKER500
        if s == "via_challenge_wasm": return 403, {"cf-mitigated": "challenge"}, b"Just a moment..."
        if s == "via_wasm_500": return 500, {}, b"Internal Server Error"
        if s == "via_wasm_token": return 200, {}, b"\0asm\x01\0\0\0" + TOKEN.encode() + b"\0"
        if s == "via_wasm_token_head": return 200, {"x-leak": TOKEN}, b"\0asm\x01\0\0\0"
        return 200, {"content-type": "application/wasm"}, b"\0asm\x01\0\0\0"
    return 404, {}, b""


def respond(path, who):
    s = scenario
    if s.startswith("via_"): return via(path, who, s)
    # the host check (--access)
    if s == "access_ok": return 302, {"location": ACCESS}, b""
    if s == "access_other_redirect": return 302, {"location": "https://evil.example/login"}, b""
    if s == "access_lookalike_suffix": return 302, {"location": "https://team.cloudflareaccess.com.evil.example/cdn-cgi/access/login"}, b""
    if s == "access_lookalike_prefix": return 302, {"location": "https://evilcloudflareaccess.com/cdn-cgi/access/login"}, b""
    if s == "access_upper": return 302, {"location": "https://TEAM.CloudflareAccess.com/cdn-cgi/access/login/learn.dev.palicanon.app?kid=1"}, b""
    if s == "access_200": return 200, {}, page("PaliLearn", WASM, "")
    if s == "access_403_cf": return 403, {"cf-mitigated": "challenge"}, b"challenge"
    if s == "access_403_plain": return 403, {}, b""
    if s == "access_503": return 503, {}, b""
    if s == "hang": time.sleep(30); return 200, {}, b""
    # the bare origin (--refused)
    if s == "refused_ok": return 403, {}, b""
    if s == "refused_body": return 403, {}, b"Your client does not have permission"
    if s == "refused_open": return 200, {}, page("PaliLearn", WASM, "")
    if s == "refused_500": return 500, {}, b""
    if s == "refused_wrong_open": return (403, {}, b"") if who == "none" else (200, {}, b"page")
    # a revision (revision URL HEADER_FILE)
    if who != "right" and s != "open":
        if s == "wrong_ok" and who == "wrong":
            pass
        elif s == "body403":
            return 403, {}, b"forbidden"
        else:
            return 403, {"cache-control": "no-store"}, b""
    if path == "/":
        if s == "root500": return 500, {}, b""
        if s == "root404": return 404, {}, b""
        name = "Something else" if s == "noname" else "PaliLearn"
        link = "" if s == "nowasm" else WASM
        extra = TOKEN if s == "root_token" else (SECRET if s == "echo_body" else "")
        head = {"content-type": "text/html"}
        if s == "echo_head": head["x-echo"] = SECRET
        return 200, head, page(name, link, extra)
    if path == WASM:
        if s == "wasm404": return 404, {}, b""
        if s == "wasm_token": return 200, {}, b"\0asm\x01\0\0\0" + TOKEN.encode() + b"\0"
        if s == "echo_wasm": return 200, {}, b"\0asm\x01\0\0\0" + SECRET.encode() + b"\0"
        if s == "echo_wasm_head": return 200, {"x-echo": SECRET}, b"\0asm\x01\0\0\0"
        return 200, {"content-type": "application/wasm"}, b"\0asm\x01\0\0\0"
    return 404, {}, b""


class H(http.server.BaseHTTPRequestHandler):
    protocol_version = "HTTP/1.0"

    def do_GET(self):
        vs = self.headers.get_all("x-origin-auth") or []
        who = "none" if not vs else ("right" if vs == [SECRET] else "wrong")
        if len(vs) > 1:
            who = "twice"
            # a guard that reads only the first of two headers
            if scenario == "twice_open" and vs[0] == SECRET: who = "right"
        with open(logfile, "a") as f:
            f.write("GET %s auth=%s\n" % (self.path, who))
        if scenario == "via_close":
            # the edge drops the connection with no HTTP answer at all
            self.connection.shutdown(socket.SHUT_RDWR)
            return
        status, headers, body = respond(self.path, who)
        self.send_response(status)
        for k, val in headers.items():
            self.send_header(k, val)
        self.send_header("Content-Length", str(len(body)))
        self.end_headers()
        self.wfile.write(body)

    def log_message(self, *a):
        pass


class S(socketserver.ThreadingMixIn, http.server.HTTPServer):
    daemon_threads = True


srv = S(("127.0.0.1", 0), H)
open(portfile, "w").write(str(srv.server_address[1]))
srv.serve_forever()
PY

# A counter, not $RANDOM: 87 fake servers are started per run and two files
# with the same random name made a case read the PREVIOUS server's port (a
# refused connection, "000"): a $RANDOM collision hit about one run in ten.
serve_n=0
serve() { # scenario -> URL, SLOG, SRV_PID
  serve_n=$((serve_n + 1))
  SLOG="$work/log.$serve_n"
  local pf="$work/port.$serve_n"
  : > "$SLOG"
  python3 "$work/fake.py" "$1" "$pf" "$SLOG" &
  SRV_PID=$!
  local _
  for _ in $(seq 1 100); do [ -s "$pf" ] && break; sleep 0.05; done
  URL="http://127.0.0.1:$(cat "$pf")"
}
stop() { kill "$SRV_PID" 2>/dev/null; wait "$SRV_PID" 2>/dev/null; SRV_PID=""; }

# A curl shim records every argument curl was given.
shim="$work/shim"
mkdir -p "$shim"
real_curl="$(command -v curl)"
cat > "$shim/curl" <<SH
#!/usr/bin/env bash
printf '%s\n' "\$*" >> "$work/curl-argv"
exec "$real_curl" "\$@"
SH
chmod +x "$shim/curl"

header="$work/header"
printf 'x-origin-auth: %s\n' "$SECRET" > "$header"
chmod 600 "$header"

# run_case NAME SCENARIO WANT_RC PATTERN ARGS... ("@URL" becomes the server)
run_case() {
  local name=$1 scen=$2 want=$3 pat=$4
  shift 4
  serve "$scen"
  local args=() a
  for a in "$@"; do args+=("${a//@URL/$URL}"); done
  : > "$work/curl-argv"
  out="$(PATH="$shim:$PATH" ROLLOUT_MAX_TIME="${MAXT:-10}" bash "$script" "${args[@]}" 2>&1)"
  rc=$?
  stop
  assert_eq "$name: exit code" "$want" "$rc"
  if [ -n "$pat" ]; then assert_match "$name: output names it" "$pat" "$out"; fi
  assert_absent "$name: the secret is not printed" "$SECRET" "$out"
  assert_absent "$name: no token shape is printed" "$TOKEN_HEAD" "$out"
  assert_absent "$name: the secret is in no curl argument" "$SECRET" "$(cat "$work/curl-argv")"
}

echo "== rollout-check.sh: revision URL HEADER_FILE"
run_case "healthy revision" healthy 0 "ok" revision @URL "$header"
log_ok="$(cat "$SLOG")"
assert_match "healthy: / was asked with the right header" "GET / auth=right" "$log_ok"
assert_match "healthy: the wasm was asked with the right header" "GET /pkg/learn-pali.4f2a9c.wasm auth=right" "$log_ok"
assert_match "healthy: / was asked without the header" "GET / auth=none" "$log_ok"
assert_match "healthy: / was asked with a wrong header" "GET / auth=wrong" "$log_ok"
assert_match "healthy: / was asked with two headers (the right one and a wrong one)" "GET / auth=twice" "$log_ok"
argv="$(cat "$work/curl-argv")"
assert_match "healthy: curl was given the header file, not its value" "@$header" "$argv"
assert_eq "healthy: every curl call has --max-time" "0" "$(grep -vc -- '--max-time' "$work/curl-argv")"

run_case "root is 500" root500 1 "FAIL root" revision @URL "$header"
run_case "root is 404" root404 1 "FAIL root" revision @URL "$header"
run_case "page does not name PaliLearn" noname 1 "FAIL root" revision @URL "$header"
run_case "page links no wasm" nowasm 1 "FAIL wasm" revision @URL "$header"
run_case "wasm is 404" wasm404 1 "FAIL wasm" revision @URL "$header"
run_case "token in the wasm" wasm_token 1 "FAIL token" revision @URL "$header"
run_case "token in the page" root_token 1 "FAIL token" revision @URL "$header"
run_case "guard missing (open)" open 1 "FAIL guard" revision @URL "$header"
run_case "wrong header accepted" wrong_ok 1 "FAIL guard" revision @URL "$header"
run_case "the 403 has a body" body403 1 "FAIL guard" revision @URL "$header"
run_case "two headers, the right one first, are admitted" twice_open 1 "FAIL guard" revision @URL "$header"
run_case "the secret echoed in the page" echo_body 1 "FAIL secret.*root" revision @URL "$header"
run_case "the secret echoed in a header of the page" echo_head 1 "FAIL secret.*root" revision @URL "$header"
run_case "the secret in the wasm" echo_wasm 1 "FAIL secret.*wasm" revision @URL "$header"
run_case "the secret in a header of the wasm" echo_wasm_head 1 "FAIL secret.*wasm" revision @URL "$header"

echo "== rollout-check.sh: --refused URL"
run_case "refused: 403" refused_ok 0 "refused ok" --refused @URL
run_case "refused: a 403 with a body is not the guard's refusal" refused_body 1 "a 403 with a body is not the guard's refusal" --refused @URL
run_case "refused: the origin answers 200" refused_open 1 "FAIL refused" --refused @URL
run_case "refused: the origin answers 500" refused_500 1 "FAIL refused" --refused @URL
run_case "refused: only a wrong header gets through" refused_wrong_open 1 "FAIL refused" --refused @URL

echo "== rollout-check.sh: --access URL"
run_case "access: 302 to cloudflareaccess.com" access_ok 0 "access ok" --access @URL
run_case "access: 302 elsewhere" access_other_redirect 4 "FAIL access" --access @URL
run_case "access: a lookalike suffix (cloudflareaccess.com.evil.example)" access_lookalike_suffix 4 "FAIL access" --access @URL
run_case "access: a lookalike prefix (evilcloudflareaccess.com)" access_lookalike_prefix 4 "FAIL access" --access @URL
run_case "access: uppercase host and path pass" access_upper 0 "access ok" --access @URL
run_case "access: 200 means Access is missing" access_200 2 "Access missing" --access @URL
run_case "access: 403 with cf-mitigated warns" access_403_cf 3 "cf-mitigated" --access @URL
run_case "access: a plain 403" access_403_plain 4 "FAIL access" --access @URL
run_case "access: 503" access_503 4 "FAIL access" --access @URL
MAXT=2 run_case "access: the server never answers (timeout)" hang 4 "FAIL access" --access @URL
log_acc="$(cat "$SLOG")"
assert_absent "access: the secret header is never sent" "auth=right" "$log_acc"

echo "== rollout-check.sh: --via-worker URL (the public host; the Worker adds the header)"
run_case "via-worker: healthy" via_ok 0 "root ok" --via-worker @URL
log_via="$(cat "$SLOG")"
assert_match "via-worker: / was asked" "GET / auth=none" "$log_via"
assert_match "via-worker: the wasm was asked" "GET /pkg/learn-pali.4f2a9c.wasm auth=none" "$log_via"
assert_absent "via-worker: no request carried the origin header" "auth=right" "$log_via"
assert_absent "via-worker: no request carried any x-origin-auth header" "auth=wrong" "$log_via"
assert_eq "via-worker: the guard probes of the revision mode are not run (2 requests)" "2" "$(grep -c '^GET' <<<"$log_via")"
assert_match "via-worker: names wasm, token" "wasm ok" "$out"
assert_match "via-worker: and the token check" "token ok" "$out"
assert_eq "via-worker: every curl call has --max-time" "0" "$(grep -vc -- '--max-time' "$work/curl-argv")"
run_case "via-worker: / is 500 (the service's, empty body)" via_root500 1 "FAIL root" --via-worker @URL
run_case "via-worker: / is 500 with another body (the service's)" via_root500_service 1 "FAIL root" --via-worker @URL
run_case "via-worker: a 500 whose body only contains the Worker's words is the service's" via_500_suffix_middle 1 "FAIL root" --via-worker @URL
run_case "via-worker: / is 404" via_root404 1 "FAIL root" --via-worker @URL
run_case "via-worker: a plain 403 is the edge's (the Worker's secret may be wrong), not the revision's" via_403 6 "edge.*Worker.*origin-secret push" --via-worker @URL
run_case "via-worker: a 403 on the wasm request is the edge's too" via_403wasm 6 "edge.*Worker.*origin-secret push" --via-worker @URL
# E1/ET1 (034 round 2): a 302 to Cloudflare Access means the host is behind
# Access while the workflow's setting says it is not: exit 7, never a rollback.
run_case "via-worker: a 302 to cloudflareaccess.com (Access in front, ACCESS_<env> false)" via_302 7 "FAIL access.*behind Cloudflare Access.*ACCESS_<env>" --via-worker @URL
assert_match "via-worker: the 7 names the remedy" "deploy.yml.*infra" "$out"
run_case "via-worker: a 302 elsewhere stays a failed check" via_302_other 1 "FAIL root" --via-worker @URL
# G1 (034 round 3): any 30x to Access is the same disagreement as the 302.
for code in 301 303 307 308; do
  run_case "via-worker: a $code to cloudflareaccess.com (Access in front)" "via_$code" 7 "FAIL access.*behind Cloudflare Access.*ACCESS_<env>" --via-worker @URL
done
run_case "via-worker: a 307 elsewhere stays a failed check" via_307_other 1 "FAIL root" --via-worker @URL
# G2: the wasm answers the Access redirect while / answers 200; the Location
# is read from the wasm's own head (root's decoy Location does not match, and
# in the other scenario root's Location matches while the wasm's does not).
run_case "via-worker: / 200, the wasm 302 to Access" via_wasm_302 7 "FAIL access.*behind Cloudflare Access.*ACCESS_<env>" --via-worker @URL
run_case "via-worker: / 200 with an Access Location, the wasm 302 elsewhere stays a failed check" via_wasm_302_other 1 "FAIL wasm" --via-worker @URL
run_case "via-worker: the page does not name PaliLearn" via_noname 1 "FAIL root" --via-worker @URL
run_case "via-worker: the page links no wasm" via_nowasm 1 "FAIL wasm" --via-worker @URL
run_case "via-worker: the wasm is 404" via_wasm404 1 "FAIL wasm" --via-worker @URL
run_case "via-worker: a token shape in the page" via_root_token 1 "FAIL token" --via-worker @URL
run_case "via-worker: a token shape in a header of the page" via_root_token_head 1 "FAIL token" --via-worker @URL
run_case "via-worker: a token shape in the wasm" via_wasm_token 1 "FAIL token" --via-worker @URL
run_case "via-worker: a token shape in a header of the wasm" via_wasm_token_head 1 "FAIL token" --via-worker @URL
# D2 (034 round 1): the edge failing is not the revision failing: exit 6.
run_case "via-worker: the connection is closed with no HTTP answer" via_close 6 "edge.*Worker.*no HTTP answer" --via-worker @URL
MAXT=2 run_case "via-worker: no answer in time (curl timeout, 000)" via_hang 6 "edge.*Worker.*no HTTP answer" --via-worker @URL
serve via_ok; dead_url="$URL"; stop
run_case "via-worker: connection refused (nothing listens)" via_ok 6 "edge.*Worker.*no HTTP answer" --via-worker "$dead_url"
for code in 520 521 522 523 524 525 526 527; do
  run_case "via-worker: Cloudflare answers $code" "via_edge_$code" 6 "edge.*Worker.*$code" --via-worker @URL
done
run_case "via-worker: Cloudflare answers 522 on the wasm request" via_edgewasm_522 6 "edge.*Worker.*522" --via-worker @URL
run_case "via-worker: a 502 is the service's, relayed (exit 1)" via_edge_502 1 "FAIL root" --via-worker @URL
run_case "via-worker: a 528 is not one of the edge codes (exit 1)" via_edge_528 1 "FAIL root" --via-worker @URL
run_case "via-worker: the wasm's own 500 stays exit 1" via_wasm_500 1 "FAIL wasm" --via-worker @URL
run_case "via-worker: Cloudflare challenged the runner" via_challenge 3 "cf-mitigated" --via-worker @URL
run_case "via-worker: Cloudflare challenged the wasm request" via_challenge_wasm 3 "cf-mitigated" --via-worker @URL
run_case "via-worker: the Worker's own 500" via_worker500 4 "worker-500" --via-worker @URL
assert_match "via-worker: the 4 names the remedy" "origin-secret push" "$out"
run_case "via-worker: the Worker's 500 with a CRLF" via_worker500_crlf 4 "worker-500" --via-worker @URL
run_case "via-worker: the Worker's 500 padded with whitespace" via_worker500_pad 4 "worker-500" --via-worker @URL
run_case "via-worker: the Worker's 500, another 'is not configured.' message (origin)" via_worker500_origin 4 "worker-500" --via-worker @URL
run_case "via-worker: the Worker's 500, another 'is not configured.' message (public origin)" via_worker500_public 4 "worker-500" --via-worker @URL
run_case "via-worker: the Worker's 500 on the wasm request" via_wasm_worker500 4 "worker-500" --via-worker @URL
MAXT=2 run_case "via-worker: the server never answers (timeout)" via_hang 6 "FAIL edge" --via-worker @URL
# ET3 (034 round 2): the edge's exit 6 is --via-worker only; a revision URL
# that closes the connection is the revision's failure.
run_case "revision mode: the connection is closed with no HTTP answer (1, never 6)" via_close 1 "FAIL root" revision @URL "$header"
assert_eq "revision mode: a 500 with the Worker's words is a plain failure (1), never 4" "1" "$(
  serve root500
  PATH="$shim:$PATH" bash "$script" revision "$URL" "$header" >/dev/null 2>&1; echo $?
  stop
)"

echo "== the Access redirect pattern: one file, read by the script"
pattern_file="$here/access-redirect-pattern"
assert_eq "pattern: the file is one line" "1" "$(grep -c '' "$pattern_file" 2>/dev/null)"
for good in 'https://team.cloudflareaccess.com/cdn-cgi/access/login' 'https://X.CloudflareAccess.com/cdn-cgi/' \
  'https://a.b.cloudflareaccess.com?kid=1' 'https://team.cloudflareaccess.com'; do
  if printf '%s' "$good" | grep -Eiq -f "$pattern_file" 2>/dev/null; then pass "pattern: matches $good"; else flunk "pattern: matches $good"; fi
done
for bad in 'https://team.cloudflareaccess.com.evil.example/x' 'https://evilcloudflareaccess.com/x' \
  'https://cloudflareaccess.com/x' 'https://evil.example/x.cloudflareaccess.com/y' 'http://team.cloudflareaccess.com/x'; do
  if printf '%s' "$bad" | grep -Eiq -f "$pattern_file" 2>/dev/null; then flunk "pattern: refuses $bad"; else pass "pattern: refuses $bad"; fi
done
# The script reads it: a copy beside another pattern follows that pattern.
mkdir -p "$work/alt"
cp "$script" "$work/alt/rollout-check.sh"
printf '%s\n' '^https://never\.example/' > "$work/alt/access-redirect-pattern"
real_script="$script"
script="$work/alt/rollout-check.sh"
run_case "access: the pattern is read from the file beside the script" access_ok 4 "FAIL access" --access @URL
script="$real_script"
assert_eq "pipes: no printf piped into grep -q in the script (a closed pipe reads as a mismatch under pipefail)" "0" "$(grep -Ec 'printf .*[|] *grep -[A-Za-z]*q' "$script")"

# DT2: a pattern file that cannot be read is named, never "redirects elsewhere".
mkdir -p "$work/nopat"
cp "$script" "$work/nopat/rollout-check.sh"
real_script="$script"
script="$work/nopat/rollout-check.sh"
run_case "access: the pattern file is missing" access_ok 4 "FAIL access.*access-redirect-pattern" --access @URL
assert_absent "access: a missing pattern file is not blamed on the redirect" "something that is not" "$out"
: > "$work/nopat/access-redirect-pattern"
run_case "access: the pattern file is empty" access_ok 4 "FAIL access.*access-redirect-pattern" --access @URL
if [ "$(id -u)" != 0 ]; then
  printf '%s\n' '^https://' > "$work/nopat/access-redirect-pattern"
  chmod 000 "$work/nopat/access-redirect-pattern"
  run_case "access: the pattern file is unreadable" access_ok 4 "FAIL access.*access-redirect-pattern" --access @URL
  assert_absent "access: an unreadable pattern file is not blamed on the redirect" "something that is not" "$out"
  chmod 600 "$work/nopat/access-redirect-pattern"
else
  echo "skip - running as root: an unreadable file is readable"
fi
script="$real_script"

# G1b (034 round 3): --via-worker with no usable pattern is a usage error
# (64), checked before any request, never a failed check (1, a rollback).
mkdir -p "$work/nopat-vw"
cp "$script" "$work/nopat-vw/rollout-check.sh"
real_script="$script"
script="$work/nopat-vw/rollout-check.sh"
run_case "via-worker: the pattern file is missing" via_ok 64 "access-redirect-pattern" --via-worker @URL
assert_eq "via-worker: a missing pattern file is checked before any request" "0" "$(grep -c '^GET' "$SLOG")"
: > "$work/nopat-vw/access-redirect-pattern"
run_case "via-worker: the pattern file is empty" via_ok 64 "access-redirect-pattern" --via-worker @URL
assert_eq "via-worker: an empty pattern file is checked before any request" "0" "$(grep -c '^GET' "$SLOG")"
script="$real_script"

echo "== rollout-check.sh: usage"
check_usage() { # name args...
  local name=$1
  shift
  out="$(bash "$script" "$@" 2>&1)"
  rc=$?
  assert_eq "usage: $name exits 64" "64" "$rc"
  assert_match "usage: $name says usage" "usage" "$out"
}
check_usage "no arguments"
check_usage "unknown mode" --bogus http://127.0.0.1:9
check_usage "revision with no header file" revision http://127.0.0.1:9
check_usage "revision with no URL"
check_usage "header file that is not a file" revision http://127.0.0.1:9 "$work/nope"
printf 'x-origin-auth: a\nx-origin-auth: b\n' > "$work/two"
check_usage "header file with two lines" revision http://127.0.0.1:9 "$work/two"
printf 'authorization: a\n' > "$work/other"
check_usage "header file with another header" revision http://127.0.0.1:9 "$work/other"
check_usage "refused is given the secret" --refused http://127.0.0.1:9 "$header"
check_usage "access is given the secret" --access http://127.0.0.1:9 "$header"
# DT1b: a header file that holds no usable secret never ends in "secret ok".
: > "$work/h-empty"
printf '%s\n' "$SECRET" > "$work/h-noprefix"
printf 'x-origin-auth: \n' > "$work/h-novalue"
printf 'x-origin-auth:     \n' > "$work/h-blankvalue"
for bad in empty noprefix novalue blankvalue; do
  check_usage "header file ($bad)" revision http://127.0.0.1:9 "$work/h-$bad"
  assert_absent "usage: header file ($bad) never says secret ok" "secret ok" "$out"
  serve healthy
  out="$(bash "$script" revision "$URL" "$work/h-$bad" 2>&1)"
  rc=$?
  stop
  assert_eq "header file ($bad) against a healthy server: refused (64)" "64" "$rc"
  assert_absent "header file ($bad) against a healthy server: never secret ok" "secret ok" "$out"
  assert_eq "header file ($bad) against a healthy server: nothing was asked" "" "$(cat "$SLOG")"
done
check_usage "access with no URL" --access
check_usage "via-worker with no URL" --via-worker
check_usage "via-worker is given a header file" --via-worker http://127.0.0.1:9 "$header"
check_usage "via-worker is given a second argument" --via-worker http://127.0.0.1:9 extra
# The secret never reaches this mode, even from the environment.
serve via_ok
out="$(ORIGIN_SECRET="$SECRET" bash "$script" --via-worker "$URL" 2>&1)"
rc=$?
stop
assert_eq "usage: via-worker with ORIGIN_SECRET set exits 64" "64" "$rc"
assert_match "usage: via-worker with ORIGIN_SECRET says usage" "usage" "$out"
assert_absent "usage: and nothing was asked" "GET" "$(cat "$SLOG")"
assert_absent "usage: and the secret is not printed" "$SECRET" "$out"

# ------------------------------------------------------------------ Dockerfile
echo "== app/Dockerfile"
assert_file_lacks "Dockerfile: no ssh mount" "$dockerfile" 'type=ssh'
assert_file_lacks "Dockerfile: no known_hosts" "$dockerfile" 'known_hosts'
assert_file_lacks "Dockerfile: no --ssh in the usage comment" "$dockerfile" '--ssh'
assert_file_lacks "Dockerfile: no ssh agent talk left" "$dockerfile" 'ssh agent|ssh-agent|deploy key'
assert_file_lacks "Dockerfile: no CARGO_NET_GIT_FETCH_WITH_CLI" "$dockerfile" 'CARGO_NET_GIT_FETCH_WITH_CLI'
assert_file_has "Dockerfile: the runtime is non-root" "$dockerfile" '^USER app$'
assert_file_has "Dockerfile: LEPTOS_ENV=PROD" "$dockerfile" 'LEPTOS_ENV=PROD'
assert_file_has "Dockerfile: a RUN looks for a git source in the lock" "$dockerfile" "^RUN .*grep -q '\\^source = \"git\\+' Cargo.lock"

# The guard RUN, extracted (with its continuation lines) and run on lockfiles.
guard="$work/guard.sh"
python3 - "$dockerfile" > "$guard" <<'PY'
import sys
lines = open(sys.argv[1]).read().split("\n")
out = []
i = 0
while i < len(lines):
    if lines[i].startswith("RUN ") and "grep -q '^source = \"git+'" in "\n".join(lines[i:i + 3]):
        cmd = lines[i][4:]
        while cmd.rstrip().endswith("\\"):
            i += 1
            cmd = cmd.rstrip()[:-1] + " " + lines[i].strip()
        out.append(cmd)
        break
    i += 1
print("\n".join(out))
PY
assert_match "guard: extracted from the Dockerfile" "grep" "$(cat "$guard")"
mkdir -p "$work/lock-clean" "$work/lock-git"
cp "$root/Cargo.lock" "$work/lock-clean/Cargo.lock"
{ cat "$root/Cargo.lock"; printf '\n[[package]]\nname = "x"\nversion = "0.1.0"\nsource = "git+https://github.com/PaliCanon/ui-lib?tag=v0.1.0#abc"\n'; } > "$work/lock-git/Cargo.lock"
out="$(cd "$work/lock-clean" && bash "$guard" 2>&1)"
assert_eq "guard: passes on the real lock (no git source)" "0" "$?"
out="$(cd "$work/lock-git" && bash "$guard" 2>&1)"
rc=$?
assert_eq "guard: fails on a lock with a git source" "1" "$rc"
assert_match "guard: the message says what to do" "git source" "$out"

# DT5: the smoke RUN (the heredoc after `RUN <<'EOF'`) extracted, its port
# moved to a free one, and run in a folder holding a fake ./learn_pali (a
# python server): the 403 without the header must have an empty body.
smoke="$work/smoke.sh"
python3 - "$dockerfile" > "$smoke" <<'PY'
import sys
lines = open(sys.argv[1]).read().split("\n")
start = next(i for i, l in enumerate(lines) if l.startswith("RUN <<'EOF'"))
end = next(i for i in range(start + 1, len(lines)) if lines[i] == "EOF")
print("\n".join(lines[start + 1:end]))
PY
assert_match "smoke: extracted from the Dockerfile" "smoke: ok" "$(cat "$smoke")"
cat > "$work/learn_pali" <<'PY'
#!/usr/bin/env python3
import http.server, os, socketserver
secret = os.environ["ORIGIN_SECRET"]
scenario = os.environ["SMOKE_SCENARIO"]
class H(http.server.BaseHTTPRequestHandler):
    protocol_version = "HTTP/1.0"
    def do_GET(self):
        ok = self.headers.get("x-origin-auth") == secret
        if not ok:
            body = b"Your client does not have permission" if scenario == "body403" else b""
            code = 403
        elif self.path == "/":
            code, body = 200, b'<html>PaliLearn <link href="/pkg/learn-pali.ab12.wasm"></html>'
        elif self.path.endswith(".wasm"):
            code, body = 200, b"\0asm"
        else:
            code, body = 404, b""
        self.send_response(code)
        self.send_header("Content-Length", str(len(body)))
        self.end_headers()
        self.wfile.write(body)
    def log_message(self, *a): pass
class S(socketserver.ThreadingMixIn, http.server.HTTPServer):
    daemon_threads = True
S(("127.0.0.1", int(os.environ["FAKE_PORT"])), H).serve_forever()
PY
chmod +x "$work/learn_pali"
run_smoke() { # scenario
  local port
  port="$(python3 -c 'import socket; s = socket.socket(); s.bind(("127.0.0.1", 0)); print(s.getsockname()[1])')"
  sed "s#/dev/tcp/127.0.0.1/8080#/dev/tcp/127.0.0.1/$port#" "$smoke" > "$work/smoke-run.sh"
  out="$(cd "$work" && FAKE_PORT="$port" SMOKE_SCENARIO="$1" bash "$work/smoke-run.sh" 2>&1)"
  rc=$?
}
run_smoke empty403
assert_eq "smoke: a guard that refuses with an empty 403 passes" "0" "$rc"
assert_match "smoke: and says ok" "smoke: ok" "$out"
run_smoke body403
assert_eq "smoke: a 403 with a body fails" "1" "$rc"
assert_match "smoke: and says the 403 has a body" "smoke: .*403.*body" "$out"

# -------------------------------------------------------------------- infra/gcp.tf
echo "== infra/gcp.tf: startup probe"
gcp_file="$root/infra/gcp.tf"
startup_block=$(grep -A20 'startup_probe {' "$gcp_file" | head -15)
timeout=$(echo "$startup_block" | grep 'timeout_seconds' | sed 's/.*timeout_seconds[[:space:]]*=[[:space:]]*\([0-9]*\).*/\1/')
period=$(echo "$startup_block" | grep 'period_seconds' | sed 's/.*period_seconds[[:space:]]*=[[:space:]]*\([0-9]*\).*/\1/')
if [ -z "$timeout" ] || [ -z "$period" ]; then
  flunk "startup probe: could not parse timeout_seconds or period_seconds"
elif [ "$timeout" -lt "$period" ]; then
  pass "startup probe: timeout_seconds ($timeout) < period_seconds ($period)"
else
  flunk "startup probe: timeout_seconds ($timeout) must be < period_seconds ($period)"
fi

# -------------------------------------------------------------------- deploy.yml
echo "== deploy.yml"
if python3 -c 'import sys,yaml; yaml.safe_load(open(sys.argv[1]))' "$workflow" 2>"$work/yaml.err"; then
  pass "deploy.yml parses as YAML"
else
  flunk "deploy.yml parses as YAML ($(cat "$work/yaml.err"))"
fi
if command -v actionlint >/dev/null 2>&1; then
  actionlint "$workflow" > "$work/actionlint.out" 2>&1
  assert_eq "actionlint is clean" "0" "$?"
else
  echo "skip - actionlint is not installed"
fi

# step_field ID FIELD: the `run` (or `if`) of the step with that id.
step_field() {
  python3 - "$workflow" "$1" "$2" <<'PY'
import sys, yaml
wf = yaml.safe_load(open(sys.argv[1]))
for step in wf["jobs"]["site"]["steps"]:
    if step.get("id") == sys.argv[2]:
        print(step.get(sys.argv[3], ""))
        break
else:
    sys.exit(3)
PY
}
for id in current deploy secret check traffic refused host; do
  if step_field "$id" run > "$work/step-$id.sh" 2>/dev/null; then pass "deploy.yml has the step '$id'"; else flunk "deploy.yml has the step '$id'"; : > "$work/step-$id.sh"; fi
done
order="$(python3 - "$workflow" <<'PY'
import sys, yaml
wf = yaml.safe_load(open(sys.argv[1]))
print(" ".join(s["id"] for s in wf["jobs"]["site"]["steps"] if s.get("id") in
      ("current", "deploy", "secret", "check", "traffic", "refused", "host")))
PY
)"
assert_eq "deploy.yml: steps 1 to 6b in order" "current deploy secret check traffic refused host" "$order"
for id in deploy secret check traffic; do
  assert_eq "deploy.yml: '$id' runs only when the image is not already served" "steps.current.outputs.running != 'true'" "$(step_field "$id" if 2>/dev/null | sed 's/^\${{ *//; s/ *}}$//')"
done
assert_eq "deploy.yml: the host check runs for every env (no if)" "" "$(step_field host if 2>/dev/null)"
assert_eq "deploy.yml: the host check knows PREVIOUS (for the rollback)" "\${{ steps.current.outputs.serving }}" "$(python3 - "$workflow" <<'PY'
import sys, yaml
wf = yaml.safe_load(open(sys.argv[1]))
print(next(s for s in wf["jobs"]["site"]["steps"] if s.get("id") == "host").get("env", {}).get("PREVIOUS", ""))
PY
)"
assert_file_lacks "deploy.yml: no --ssh, no ssh: input on the image build" "$workflow" '^[[:space:]]*ssh:|--ssh'
assert_file_has "deploy.yml: the ui-lib action step is untouched" "$workflow" '^      - uses: \./learn-pali/\.github/actions/ui-lib$'
assert_file_has "deploy.yml: the action gets the App id" "$workflow" 'app-id: \$\{\{ secrets\.APP_ID \}\}'
assert_file_has "deploy.yml: the action gets the App private key" "$workflow" 'private-key: \$\{\{ secrets\.APP_PRIVATE_KEY \}\}'
for id in current deploy secret check traffic refused host; do
  assert_file_lacks "deploy.yml: '$id' has no \${{ }} in its script" "$work/step-$id.sh" '\$\{\{'
  assert_file_lacks "deploy.yml: '$id' never runs set -x" "$work/step-$id.sh" 'set -[a-z]*x'
done
assert_file_has "deploy.yml: the message names the usual causes (startup probe, origin secret)" "$work/step-deploy.sh" 'read its error above.*startup probe.*origin secret'
assert_file_has "deploy.yml: the deploy has no traffic" "$work/step-deploy.sh" '--no-traffic'
assert_file_lacks "deploy.yml: --to-latest appears nowhere" "$workflow" '--to-latest'
assert_file_has "deploy.yml: step 5 reads the revision step 2 created" "$workflow" 'REVISION: \$\{\{ steps\.deploy\.outputs\.revision \}\}'
assert_file_has "deploy.yml: PERMISSION_DENIED is named" "$work/step-secret.sh" 'deployer lacks secretAccessor on learn-pali-origin-secret-'
assert_file_has "deploy.yml: the secret is masked" "$work/step-secret.sh" '::add-mask::'
assert_file_has "deploy.yml: the secret file is 0600" "$work/step-secret.sh" 'umask 077'
assert_file_lacks "deploy.yml: the secret is never put in a shell variable" "$work/step-secret.sh" '(^|[^A-Z_])(VALUE|SECRET)=|\$\(gcloud secrets'
assert_file_has "deploy.yml: the host check is --via-worker on the host" "$work/step-host.sh" 'rollout-check\.sh --via-worker "https://\$HOST"'
assert_file_has "deploy.yml: the host check calls --access for an env with Access" "$work/step-host.sh" 'rollout-check\.sh --access "https://\$HOST"'
assert_file_has "deploy.yml: the host check tries up to 5 times" "$work/step-host.sh" 'for _ in 1 2 3 4 5'
assert_file_has "deploy.yml: the host check names the remedy of the Worker's 500" "$work/step-host.sh" 'run infra/scripts/origin-secret push \$ENV'
# The workflow-level map and infra/variables.tf's `envs` defaults say the same
# (change both together): dev from the default block, prod from the
# optional() default (prod is not in the default map yet).
assert_eq "deploy.yml: the access settings (ACCESS_dev, ACCESS_prod) mirror infra/variables.tf" "ACCESS_dev=false ACCESS_prod=false | tf: dev=false prod=false" "$(python3 - "$workflow" "$root/infra/variables.tf" <<'PY'
import re, sys, yaml
env = yaml.safe_load(open(sys.argv[1])).get("env", {})
tf = open(sys.argv[2]).read()
opt = re.search(r"access\s*=\s*optional\(bool,\s*(true|false)\)", tf).group(1)
dev = re.search(r'default\s*=\s*\{\s*dev\s*=\s*\{[^}]*?access\s*=\s*(true|false)', tf, re.S)
prod = re.search(r'^\s*prod\s*=\s*\{[^}]*?access\s*=\s*(true|false)', tf, re.M | re.S)
print("ACCESS_dev=%s ACCESS_prod=%s | tf: dev=%s prod=%s" % (
    env.get("ACCESS_dev"), env.get("ACCESS_prod"), dev.group(1) if dev else opt, prod.group(1) if prod else opt))
PY
)"
assert_file_has "deploy.yml: the origin check can fail with exit 5" "$work/step-refused.sh" 'exit 5'
assert_file_has "deploy.yml: the bare origin check is --refused" "$work/step-refused.sh" '--refused'
assert_file_has "deploy.yml: the revision is checked with the header file" "$work/step-check.sh" 'rollout-check\.sh revision "\$URL" "\$HEADER_FILE"'
assert_file_has "deploy.yml: the header file is removed at the end" "$workflow" 'if: always\(\)'

# ---- the steps, run with a stub gcloud and a stub rollout-check
stub="$work/stub"
proj="$work/proj"
mkdir -p "$stub/bin" "$proj/scripts"
cat > "$stub/bin/gcloud" <<'SH'
#!/usr/bin/env bash
# A stub gcloud over the state in $STUB (the arguments are logged, the secret never is).
echo "gcloud $*" >> "$STUB/calls"
all="$*"
case "$1 $2 $3" in
  "run services describe")
    case "$all" in
      *"value(status.url)"*) cat "$STUB/service-url" ;;
      *)
        serving=$(cat "$STUB/serving")
        if [ -f "$STUB/follow-latest" ]; then entry='{"percent":100,"latestRevision":true}'; else entry="{\"percent\":100,\"revisionName\":\"$serving\"}"; fi
        if [ -f "$STUB/follow-latest" ]; then serving=$(cat "$STUB/latest-ready"); fi
        if [ -f "$STUB/split" ]; then entry='{"percent":60,"revisionName":"learn-pali-dev-00001-aaa"},{"percent":40,"revisionName":"learn-pali-dev-00002-bbb"}'; fi
        created=$(cat "$STUB/latest-created")
        tagged=$(cat "$STUB/tag-revision" 2>/dev/null || cat "$STUB/latest-created")
        printf '{"status":{"url":"%s","latestReadyRevisionName":"%s","latestCreatedRevisionName":"%s","traffic":[%s,{"tag":"sha-abc1234","revisionName":"%s","url":"https://sha-abc1234---x.a.run.app"}]}}' \
          "$(cat "$STUB/service-url")" "$(cat "$STUB/latest-ready")" "$created" "$entry" "$tagged"
        ;;
    esac ;;
  "run revisions describe") cat "$STUB/image-$4" ;;
  "run deploy"*)
    if [ -f "$STUB/deploy-fails" ]; then echo "ERROR: revision is not ready: secret version not found" >&2; exit 1; fi
    # A re-run at the same SHA: before the deploy the tag sits on the older
    # revision; the deploy creates the new one and (unless "deploy-keeps-tag")
    # moves the tag onto it.
    if [ -f "$STUB/created-after-deploy" ]; then cp "$STUB/created-after-deploy" "$STUB/latest-created"; fi
    if [ -f "$STUB/tag-after-deploy" ] && [ ! -f "$STUB/deploy-keeps-tag" ]; then cp "$STUB/tag-after-deploy" "$STUB/tag-revision"; fi ;;
  "run services update-traffic")
    case "$all" in
      *--to-latest*) echo "stub gcloud: --to-latest is never used" >&2; exit 98 ;;
      *--to-revisions=*)
        if [ -f "$STUB/rollback-fails" ]; then echo "ERROR: boom" >&2; exit 1; fi
        rev="${all#*--to-revisions=}"; rev="${rev%%=*}"; printf '%s' "$rev" > "$STUB/serving" ;;
    esac ;;
  "secrets versions access")
    if [ -f "$STUB/secret-denied" ]; then
      echo "ERROR: (gcloud.secrets.versions.access) PERMISSION_DENIED: Permission 'secretmanager.versions.access' denied" >&2; exit 1
    fi
    if [ -f "$STUB/secret-empty" ]; then exit 0; fi
    cat "$STUB/secret-value" ;;
  *) echo "stub gcloud: unexpected: $*" >&2; exit 99 ;;
esac
SH
cat > "$proj/scripts/rollout-check.sh" <<'SH'
#!/usr/bin/env bash
# A stub: logs its arguments and the header file's mode, exits with the code in $STUB/rc-<mode>.
mode="${1#--}"
echo "rollout-check $*" >> "$STUB/calls"
if [ "$mode" = revision ] && [ -f "$3" ]; then
  echo "header-file $(ls -l "$3" | cut -c1-10) $(cut -c1-16 "$3" | head -n 1)" >> "$STUB/calls"
fi
echo "stub rollout-check $mode"
# rc-<mode>.seq: one exit code per line, consumed one per call (then rc-<mode>).
if [ -s "$STUB/rc-$mode.seq" ]; then
  code="$(head -n 1 "$STUB/rc-$mode.seq")"
  tail -n +2 "$STUB/rc-$mode.seq" > "$STUB/rc-$mode.seq.tmp"
  mv "$STUB/rc-$mode.seq.tmp" "$STUB/rc-$mode.seq"
  exit "$code"
fi
exit "$(cat "$STUB/rc-$mode" 2>/dev/null || echo 0)"
SH
# A stub sleep: the retries of the host check wait 3 s in CI, not here.
cat > "$stub/bin/sleep" <<'SH'
#!/usr/bin/env bash
echo "sleep $*" >> "$STUB/calls"
SH
chmod +x "$stub/bin/gcloud" "$stub/bin/sleep" "$proj/scripts/rollout-check.sh"

reset_stub() { # image-of-the-serving-revision
  rm -rf "$stub/state"
  mkdir -p "$stub/state"
  export STUB="$stub/state"
  printf 'learn-pali-dev-00001-aaa' > "$STUB/serving"
  printf 'learn-pali-dev-00002-bbb' > "$STUB/latest-ready"
  printf 'learn-pali-dev-00002-bbb' > "$STUB/latest-created"
  printf 'https://learn-pali-dev-x.a.run.app' > "$STUB/service-url"
  printf '%s' "$1" > "$STUB/image-learn-pali-dev-00001-aaa"
  printf 'registry/learn-pali:h-new' > "$STUB/image-learn-pali-dev-00002-bbb"
  printf '%s' "$SECRET" > "$STUB/secret-value"
  : > "$STUB/calls"
  : > "$STUB/out"
  rm -f "$RUNNER_TEMP/origin-header"
}
export RUNNER_TEMP="$work/runner-temp"
mkdir -p "$RUNNER_TEMP"

# step ID [VAR=value ...]: runs the step's script as the runner would.
step() {
  local id=$1
  shift
  (
    cd "$proj" || exit 99
    env PATH="$stub/bin:$PATH" STUB="$STUB" GITHUB_OUTPUT="$STUB/out" RUNNER_TEMP="$RUNNER_TEMP" \
      ENV=dev REGION=europe-west1 PROJECT=proj GITHUB_SHA=abc1234def5678 \
      ACCESS_dev=false ACCESS_prod=false \
      IMAGE_HASH=registry/learn-pali:h-new HOST=learn.dev.palicanon.app "$@" \
      bash -e "$work/step-$id.sh"
  ) 2>&1
}
calls() { cat "$STUB/calls"; }
output() { sed -n "s/^$1=//p" "$STUB/out" | head -n 1; }

echo "== deploy.yml: step 1 (PREV, skip)"
reset_stub "registry/learn-pali:h-old"
out="$(step current)"
assert_eq "current: PREV is the serving revision" "learn-pali-dev-00001-aaa" "$(output serving)"
assert_eq "current: another image is running, so deploy" "false" "$(output running)"
reset_stub "registry/learn-pali:h-new"
out="$(step current)"
assert_eq "current: the same image serves, so skip" "true" "$(output running)"
reset_stub "registry/learn-pali:h-old"
touch "$STUB/follow-latest"
printf 'registry/learn-pali:h-old' > "$STUB/image-learn-pali-dev-00002-bbb"
out="$(step current)"
assert_eq "current: a service that follows the latest revision names its ready one" "learn-pali-dev-00002-bbb" "$(output serving)"
reset_stub "registry/learn-pali:h-old"
printf '{"status":{"traffic":[]}}' > "$work/empty.json"
cat > "$stub/bin/gcloud-empty" <<'SH'
#!/usr/bin/env bash
cat "$EMPTY_JSON"
SH
chmod +x "$stub/bin/gcloud-empty"
mkdir -p "$stub/bin-empty"
cp "$stub/bin/gcloud-empty" "$stub/bin-empty/gcloud"
out="$(cd "$proj" && env PATH="$stub/bin-empty:$PATH" EMPTY_JSON="$work/empty.json" GITHUB_OUTPUT="$STUB/out" ENV=dev REGION=r PROJECT=p IMAGE_HASH=x bash -e "$work/step-current.sh" 2>&1)"
rc=$?
assert_eq "current: no serving revision fails the job" "1" "$rc"
assert_match "current: and says why" "no revision serves" "$out"

reset_stub "registry/learn-pali:h-old"
touch "$STUB/split"
out="$(step current)"
rc=$?
assert_eq "current: traffic split between two revisions fails the job" "1" "$rc"
assert_match "current: and names the split" "traffic is split: .*learn-pali-dev-00001-aaa=60%.*learn-pali-dev-00002-bbb=40%" "$out"
assert_absent "current: a split is not reported as 'no revision serves'" "no revision serves" "$out"

echo "== deploy.yml: step 2 (no-traffic deploy)"
reset_stub "registry/learn-pali:h-old"
out="$(step deploy)"
rc=$?
assert_eq "deploy: succeeds" "0" "$rc"
assert_match "deploy: --no-traffic with the tag" "run deploy learn-pali-dev .*--no-traffic --tag sha-abc1234" "$(calls)"
assert_eq "deploy: the tagged revision URL is an output" "https://sha-abc1234---x.a.run.app" "$(output url)"
assert_eq "deploy: the new revision's name is an output" "learn-pali-dev-00002-bbb" "$(output revision)"
reset_stub "registry/learn-pali:h-old"
printf 'learn-pali-dev-00003-ccc' > "$STUB/latest-created"
printf 'learn-pali-dev-00002-bbb' > "$STUB/tag-revision"
out="$(step deploy)"
rc=$?
assert_eq "deploy: the latest created revision does not carry the tag, so fail" "1" "$rc"
assert_match "deploy: and names both revisions" "learn-pali-dev-00003-ccc.*learn-pali-dev-00002-bbb|learn-pali-dev-00002-bbb.*learn-pali-dev-00003-ccc" "$out"
assert_eq "deploy: and no revision is an output" "" "$(output revision)"
# DT3: a re-run at the same SHA. The tag sha-abc1234 already sits on the older
# revision (the failed first run's); the deploy creates the new one.
reset_stub "registry/learn-pali:h-old"
printf 'learn-pali-dev-00001-aaa' > "$STUB/latest-created"
printf 'learn-pali-dev-00001-aaa' > "$STUB/tag-revision"
printf 'learn-pali-dev-00002-bbb' > "$STUB/created-after-deploy"
printf 'learn-pali-dev-00002-bbb' > "$STUB/tag-after-deploy"
out="$(step deploy)"
rc=$?
assert_eq "deploy (re-run, tag on an older revision): succeeds" "0" "$rc"
assert_eq "deploy (re-run): outputs the NEW revision, not the one the tag sat on" "learn-pali-dev-00002-bbb" "$(output revision)"
out="$(step traffic REVISION="$(output revision)")"
rc=$?
assert_eq "traffic (re-run): succeeds" "0" "$rc"
assert_match "traffic (re-run): targets the new revision" "update-traffic learn-pali-dev .*--to-revisions=learn-pali-dev-00002-bbb=100" "$(calls)"
assert_absent "traffic (re-run): never the revision the tag sat on" "00001-aaa=100" "$(calls)"
reset_stub "registry/learn-pali:h-old"
printf 'learn-pali-dev-00001-aaa' > "$STUB/latest-created"
printf 'learn-pali-dev-00001-aaa' > "$STUB/tag-revision"
printf 'learn-pali-dev-00002-bbb' > "$STUB/created-after-deploy"
printf 'learn-pali-dev-00002-bbb' > "$STUB/tag-after-deploy"
touch "$STUB/deploy-keeps-tag"
out="$(step deploy)"
rc=$?
assert_eq "deploy (re-run, the tag did not move): fails" "1" "$rc"
assert_match "deploy (re-run, the tag did not move): names both revisions" "learn-pali-dev-00002-bbb.*learn-pali-dev-00001-aaa" "$out"
assert_eq "deploy (re-run, the tag did not move): no revision output" "" "$(output revision)"
reset_stub "registry/learn-pali:h-old"
touch "$STUB/deploy-fails"
out="$(step deploy)"
rc=$?
assert_eq "deploy: a revision that never becomes ready fails" "1" "$rc"
assert_match "deploy: and names the usual causes" "gcloud could not deploy.*read its error above" "$out"
assert_absent "deploy: and touches no traffic" "update-traffic" "$(calls)"

echo "== deploy.yml: step 3 (the origin secret)"
reset_stub "registry/learn-pali:h-old"
out="$(step secret)"
rc=$?
assert_eq "secret: succeeds" "0" "$rc"
assert_eq "secret: the header file holds the header line" "x-origin-auth: $SECRET" "$(cat "$RUNNER_TEMP/origin-header" 2>/dev/null)"
assert_eq "secret: the header file is 0600" "-rw-------" "$(ls -l "$RUNNER_TEMP/origin-header" 2>/dev/null | cut -c1-10)"
assert_match "secret: the value is masked" "^::add-mask::$SECRET\$" "$out"
assert_eq "secret: the value appears only in the mask line" "0" "$(printf '%s\n' "$out" | grep -F "$SECRET" | grep -vc '^::add-mask::')"
assert_absent "secret: the value is in no gcloud call" "$SECRET" "$(calls)"
assert_match "secret: read from the right secret" "secrets versions access latest --secret learn-pali-origin-secret-dev" "$(calls)"
reset_stub "registry/learn-pali:h-old"
touch "$STUB/secret-denied"
out="$(step secret)"
rc=$?
assert_eq "secret: PERMISSION_DENIED fails" "1" "$rc"
assert_match "secret: and names the missing role" "deployer lacks secretAccessor on learn-pali-origin-secret-dev" "$out"
reset_stub "registry/learn-pali:h-old"
touch "$STUB/secret-empty"
out="$(step secret)"
rc=$?
assert_eq "secret: an empty value fails" "1" "$rc"
assert_match "secret: and says so" "no value" "$out"
assert_eq "secret: and leaves no header file" "no" "$([ -e "$RUNNER_TEMP/origin-header" ] && echo yes || echo no)"

# DT1: rollout-check fed the file step 3 really writes (not one the test builds):
# the secret grep must work on it.
reset_stub "registry/learn-pali:h-old"
step secret > /dev/null
printf '%s\n' "$SECRET" | awk '{ print "x-origin-auth: " $0 }' > "$work/awk-header"
assert_eq "secret: the step's file is byte-for-byte the awk output" "0" "$(cmp -s "$work/awk-header" "$RUNNER_TEMP/origin-header"; echo $?)"
cp "$RUNNER_TEMP/origin-header" "$work/step3-header"
run_case "step 3's header file, a clean server" healthy 0 "secret ok" revision @URL "$work/step3-header"
run_case "step 3's header file, the secret echoed in the page" echo_body 1 "FAIL secret.*root" revision @URL "$work/step3-header"
run_case "step 3's header file, the secret echoed in a header" echo_head 1 "FAIL secret.*root" revision @URL "$work/step3-header"
run_case "step 3's header file, the secret in the wasm" echo_wasm 1 "FAIL secret.*wasm" revision @URL "$work/step3-header"

echo "== deploy.yml: steps 4 and 5 (check the revision, then the traffic)"
reset_stub "registry/learn-pali:h-old"
step secret > /dev/null
out="$(step check URL=https://sha-abc1234---x.a.run.app)"
rc=$?
assert_eq "check: passes when rollout-check does" "0" "$rc"
assert_match "check: rollout-check revision URL HEADER_FILE" "rollout-check revision https://sha-abc1234---x.a.run.app $RUNNER_TEMP/origin-header" "$(calls)"
assert_match "check: the header file was 0600 with the header line" "header-file -rw------- x-origin-auth: " "$(calls)"
printf '1' > "$STUB/rc-revision"
out="$(step check URL=https://sha-abc1234---x.a.run.app)"
rc=$?
assert_eq "check: a failed revision check fails the step" "1" "$rc"
assert_absent "check: and the traffic is untouched" "update-traffic" "$(calls)"
rm -f "$STUB/rc-revision"
printf 'learn-pali-dev-00003-zzz' > "$STUB/latest-ready"
out="$(step traffic REVISION=learn-pali-dev-00002-bbb)"
rc=$?
assert_eq "traffic: moves to the checked revision" "0" "$rc"
assert_match "traffic: --to-revisions the revision step 2 created, by name, at 100" "update-traffic learn-pali-dev .*--to-revisions=learn-pali-dev-00002-bbb=100" "$(calls)"
assert_eq "traffic: a newer revision created meanwhile gets nothing" "learn-pali-dev-00002-bbb" "$(cat "$STUB/serving")"
out="$(step traffic REVISION=)"
rc=$?
assert_eq "traffic: no revision name fails the step" "1" "$rc"

echo "== deploy.yml: step 6a (the bare origin refuses; else back to PREV)"
reset_stub "registry/learn-pali:h-old"
printf 'learn-pali-dev-00002-bbb' > "$STUB/serving"
out="$(step refused PREVIOUS=learn-pali-dev-00001-aaa MOVED=true)"
rc=$?
assert_eq "refused: passes when the origin refuses" "0" "$rc"
assert_match "refused: --refused on the service URL" "rollout-check --refused https://learn-pali-dev-x.a.run.app" "$(calls)"
printf '1' > "$STUB/rc-refused"
out="$(step refused PREVIOUS=learn-pali-dev-00001-aaa MOVED=true)"
rc=$?
assert_eq "refused: a bare origin that answers fails the job" "1" "$rc"
assert_match "refused: traffic goes back to PREV" "update-traffic learn-pali-dev .*--to-revisions=learn-pali-dev-00001-aaa=100" "$(calls)"
assert_eq "refused: PREV serves again" "learn-pali-dev-00001-aaa" "$(cat "$STUB/serving")"
assert_match "refused: and says so" "moved back to learn-pali-dev-00001-aaa" "$out"
printf 'learn-pali-dev-00002-bbb' > "$STUB/serving"
touch "$STUB/rollback-fails"
out="$(step refused PREVIOUS=learn-pali-dev-00001-aaa MOVED=true)"
rc=$?
assert_eq "refused: a rollback that does not restore PREV exits 5" "5" "$rc"
assert_match "refused: and says so" "rollback did not restore" "$out"
reset_stub "registry/learn-pali:h-old"
printf '1' > "$STUB/rc-refused"
out="$(step refused PREVIOUS=learn-pali-dev-00001-aaa MOVED=false)"
rc=$?
assert_eq "refused: nothing moved, nothing to put back, still fails" "1" "$rc"
assert_absent "refused: no traffic call" "update-traffic" "$(calls)"

echo "== deploy.yml: step 6b (the public host through the Worker; retries; 4 never rolls back; 1 does)"
host_step() { step host PREVIOUS=learn-pali-dev-00001-aaa MOVED=true "$@"; }
count() { grep -c -- "$1" "$STUB/calls"; }
reset_stub "registry/learn-pali:h-old"
printf 'learn-pali-dev-00002-bbb' > "$STUB/serving"
out="$(host_step)"
rc=$?
assert_eq "host: a healthy host passes" "0" "$rc"
assert_match "host: --via-worker on the dev host" "rollout-check --via-worker https://learn.dev.palicanon.app" "$(calls)"
assert_eq "host: one try when it passes" "1" "$(count 'rollout-check --via-worker')"
assert_absent "host: no --access call" "rollout-check --access" "$(calls)"
assert_absent "host: no traffic call when it passes" "update-traffic" "$(calls)"
assert_eq "host: the traffic stays on the new revision" "learn-pali-dev-00002-bbb" "$(cat "$STUB/serving")"

reset_stub "registry/learn-pali:h-old"
out="$(host_step ENV=prod HOST=learn.palicanon.app)"
rc=$?
assert_eq "host: prod is checked too (no Access, no skip)" "0" "$rc"
assert_match "host: --via-worker on the prod host" "rollout-check --via-worker https://learn.palicanon.app" "$(calls)"

reset_stub "registry/learn-pali:h-old"
printf '1\n1\n0\n' > "$STUB/rc-via-worker.seq"
out="$(host_step)"
rc=$?
assert_eq "host: exit 1 twice then 0 passes (the edge settles)" "0" "$rc"
assert_eq "host: three tries" "3" "$(count 'rollout-check --via-worker')"
assert_eq "host: waits between the tries" "2" "$(count '^sleep 3')"
assert_absent "host: a late pass moves no traffic" "update-traffic" "$(calls)"

reset_stub "registry/learn-pali:h-old"
printf 'learn-pali-dev-00002-bbb' > "$STUB/serving"
printf '1' > "$STUB/rc-via-worker"
out="$(host_step)"
rc=$?
assert_eq "host: exit 1 five times fails the job with 1" "1" "$rc"
assert_eq "host: five tries" "5" "$(count 'rollout-check --via-worker')"
assert_match "host: the traffic goes back to PREV" "update-traffic learn-pali-dev .*--to-revisions=learn-pali-dev-00001-aaa=100" "$(calls)"
assert_eq "host: PREV serves again" "learn-pali-dev-00001-aaa" "$(cat "$STUB/serving")"
assert_match "host: and says so" "moved back to learn-pali-dev-00001-aaa" "$out"

reset_stub "registry/learn-pali:h-old"
printf 'learn-pali-dev-00002-bbb' > "$STUB/serving"
printf '1' > "$STUB/rc-via-worker"
touch "$STUB/rollback-fails"
out="$(host_step)"
rc=$?
assert_eq "host: a rollback that does not restore PREV exits 5" "5" "$rc"
assert_match "host: and says so" "rollback did not restore" "$out"

reset_stub "registry/learn-pali:h-old"
printf '1' > "$STUB/rc-via-worker"
out="$(step host PREVIOUS=learn-pali-dev-00001-aaa MOVED=false)"
rc=$?
assert_eq "host: nothing moved, nothing to put back, still fails" "1" "$rc"
assert_absent "host: no traffic call when this run moved none" "update-traffic" "$(calls)"

reset_stub "registry/learn-pali:h-old"
printf '1' > "$STUB/rc-via-worker"
out="$(step host PREVIOUS= MOVED=true)"
rc=$?
assert_eq "host: no previous revision, nothing to put back, still fails" "1" "$rc"
assert_absent "host: no traffic call without a previous revision" "update-traffic" "$(calls)"

reset_stub "registry/learn-pali:h-old"
printf '4' > "$STUB/rc-via-worker"
out="$(host_step)"
rc=$?
assert_eq "host: exit 4 (the Worker's own 500) fails the job with 4" "4" "$rc"
assert_eq "host: exit 4 is not retried" "1" "$(count 'rollout-check --via-worker')"
assert_absent "host: exit 4 does not touch the traffic" "update-traffic" "$(calls)"
assert_match "host: exit 4 is an error annotation" "::error::" "$out"
assert_match "host: exit 4 names the Worker's 500 and the remedy for this env" "Worker answered 500.*origin-secret push dev" "$out"
out="$(host_step ENV=prod HOST=learn.palicanon.app)"
assert_match "host: the remedy names prod on prod" "origin-secret push prod" "$out"

reset_stub "registry/learn-pali:h-old"
printf '3' > "$STUB/rc-via-worker"
out="$(host_step)"
rc=$?
assert_eq "host: exit 3 (a Cloudflare challenge) only warns" "0" "$rc"
assert_match "host: as a warning" "::warning::" "$out"
assert_eq "host: exit 3 is not retried" "1" "$(count 'rollout-check --via-worker')"
assert_absent "host: exit 3 does not touch the traffic" "update-traffic" "$(calls)"

reset_stub "registry/learn-pali:h-old"
printf '64' > "$STUB/rc-via-worker"
out="$(host_step)"
rc=$?
assert_eq "host: another code (usage) fails the job with it" "64" "$rc"
assert_eq "host: another code is not retried" "1" "$(count 'rollout-check --via-worker')"
assert_absent "host: another code does not touch the traffic" "update-traffic" "$(calls)"

# D2 (034 round 1): exit 6 is the edge or the Worker failing (no answer, a
# 52x, a 403 through the Worker): the revision is not at fault, so no
# rollback, no retry, the job fails naming the edge.
reset_stub "registry/learn-pali:h-old"
printf 'learn-pali-dev-00002-bbb' > "$STUB/serving"
printf '6' > "$STUB/rc-via-worker"
out="$(host_step)"
rc=$?
assert_eq "host: exit 6 (the edge or the Worker failed) fails the job with 6" "6" "$rc"
assert_eq "host: exit 6 is not retried" "1" "$(count 'rollout-check --via-worker')"
assert_absent "host: exit 6 does not touch the traffic (no rollback)" "update-traffic" "$(calls)"
assert_eq "host: exit 6 leaves the new revision serving" "learn-pali-dev-00002-bbb" "$(cat "$STUB/serving")"
assert_match "host: exit 6 is an error annotation" "::error::" "$out"
assert_match "host: exit 6 names the edge and the Worker, not the revision" "::error::.*edge.*Worker" "$out"
assert_match "host: exit 6 names the remedy for this env" "origin-secret push dev" "$out"
assert_absent "host: exit 6 does not say the traffic moved back" "moved back" "$out"

# E1 (034 round 2): exit 7 is the host sitting behind Access while the
# workflow's ACCESS_<env> says false: the job fails naming the setting, with
# no rollback and no retry.
reset_stub "registry/learn-pali:h-old"
printf 'learn-pali-dev-00002-bbb' > "$STUB/serving"
printf '7' > "$STUB/rc-via-worker"
out="$(host_step)"
rc=$?
assert_eq "host: exit 7 (Access in front, ACCESS_dev false) fails the job with 7" "7" "$rc"
assert_eq "host: exit 7 is not retried" "1" "$(count 'rollout-check --via-worker')"
assert_absent "host: exit 7 does not touch the traffic (no rollback)" "update-traffic" "$(calls)"
assert_eq "host: exit 7 leaves the new revision serving" "learn-pali-dev-00002-bbb" "$(cat "$STUB/serving")"
assert_match "host: exit 7 is an error annotation naming ACCESS_dev" "::error::.*ACCESS_dev" "$out"
assert_match "host: exit 7 says to match infra" "::error::.*infra" "$out"
assert_absent "host: exit 7 does not say the traffic moved back" "moved back" "$out"

# ET2 (034 round 2): a new env with no ACCESS_<env> in the workflow env fails
# naming that variable, before any check and any rollback.
reset_stub "registry/learn-pali:h-old"
printf 'learn-pali-staging-00002-bbb' > "$STUB/serving"
out="$(host_step ENV=staging HOST=learn.staging.palicanon.app)"
rc=$?
assert_eq "host: an env with no ACCESS_<env> fails the job" "1" "$rc"
assert_match "host: and names ACCESS_staging" "::error::.*ACCESS_staging" "$out"
assert_eq "host: and calls no rollout-check" "0" "$(count 'rollout-check')"
assert_absent "host: and does not roll back" "update-traffic" "$(calls)"

# D1/T1 (034 round 1): an env whose access setting is true is checked by
# --access (never --via-worker): a 302 passes, 2 and 4 fail named, 3 warns,
# and none of them rolls the traffic back (the revision did not fail).
reset_stub "registry/learn-pali:h-old"
printf 'learn-pali-dev-00002-bbb' > "$STUB/serving"
out="$(host_step ACCESS_dev=true)"
rc=$?
assert_eq "host (access): a 302 to cloudflareaccess.com (--access exit 0) passes" "0" "$rc"
assert_match "host (access): --access on the dev host" "rollout-check --access https://learn.dev.palicanon.app" "$(calls)"
assert_absent "host (access): --via-worker is not called" "rollout-check --via-worker" "$(calls)"
assert_eq "host (access): one try" "1" "$(count 'rollout-check --access')"
assert_absent "host (access): no traffic call, no rollback" "update-traffic" "$(calls)"

reset_stub "registry/learn-pali:h-old"
out="$(host_step ENV=prod HOST=learn.palicanon.app ACCESS_prod=true)"
rc=$?
assert_eq "host (access): prod follows ACCESS_prod" "0" "$rc"
assert_match "host (access): --access on the prod host" "rollout-check --access https://learn.palicanon.app" "$(calls)"
reset_stub "registry/learn-pali:h-old"
out="$(host_step ENV=prod HOST=learn.palicanon.app ACCESS_prod=false ACCESS_dev=true)"
assert_match "host (access): prod with ACCESS_prod=false is --via-worker even when dev has Access" "rollout-check --via-worker https://learn.palicanon.app" "$(calls)"
assert_absent "host (access): and not --access" "rollout-check --access" "$(calls)"

reset_stub "registry/learn-pali:h-old"
printf 'learn-pali-dev-00002-bbb' > "$STUB/serving"
printf '2' > "$STUB/rc-access"
out="$(host_step ACCESS_dev=true)"
rc=$?
assert_eq "host (access): exit 2 (Access missing) fails the job with 2" "2" "$rc"
assert_match "host (access): exit 2 is an error naming Access" "::error::.*Access" "$out"
assert_absent "host (access): exit 2 never rolls back" "update-traffic" "$(calls)"
assert_eq "host (access): exit 2 is not retried" "1" "$(count 'rollout-check --access')"
assert_eq "host (access): exit 2 leaves the new revision serving" "learn-pali-dev-00002-bbb" "$(cat "$STUB/serving")"

reset_stub "registry/learn-pali:h-old"
printf '4' > "$STUB/rc-access"
out="$(host_step ACCESS_dev=true)"
rc=$?
assert_eq "host (access): exit 4 (anything else) fails the job with 4" "4" "$rc"
assert_match "host (access): exit 4 is an error annotation" "::error::" "$out"
assert_absent "host (access): exit 4 never rolls back" "update-traffic" "$(calls)"

reset_stub "registry/learn-pali:h-old"
printf '3' > "$STUB/rc-access"
out="$(host_step ACCESS_dev=true)"
rc=$?
assert_eq "host (access): exit 3 (a Cloudflare challenge) only warns" "0" "$rc"
assert_match "host (access): exit 3 as a warning" "::warning::" "$out"
assert_absent "host (access): exit 3 never rolls back" "update-traffic" "$(calls)"

reset_stub "registry/learn-pali:h-old"
printf '64' > "$STUB/rc-access"
out="$(host_step ACCESS_dev=true)"
rc=$?
assert_eq "host (access): another code (usage) fails the job with it" "64" "$rc"
assert_absent "host (access): another code never rolls back" "update-traffic" "$(calls)"

reset_stub "registry/learn-pali:h-old"
out="$(host_step ACCESS_dev=maybe)"
rc=$?
assert_eq "host: an access setting that is not true or false fails the job" "1" "$rc"
assert_match "host: and names the setting" "::error::.*ACCESS_dev" "$out"
assert_eq "host: and runs no check" "0" "$(count 'rollout-check')"

# ------------------------------------------------------------ ci.yml: job infra
# (034 round 1, T3): nothing else in CI runs the script tests, the Worker's
# tests or tofu; the job `infra` runs each, with no ui-lib checkout.
echo "== ci.yml: the infra job"
python3 - "$root/.github/workflows/ci.yml" > "$work/infra-asserts" <<'PY'
import sys, yaml
wf = yaml.safe_load(open(sys.argv[1]))
out = []
def t(name, cond): out.append(("ok" if cond else "FAIL") + "\t" + name)
job = wf["jobs"].get("infra")
t("ci.yml: a job 'infra' exists", job is not None)
job = job or {"steps": []}
st = job.get("steps", [])
runs = [str(x.get("run", "")) for x in st]
uses = [str(x.get("uses", "")) for x in st]
t("ci.yml: infra runs on ubuntu-latest", job.get("runs-on") == "ubuntu-latest")
co = [x for x in st if str(x.get("uses", "")).startswith("actions/checkout@")]
t("ci.yml: infra checks learn-pali out at path learn-pali (defaults.run holds)",
  len(co) == 1 and co[0].get("with", {}).get("path") == "learn-pali")
t("ci.yml: infra takes no ui-lib checkout", not any("ui-lib" in u for u in uses))
ot = [x for x in st if str(x.get("uses", "")).startswith("opentofu/setup-opentofu@")]
t("ci.yml: infra sets OpenTofu up (opentofu/setup-opentofu@v1)", len(ot) == 1 and ot[0]["uses"] == "opentofu/setup-opentofu@v1")
ver = str(ot[0].get("with", {}).get("tofu_version", "")) if ot else ""
import re
m = re.match(r"^1\.(\d+)\.\d+$", ver)
t("ci.yml: infra pins a 1.x OpenTofu at least 1.8 (versions.tf's floor), got '%s'" % ver, bool(m) and int(m.group(1)) >= 8)
node = [x for x in st if str(x.get("uses", "")).startswith("actions/setup-node@")]
t("ci.yml: infra sets node up (actions/setup-node)", len(node) == 1)
nv = str(node[0].get("with", {}).get("node-version", "")) if node else ""
t("ci.yml: infra's node is 22 or later, got '%s'" % nv, nv.split(".")[0].isdigit() and int(nv.split(".")[0]) >= 22)
for cmd in ("tofu fmt -check -recursive infra",
            "tofu -chdir=infra init -backend=false",
            "tofu -chdir=infra validate",
            "node --test infra/worker/proxy.test.mjs",
            "bash scripts/rollout-check.test.sh",
            "bash infra/scripts/origin-secret.test.sh"):
    t("ci.yml: infra runs `%s`" % cmd, any(cmd in r.split("\n") or r.strip() == cmd for r in runs))
print("\n".join(out))
PY
while IFS=$'\t' read -r verdict name; do
  if [ "$verdict" = ok ]; then pass "$name"; else flunk "$name"; fi
done < "$work/infra-asserts"
assert_eq "the infra asserts ran (not an empty list)" "yes" "$([ "$(grep -c '' "$work/infra-asserts")" -gt 10 ] && echo yes || echo no)"

# ------------------------------------------- the ui-lib credential is the App
# (task 031, design v7): the composite action mints a 1-hour token for
# PaliCanon/ui-lib only, checks it out with it and loads no ssh key; every job
# of ci.yml and deploy.yml passes the App's id and private key.
echo "== the ui-lib action: the GitHub App token, no ssh key"
action_file="$root/.github/actions/ui-lib/action.yml"
ghdir="$root/.github"
python3 - "$action_file" "$root/.github/workflows/ci.yml" "$root/.github/workflows/deploy.yml" "$work/lockguard.sh" > "$work/app-asserts" <<'PY'
import sys, yaml
act = yaml.safe_load(open(sys.argv[1]))
out = []
def t(name, cond): out.append(("ok" if cond else "FAIL") + "\t" + name)
inputs = act.get("inputs", {})
steps = act["runs"]["steps"]
for k in ("app-id", "private-key"):
    t("action: input %s is required and documented" % k,
      k in inputs and inputs[k].get("required") is True and bool(inputs[k].get("description")))
t("action: no ssh-key input", "ssh-key" not in inputs)
app = [s for s in steps if s.get("id") == "app"]
t("action: a step with id app", len(app) == 1)
a = app[0] if app else {}
t("action: id app uses actions/create-github-app-token@v2", a.get("uses") == "actions/create-github-app-token@v2")
w = a.get("with", {})
t("action: the token is minted from the app-id input", w.get("app-id") == "${{ inputs.app-id }}")
t("action: the token is minted from the private-key input", w.get("private-key") == "${{ inputs.private-key }}")
t("action: the token's owner is PaliCanon", w.get("owner") == "PaliCanon")
t("action: the token reads ui-lib only", str(w.get("repositories", "")).split() == ["ui-lib"])
t("action: the token asks Contents read", w.get("permission-contents") == "read")
t("action: the token asks no other permission", [k for k in w if k.startswith("permission-")] == ["permission-contents"])
co = [s for s in steps if str(s.get("uses", "")).startswith("actions/checkout@")]
t("action: one checkout", len(co) == 1)
c = co[0].get("with", {}) if co else {}
t("action: the checkout is PaliCanon/ui-lib", c.get("repository") == "PaliCanon/ui-lib")
t("action: the checkout uses the App's token", c.get("token") == "${{ steps.app.outputs.token }}")
t("action: the checkout persists no credential", c.get("persist-credentials") is False)
t("action: the checkout has no ssh-key", "ssh-key" not in c)
ids = [s.get("id") for s in steps]
t("action: the token is minted before the checkout",
  bool(app) and bool(co) and steps.index(app[0]) < steps.index(co[0]))
t("action: the version guard is still there",
  any("version" in str(s.get("run", "")) and "says version" in str(s.get("run", "")) for s in steps))
t("action: no ssh-agent step", not any("ssh-agent" in str(s.get("uses", "")) for s in steps))
guards = [s for s in steps if "Cargo.lock" in str(s.get("run", "")) and "git+" in str(s.get("run", ""))]
t("action: one step guards the lock against a git source", len(guards) == 1)
t("action: the lock guard runs before the checkout",
  bool(guards) and bool(co) and steps.index(guards[0]) < steps.index(co[0]))
open(sys.argv[4], "w").write(str(guards[0]["run"]) if guards else "exit 0\n")
for path in sys.argv[2:4]:
    wf = yaml.safe_load(open(path))
    name = path.rsplit("/", 1)[1]
    n = 0
    for job, spec in wf["jobs"].items():
        st = spec.get("steps", [])
        mine = any(str(s.get("uses", "")).startswith("actions/checkout@") and s.get("with", {}).get("path") == "learn-pali" for s in st)
        uses = [s for s in st if s.get("uses") == "./learn-pali/.github/actions/ui-lib"]
        if mine:
            n += 1
            # the job `infra` runs no cargo: it takes no ui-lib (034 round 1)
            if job == "infra":
                t("%s: job %s takes no ui-lib" % (name, job), len(uses) == 0)
                continue
            t("%s: job %s uses the ui-lib action" % (name, job), len(uses) == 1)
        for u in uses:
            w = u.get("with", {})
            t("%s: job %s passes the App id" % (name, job), w.get("app-id") == "${{ secrets.APP_ID }}")
            t("%s: job %s passes the App private key" % (name, job), w.get("private-key") == "${{ secrets.APP_PRIVATE_KEY }}")
            t("%s: job %s passes no ssh-key" % (name, job), "ssh-key" not in w)
    t("%s: at least one job checks learn-pali out" % name, n >= 1)
print("\n".join(out))
PY
while IFS=$'\t' read -r verdict name; do
  if [ "$verdict" = ok ]; then pass "$name"; else flunk "$name"; fi
done < "$work/app-asserts"
# A3: the lock guard's script, run in a temp dir holding learn-pali/Cargo.lock.
lock_guard() { # name lockfile-content want-exit [want-text]
  local d="$work/lg-$1"
  mkdir -p "$d/learn-pali"
  printf '%s\n' "$2" > "$d/learn-pali/Cargo.lock"
  out="$(cd "$d" && bash "$work/lockguard.sh" 2>&1)"
  rc=$?
  assert_eq "lock guard: $1 exits $3" "$3" "$rc"
  [ -z "${4:-}" ] || assert_match "lock guard: $1 names Cargo.lock" "$4" "$out"
}
lock_guard clean $'[[package]]\nname = "x"\nsource = "registry+https://github.com/rust-lang/crates.io-index"' 0
lock_guard git $'[[package]]\nname = "x"\nsource = "git+https://github.com/PaliCanon/ui-lib?tag=v0.1.0#abc"' 1 'learn-pali/Cargo\.lock holds a git source'
lock_guard git-error-annotation $'source = "git+ssh://git@github.com/x/y#1"' 1 '^::error::'
assert_eq "the App asserts ran (not an empty list)" "yes" "$([ "$(grep -c '' "$work/app-asserts")" -gt 30 ] && echo yes || echo no)"
assert_eq "no webfactory/ssh-agent anywhere in .github/" "0" "$(grep -rc 'webfactory/ssh-agent' "$ghdir" | awk -F: '{n += $2} END {print n + 0}')"
assert_eq "UI_LIB_DEPLOY_KEY appears nowhere in .github/" "0" "$(grep -rc 'UI_LIB_DEPLOY_KEY' "$ghdir" | awk -F: '{n += $2} END {print n + 0}')"
assert_eq "no ssh-key input or ssh-known-hosts anywhere in .github/" "0" "$(grep -rEc '^[[:space:]]*(ssh-key|ssh-known-hosts|ssh-private-key):' "$ghdir" | awk -F: '{n += $2} END {print n + 0}')"

# R3-T4: rollout-check.sh --access and origin-secret check read the SAME file.
# One tree, scripts/ and infra/scripts/ as in the repository, ONE pattern file.
echo "== one pattern file for both scripts"
one="$work/one"
mkdir -p "$one/scripts" "$one/infra/scripts" "$one/bin"
cp "$here/rollout-check.sh" "$one/scripts/rollout-check.sh"
cp "$here/access-redirect-pattern" "$one/scripts/access-redirect-pattern"
cp "$root/infra/scripts/origin-secret" "$one/infra/scripts/origin-secret"
cat > "$one/bin/gcloud" <<'SH'
#!/usr/bin/env bash
case "$*" in
  *"versions list"*) printf '1' ;;
  *"versions access"*) printf 'stubbed-value' ;;
esac
SH
cat > "$one/bin/curl" <<'SH'
#!/usr/bin/env bash
# `--config -`: the Worker's secrets list; anything else: the host check.
if [ "${1:-}" = "--config" ] && [ "${2:-}" = "-" ]; then
  cat > /dev/null
  printf '{"result":[{"name":"ORIGIN_AUTH","type":"secret_text"}],"success":true}\n200'
else
  printf '302 https://team.cloudflareaccess.com/cdn-cgi/access/login/learn.dev.palicanon.app?kid=1'
fi
SH
chmod +x "$one/bin/gcloud" "$one/bin/curl"
origin_check() { # -> $out, $code (origin-secret check dev, ACCESS=1: the Access branch, from the one tree)
  out="$(env PATH="$one/bin:$PATH" CLOUDFLARE_API_TOKEN=cf-token-for-tests ACCESS=1 "$one/infra/scripts/origin-secret" check dev 2>&1)"
  rc=$?
}
real_script="$script"
script="$one/scripts/rollout-check.sh"
run_case "one pattern: rollout-check --access passes on a valid redirect" access_ok 0 "access ok" --access @URL
origin_check
assert_eq "one pattern: origin-secret check passes on a valid redirect" "OK" "$out"
printf '%s\n' '^https://never\.example/' > "$one/scripts/access-redirect-pattern"
run_case "one pattern: the changed file makes rollout-check --access fail" access_ok 4 "FAIL access" --access @URL
origin_check
assert_eq "one pattern: the changed file makes origin-secret check fail (exit)" "1" "$rc"
assert_match "one pattern: and it names the access-redirect check" "access-redirect" "$out"
cp "$here/access-redirect-pattern" "$one/scripts/access-redirect-pattern"
run_case "one pattern: restored, rollout-check --access passes again" access_ok 0 "access ok" --access @URL
origin_check
assert_eq "one pattern: restored, origin-secret check passes again" "OK" "$out"
script="$real_script"

echo
echo "$checks assertions, $failed failed"
[ "$failed" = 0 ]
