#!/usr/bin/env bash
# Tests for app/Dockerfile and the ui-lib composite action
# (.github/actions/ui-lib/action.yml). Run: scripts/image.test.sh
#
#   1. the Dockerfile: no ssh mount, no known_hosts, the git-source guard
#      (run against lockfiles), the 403 smoke check, non-root, LEPTOS_ENV=PROD.
#   2. the ui-lib action: the GitHub App token (no ssh key), the lock guard
#      (run against lockfiles), and every job of ci.yml passes the App's id
#      and private key.
#
# Moved out of rollout-check.test.sh when Cloud Run and the Worker went (083).
# Needs bash, python3 with PyYAML, jq. Exit 0 when every assertion held.
set -uo pipefail

here="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
root="$(cd "$here/.." && pwd)"
dockerfile="$root/app/Dockerfile"

work="$(mktemp -d)"
cleanup() { rm -rf "$work"; }
trap cleanup EXIT

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
assert_file_has() { # name file pattern
  if grep -Eq -- "$3" "$2" 2>/dev/null; then pass "$1"; else flunk "$1 (/$3/ not in $2)"; fi
}
assert_file_lacks() { # name file pattern
  if grep -Eq -- "$3" "$2" 2>/dev/null; then flunk "$1 (/$3/ found in $2)"; else pass "$1"; fi
}

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

# ------------------------------------------- the ui-lib credential is the App
# (task 031, design v7): the composite action mints a 1-hour token for
# PaliCanon/ui-lib only, checks it out with it and loads no ssh key; every job
# of ci.yml passes the App's id and private key.
echo "== the ui-lib action: the GitHub App token, no ssh key"
action_file="$root/.github/actions/ui-lib/action.yml"
ghdir="$root/.github"
python3 - "$action_file" "$root/.github/workflows/ci.yml" "$work/lockguard.sh" > "$work/app-asserts" <<'PY'
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
open(sys.argv[3], "w").write(str(guards[0]["run"]) if guards else "exit 0\n")
for path in sys.argv[2:3]:
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

echo
echo "$checks assertions, $failed failed"
[ "$failed" = 0 ]
