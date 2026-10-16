#!/usr/bin/env bash
# The app's feature set holds nothing of the site's web store (task 048):
# builds `learn_pali` for wasm32 with `csr` alone and fails when the
# `localStorage` store or the Blob / file-input transfer (src/web.rs) is in
# it. The positive control: the `hydrate` build, which has them, shows the
# same markers, so the check cannot pass by looking for names that never
# appear. The markers are the names only src/web.rs holds, found in the
# built rlib: the storage read (`get_item`), the Blob URL
# (`create_object_url`), the file read (`read_as_text`), the hidden input
# (`hidden_file_input`) and the store type (`WebStore`); and, for the
# site's Google Drive sync (src/web_sync, task 050), the token client's
# name (`initTokenClient`).
#
# Run from anywhere: scripts/csr-without-web-store.sh. Needs the wasm32 target.
set -euo pipefail

root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$root"
markers=(get_item create_object_url read_as_text hidden_file_input WebStore initTokenClient)
work="$(mktemp -d)"
trap 'rm -rf "$work"' EXIT

build() { # build <features> <copy>
  cargo build --locked -p learn_pali --lib --features "$1" --target wasm32-unknown-unknown
  cp target/wasm32-unknown-unknown/debug/liblearn_pali.rlib "$2"
}

build hydrate "$work/hydrate.rlib"
build csr "$work/csr.rlib"

status=0
for marker in "${markers[@]}"; do
  with="$(grep -a -c "$marker" "$work/hydrate.rlib" || true)"
  without="$(grep -a -c "$marker" "$work/csr.rlib" || true)"
  if [ "$with" -eq 0 ]; then
    echo "control failed: '$marker' is not in the hydrate build, the check proves nothing" >&2
    status=1
  elif [ "$without" -ne 0 ]; then
    echo "'$marker' is in the csr build: the web store leaked into the app's feature set" >&2
    status=1
  else
    echo "ok: '$marker' in hydrate ($with), not in csr"
  fi
done
exit "$status"
