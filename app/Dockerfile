# syntax=docker/dockerfile:1.7
#
# learn-pali: the server and the site (WASM, CSS, assets) in one image.
#
# Build context: the FOLDER HOLDING `learn-pali/` AND `ui-lib/` (CI checks
# both out side by side, as ci.yml does). The workspace reaches the sibling:
# its `[patch]` to `../ui-lib`, `app/style/main.css` (`@import` of
# `../../../ui-lib/style/theme.css`) and `app/build.rs` (`#[path]` to
# `../../ui-lib/build/placeholders.rs`). So, from that folder:
#
#   docker build -f learn-pali/app/Dockerfile .
#
# NO CREDENTIAL IS NEEDED TO BUILD (task 031, as search-front 007). ui-lib is
# a private git dependency, but the workspace's `[patch]` replaces it with the
# sibling checkout COPYed below and `Cargo.lock` records it as a path package:
# the lock holds no git source, so Cargo fetches nothing from GitHub. The
# credential for the checkout lives in CI only; it is never an argument, a
# mount or a layer here. A RUN below fails the build if the lock ever gains a
# git source, instead of letting the build ask for a credential.
#
# There is no .dockerignore (it would live in the parent folder, outside this
# repository); every COPY below names what it takes instead.

# The image tag (deploy.yml) hashes this file, the smoke test below and
# app/build.rs with the sources: a fix here changes the tag.

# ---------------------------------------------------------------- tools
# The build tools, in their own layers so a source change does not reinstall
# them. 1.98 as the API's image (MSRV of the workspace is 1.88, checked in CI).
FROM rust:1.98-bookworm AS tools
RUN rustup target add wasm32-unknown-unknown
# cargo-leptos builds the server and the WASM, hashes the file names, and
# runs Tailwind. It downloads Tailwind's standalone binary and binaryen
# (wasm-opt) itself, at versions it pins, on first use.
ARG CARGO_LEPTOS=0.3.11
RUN cargo install cargo-leptos --version "$CARGO_LEPTOS" --locked
# wasm-bindgen-cli must match the `wasm-bindgen` crate of Cargo.lock to the
# patch version, or the glue fails at build time: it is read from the lock,
# never typed twice.
COPY learn-pali/Cargo.lock /lock/Cargo.lock
RUN WB=$(awk '/^name = "wasm-bindgen"$/ {getline; gsub(/[^0-9.]/, ""); print; exit}' /lock/Cargo.lock) \
    && test -n "$WB" \
    && echo "wasm-bindgen-cli $WB (from Cargo.lock)" \
    && cargo install wasm-bindgen-cli --version "$WB" --locked

# ---------------------------------------------------------------- builder
FROM tools AS builder
WORKDIR /src
# ui-lib: what the workspace reads (manifest, build script, locales, theme,
# sources; README.md because a source `include_str!`s it). No .git.
COPY ui-lib/Cargo.toml ui-lib/build.rs ui-lib/README.md ./ui-lib/
COPY ui-lib/build ./ui-lib/build
COPY ui-lib/locales ./ui-lib/locales
COPY ui-lib/src ./ui-lib/src
COPY ui-lib/style ./ui-lib/style
# learn-pali: the workspace needs the manifest of all five members to load
# (cli, tools); `app`, `core` and `passage` are compiled.
COPY learn-pali/Cargo.toml learn-pali/Cargo.lock ./learn-pali/
COPY learn-pali/app ./learn-pali/app
COPY learn-pali/core ./learn-pali/core
COPY learn-pali/passage ./learn-pali/passage
COPY learn-pali/cli ./learn-pali/cli
COPY learn-pali/tools ./learn-pali/tools
WORKDIR /src/learn-pali
# The build needs no credential only while the lock holds no git source.
RUN ! grep -q '^source = "git+' Cargo.lock \
    || { echo "Cargo.lock holds a git source: the build would need a GitHub credential (task 031)"; exit 1; }
RUN cargo leptos build --release --cargo-locked \
    && mkdir /out \
    && cp target/release/learn_pali target/release/hash.txt /out/ \
    && cp -r target/site /out/site

# ---------------------------------------------------------------- runtime
FROM debian:bookworm-slim
RUN useradd --system --uid 10001 --home /nonexistent --shell /usr/sbin/nologin app
WORKDIR /app
# The server reads hash.txt NEXT TO ITS OWN EXECUTABLE to name the hashed
# files (learn-pali.<hash>.js/.wasm); site/ is LEPTOS_SITE_ROOT.
COPY --from=builder --chown=app:app /out/learn_pali /out/hash.txt ./
COPY --from=builder --chown=app:app /out/site ./site
# `get_configuration(None)` reads these and nothing else: no Cargo.toml in
# the image. Cloud Run sends its traffic to 8080.
ENV LEPTOS_OUTPUT_NAME=learn-pali \
    LEPTOS_SITE_ROOT=site \
    LEPTOS_SITE_PKG_DIR=pkg \
    LEPTOS_SITE_ADDR=0.0.0.0:8080 \
    LEPTOS_HASH_FILES=true \
    LEPTOS_ENV=PROD \
    RUST_LOG=info \
    PORT=8080
USER app
# Smoke test: starts the server as the image will run it, with a throwaway
# origin secret made here (valid for this run only, never the real one), and
# asks it what the rollout will ask (deploy.yml): `/` is 200 and names
# PaliLearn, the hashed .wasm it references is served, `/review` is gone
# (404), and a request without the header is 403 with an EMPTY body (the
# guard's refusal; LEPTOS_ENV=PROD makes the service refuse to start without
# ORIGIN_SECRET). A broken build or a missing
# hash file fails here, not on Cloud Run. Plain bash (/dev/tcp): the image has
# no curl and gains none.
RUN <<'EOF'
#!/bin/bash
set -eu
secret=$(head -c 24 /dev/urandom | od -An -tx1 | tr -d ' \n')
req() { exec 3<>"/dev/tcp/127.0.0.1/8080"; printf 'GET %s HTTP/1.0\r\nHost: smoke\r\n%s' "$1" "$2" >&3; cat <&3; }
get() { req "$1" "x-origin-auth: $secret"$'\r\n\r\n'; }
anon() { req "$1" $'\r\n'; }
status() { (get "$1") | { read -r _ code _; echo "$code"; }; }
ORIGIN_SECRET="$secret" ./learn_pali &
pid=$!
trap 'kill $pid 2>/dev/null || true' EXIT
page=""
for _ in $(seq 1 50); do
  page=$(get / 2>/dev/null) && break
  sleep 0.2
done
test -n "$page" || { echo "smoke: no answer on 8080"; exit 1; }
test "$(printf '%s' "$page" | { read -r _ code _; echo "$code"; })" = 200 || { echo "smoke: / is not 200"; exit 1; }
printf '%s' "$page" | grep -q PaliLearn || { echo "smoke: / does not name PaliLearn"; exit 1; }
wasm=$(printf '%s' "$page" | grep -o '/pkg/[A-Za-z0-9_.-]*\.wasm' | head -1)
test -n "$wasm" || { echo "smoke: / references no .wasm"; exit 1; }
test "$(status "$wasm")" = 200 || { echo "smoke: $wasm is not served"; exit 1; }
test "$(status /review)" = 404 || { echo "smoke: /review is not 404"; exit 1; }
# The trailing x keeps the command substitution from eating the final newlines.
refusal=$(anon /; printf x)
refusal=${refusal%x}
test "$(printf '%s' "$refusal" | { read -r _ code _; echo "$code"; })" = 403 || { echo "smoke: a request without the header is not 403"; exit 1; }
test -z "${refusal#*$'\r\n\r\n'}" || { echo "smoke: the 403 without the header has a body (the guard's refusal is empty)"; exit 1; }
echo "smoke: ok ($wasm)"
EOF
EXPOSE 8080
CMD ["./learn_pali"]
