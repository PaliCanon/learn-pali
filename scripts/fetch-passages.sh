#!/usr/bin/env bash
# Fetches the passages asset named by a lock file (070) and checks its
# SHA-256: `fetch-passages.sh <lock> <out>`. The lock's last line is
# `passages <object> <sha256>`; the object lives under
# gs://palicanon-app-assets/passages/. `FETCH=<command>` replaces the
# gcloud copy (tests): it is run as `$FETCH <uri> <out>`.
set -euo pipefail
lock=$1
out=$2
read -r kind object sha < <(grep -E '^passages ' "$lock" | tail -1)
[ "$kind" = passages ] || { echo "fetch-passages: no passages line in $lock" >&2; exit 2; }
[[ "$sha" =~ ^[0-9a-f]{64}$ ]] || { echo "fetch-passages: the digest in $lock is not 64 hex" >&2; exit 2; }
mkdir -p "$(dirname "$out")"
uri="gs://palicanon-app-assets/passages/$object"
if [ -n "${FETCH:-}" ]; then
  $FETCH "$uri" "$out"
else
  gcloud storage cp --no-user-output-enabled "$uri" "$out"
fi
got=$(sha256sum "$out" | cut -c1-64)
if [ "$got" != "$sha" ]; then
  rm -f "$out"
  echo "fetch-passages: $object has digest $got, the lock says $sha" >&2
  exit 1
fi
echo "fetch-passages: $object ($sha) at $out"
