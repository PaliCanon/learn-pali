#!/usr/bin/env bash
# The fetch script (070): the lock's digest is checked, a mismatch leaves no
# file, a bad lock is refused, and the Dockerfile reads the asset where the
# workflow puts it.
set -euo pipefail
here=$(cd "$(dirname "$0")" && pwd)
root=$(cd "$here/.." && pwd)
tmp=$(mktemp -d)
trap 'rm -rf "$tmp"' EXIT
fail() { echo "FAIL: $*" >&2; exit 1; }
printf 'hello\n' > "$tmp/object"
sha=$(sha256sum "$tmp/object" | cut -c1-64)
fake="$tmp/fake-fetch"
printf '#!/usr/bin/env bash\ncp "%s" "$2"\n' "$tmp/object" > "$fake"; chmod +x "$fake"
printf 'passages passages-test.db %s\n' "$sha" > "$tmp/good.lock"
FETCH="$fake" bash "$here/fetch-passages.sh" "$tmp/good.lock" "$tmp/out/passages.db" >/dev/null || fail "a matching digest is accepted"
[ -f "$tmp/out/passages.db" ] || fail "the asset lands at the out path"
printf 'passages passages-test.db %s\n' "$(printf '%064d' 0)" > "$tmp/bad.lock"
if FETCH="$fake" bash "$here/fetch-passages.sh" "$tmp/bad.lock" "$tmp/out2/passages.db" 2>/dev/null; then fail "a wrong digest is refused"; fi
[ ! -f "$tmp/out2/passages.db" ] || fail "a refused asset leaves no file"
printf 'passages passages-test.db abc\n' > "$tmp/short.lock"
if FETCH="$fake" bash "$here/fetch-passages.sh" "$tmp/short.lock" "$tmp/out3/passages.db" 2>/dev/null; then fail "a short digest is refused"; fi
printf 'nothing here\n' > "$tmp/none.lock"
if FETCH="$fake" bash "$here/fetch-passages.sh" "$tmp/none.lock" "$tmp/out4/passages.db" 2>/dev/null; then fail "a lock with no passages line is refused"; fi
printf 'stale\n' > "$tmp/out/passages.db"
FETCH="$fake" bash "$here/fetch-passages.sh" "$tmp/good.lock" "$tmp/out/passages.db" >/dev/null || fail "a rerun over a stale file succeeds"
[ "$(cat "$tmp/out/passages.db")" = "hello" ] || fail "a rerun replaces the stale file"
grep -qE '^passages passages-[0-9-]+\.db [0-9a-f]{64}$' "$root/app/assets.lock" || fail "app/assets.lock names the asset and a 64-hex digest"
grep -q 'bash scripts/fetch-passages.sh app/assets.lock assets/passages.db' "$root/.github/workflows/deploy.yml" || fail "the workflow fetches to assets/passages.db"
grep -q 'COPY --chown=app:app learn-pali/assets/passages.db ./passages.db' "$root/app/Dockerfile" || fail "the Dockerfile copies the fetched asset"
grep -q 'LEARN_PALI_PASSAGES=/app/passages.db' "$root/app/Dockerfile" || fail "the image names the asset to the server"
echo "fetch-passages.test.sh: ok"
