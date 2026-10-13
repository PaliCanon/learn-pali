#!/usr/bin/env bash
# Tests for scripts/rollout-check.sh, app/Dockerfile and .github/workflows/
# deploy.yml (task 031, slice R). Run: scripts/rollout-check.test.sh
#
#   1. rollout-check.sh against a python3 fake server that plays every way a
#      deployment can be wrong: each check has a server that must fail it, the
#      failure names the check, and the origin secret is in no output and no
#      curl argument (a curl shim records them).
#   2. the Dockerfile: no ssh mount, no known_hosts, the git-source guard
#      (run against lockfiles), non-root, LEPTOS_ENV=PROD.
#   3. deploy.yml: parses; its steps are run with a stub gcloud and a stub
#      rollout-check to prove PREV, the skip, the no-traffic deploy, the
#      secret step, the rollback (exit 5) and the host check (never a rollback).
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
import http.server, socketserver, sys, time

scenario, portfile, logfile = sys.argv[1:4]
SECRET = "origin_canary_rollout_9d41c7"
TOKEN = "pc_dev_A1b2C3d4E5f6G7h8I9j0K1l2M3n4O5p6Q7r8S9t0U1v"
assert len(TOKEN) == 7 + 43
WASM = "/pkg/learn-pali.4f2a9c.wasm"
ACCESS = "https://team.cloudflareaccess.com/cdn-cgi/access/login/learn.dev.palicanon.app?kid=1"


def page(name, link, extra):
    return ('<!DOCTYPE html><html><head><title>%s</title>%s</head><body>%s</body></html>'
            % (name, '<link rel="preload" href="%s" as="fetch" type="application/wasm">' % link if link else "", extra)).encode()


def respond(path, who):
    s = scenario
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

serve() { # scenario -> URL, SLOG, SRV_PID
  SLOG="$work/log.$RANDOM"
  local pf="$work/port.$RANDOM"
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
assert_eq "deploy.yml: the host check runs on dev only" "env.ENV == 'dev'" "$(step_field host if 2>/dev/null | sed 's/^\${{ *//; s/ *}}$//')"
assert_file_lacks "deploy.yml: no --ssh, no ssh: input on the image build" "$workflow" '^[[:space:]]*ssh:|--ssh'
assert_file_has "deploy.yml: the ui-lib action step is untouched" "$workflow" '^      - uses: \./learn-pali/\.github/actions/ui-lib$'
assert_file_has "deploy.yml: the action gets the App id" "$workflow" 'app-id: \$\{\{ secrets\.APP_ID \}\}'
assert_file_has "deploy.yml: the action gets the App private key" "$workflow" 'private-key: \$\{\{ secrets\.APP_PRIVATE_KEY \}\}'
for id in current deploy secret check traffic refused host; do
  assert_file_lacks "deploy.yml: '$id' has no \${{ }} in its script" "$work/step-$id.sh" '\$\{\{'
  assert_file_lacks "deploy.yml: '$id' never runs set -x" "$work/step-$id.sh" 'set -[a-z]*x'
done
assert_file_has "deploy.yml: the message names 'secret has no version?'" "$work/step-deploy.sh" 'secret has no version\?'
assert_file_has "deploy.yml: the deploy has no traffic" "$work/step-deploy.sh" '--no-traffic'
assert_file_lacks "deploy.yml: --to-latest appears nowhere" "$workflow" '--to-latest'
assert_file_has "deploy.yml: step 5 reads the revision step 2 created" "$workflow" 'REVISION: \$\{\{ steps\.deploy\.outputs\.revision \}\}'
assert_file_has "deploy.yml: PERMISSION_DENIED is named" "$work/step-secret.sh" 'deployer lacks secretAccessor on learn-pali-origin-secret-'
assert_file_has "deploy.yml: the secret is masked" "$work/step-secret.sh" '::add-mask::'
assert_file_has "deploy.yml: the secret file is 0600" "$work/step-secret.sh" 'umask 077'
assert_file_lacks "deploy.yml: the secret is never put in a shell variable" "$work/step-secret.sh" '(^|[^A-Z_])(VALUE|SECRET)=|\$\(gcloud secrets'
assert_file_has "deploy.yml: the host check never rolls back" "$work/step-host.sh" '--access'
assert_file_lacks "deploy.yml: no rollback in the host check" "$work/step-host.sh" 'rollback|update-traffic'
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
exit "$(cat "$STUB/rc-$mode" 2>/dev/null || echo 0)"
SH
chmod +x "$stub/bin/gcloud" "$proj/scripts/rollout-check.sh"

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
assert_match "deploy: and names the missing secret version" "secret has no version\?" "$out"
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

echo "== deploy.yml: step 6b (the host, Access only, never a rollback)"
reset_stub "registry/learn-pali:h-old"
out="$(step host PREVIOUS=learn-pali-dev-00001-aaa MOVED=true)"
rc=$?
assert_eq "host: 302 to Access passes" "0" "$rc"
assert_match "host: --access on the host" "rollout-check --access https://learn.dev.palicanon.app" "$(calls)"
for code in 2 4; do
  reset_stub "registry/learn-pali:h-old"
  printf '%s' "$code" > "$STUB/rc-access"
  out="$(step host PREVIOUS=learn-pali-dev-00001-aaa MOVED=true)"
  rc=$?
  assert_eq "host: exit $code fails the job with that code" "$code" "$rc"
  assert_absent "host: exit $code does not touch the traffic" "update-traffic" "$(calls)"
done
reset_stub "registry/learn-pali:h-old"
printf '3' > "$STUB/rc-access"
out="$(step host PREVIOUS=learn-pali-dev-00001-aaa MOVED=true)"
rc=$?
assert_eq "host: exit 3 (a Cloudflare challenge) only warns" "0" "$rc"
assert_match "host: as a warning" "::warning::" "$out"

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
origin_check() { # -> $out, $code (origin-secret check dev from the one tree)
  out="$(env PATH="$one/bin:$PATH" CLOUDFLARE_API_TOKEN=cf-token-for-tests "$one/infra/scripts/origin-secret" check dev 2>&1)"
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
