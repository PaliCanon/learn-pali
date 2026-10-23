#!/usr/bin/env bash
# Tests for infra/scripts/api-token (task 062): `gcloud` and `security` are
# stubs first on PATH that log their argv and stdin. The contract under
# test: the token travels from the keychain to gcloud's stdin only (never
# in an argument, never on screen), `create` is idempotent, `rotate` always
# adds, `check` prints OK or "not OK" with the name of the failing check.
# Run: bash infra/scripts/api-token.test.sh
set -uo pipefail

here=$(cd "$(dirname "$0")" && pwd)
script="$here/api-token"
work=$(mktemp -d)
trap 'rm -rf "$work"' EXIT
bin="$work/bin"
log="$work/log"
mkdir -p "$bin" "$log"

TOKEN="pc_dev_stubbedtoken0123456789abcdefghijklmnopqrstu"

cat >"$bin/gcloud" <<STUB
#!/usr/bin/env bash
printf '%s\n' "\$*" >>"$log/gcloud.argv"
case "\$*" in
  *"versions list"*) printf '%s' "\${STUB_VERSIONS:-}" ;;
  *"versions add"*) cat >"$log/gcloud.add.stdin" ;;
esac
STUB
cat >"$bin/security" <<STUB
#!/usr/bin/env bash
printf '%s\n' "\$*" >>"$log/security.argv"
if [ "\${STUB_KEYCHAIN:-yes}" = "no" ]; then exit 44; fi
printf '%s\n' "$TOKEN"
STUB
chmod +x "$bin/gcloud" "$bin/security"
export PATH="$bin:$PATH"

fails=0
check() { # check NAME CONDITION...
  local name=$1
  shift
  if "$@"; then echo "ok   $name"; else echo "FAIL $name"; fails=$((fails + 1)); fi
}
reset() { rm -f "$log"/*; }

# create, no version yet: the value goes to gcloud's stdin, nowhere else
reset
out=$(STUB_VERSIONS= bash "$script" create dev 2>&1)
code=$?
check "create exits 0" test "$code" -eq 0
check "create adds a version" grep -q "versions add learn-pali-api-token-dev" "$log/gcloud.argv"
check "the token is on stdin" test "$(cat "$log/gcloud.add.stdin")" = "$TOKEN"
check "the token is in no argument" bash -c "! grep -q '$TOKEN' '$log/gcloud.argv'"
check "the token is not on screen" bash -c "! printf '%s' \"\$1\" | grep -q '$TOKEN'" _ "$out"
check "the keychain service is the learn one" grep -q "pali-api-token-dev-learn" "$log/security.argv"

# create, a version exists: nothing added
reset
STUB_VERSIONS="projects/p/secrets/s/versions/1" bash "$script" create dev >/dev/null 2>&1
check "create is idempotent" bash -c "! grep -q 'versions add' '$log/gcloud.argv'"

# rotate always adds
reset
STUB_VERSIONS="projects/p/secrets/s/versions/1" bash "$script" rotate prod >/dev/null 2>&1
check "rotate adds to the prod secret" grep -q "versions add learn-pali-api-token-prod" "$log/gcloud.argv"

# create without the keychain value refuses
reset
STUB_KEYCHAIN=no bash "$script" create dev >/dev/null 2>&1
check "no keychain value: exit 1" test $? -eq 1
check "no keychain value: nothing added" bash -c "! grep -q 'versions add' '$log/gcloud.argv'"

# check
check "check OK" test "$(STUB_VERSIONS=v1 bash "$script" check dev 2>&1)" = "OK"
check "check names the missing version" test "$(STUB_VERSIONS= bash "$script" check dev 2>&1)" = "not OK version"
check "check names the missing keychain value" test "$(STUB_VERSIONS=v1 STUB_KEYCHAIN=no bash "$script" check dev 2>&1)" = "not OK keychain"

# usage
bash "$script" frob dev >/dev/null 2>&1
check "a bad command is usage (2)" test $? -eq 2
bash "$script" create staging >/dev/null 2>&1
check "a bad env is usage (2)" test $? -eq 2

if [ "$fails" -ne 0 ]; then echo "$fails failing"; exit 1; fi
echo "all ok"
