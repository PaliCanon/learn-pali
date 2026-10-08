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
#   docker build -f learn-pali/app/Dockerfile --ssh default .
#
# `--ssh default` forwards an ssh agent holding the read-only deploy key on
# PaliCanon/ui-lib: ui-lib is a private repository and also a git
# dependency, so Cargo fetches it even though the patch replaces it. The key
# is never written to a layer (`RUN --mount=type=ssh`).
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
ENV CARGO_NET_GIT_FETCH_WITH_CLI=true
# GitHub's ed25519 host key (https://api.github.com/meta), as ci.yml pins it.
RUN mkdir -p /root/.ssh \
    && echo "github.com ssh-ed25519 AAAAC3NzaC1lZDI1NTE5AAAAIOMqqnkVzrm0SdG6UOoqKLsabgH5C9okWi0dh2l9GKJl" > /root/.ssh/known_hosts
WORKDIR /src
# ui-lib: what the workspace reads (manifest, build script, locales, theme,
# sources; README.md because a source `include_str!`s it). No .git.
COPY ui-lib/Cargo.toml ui-lib/build.rs ui-lib/README.md ./ui-lib/
COPY ui-lib/build ./ui-lib/build
COPY ui-lib/locales ./ui-lib/locales
COPY ui-lib/src ./ui-lib/src
COPY ui-lib/style ./ui-lib/style
# learn-pali: the workspace needs the manifest of all four members to load
# (cli, tools); only `app` and `core` are compiled.
COPY learn-pali/Cargo.toml learn-pali/Cargo.lock ./learn-pali/
COPY learn-pali/app ./learn-pali/app
COPY learn-pali/core ./learn-pali/core
COPY learn-pali/cli ./learn-pali/cli
COPY learn-pali/tools ./learn-pali/tools
# The footer badge shows the branch and short commit the build came from.
# app/build.rs reads them from these two variables when both are set: the
# image has no .git (a checkout's .git/config can hold a token, and the build
# cache exports layers), so CI passes them as build args. A build without
# them shows "unknown".
ARG LEARN_PALI_GIT_BRANCH=unknown
ARG LEARN_PALI_GIT_COMMIT=unknown
WORKDIR /src/learn-pali
RUN --mount=type=ssh \
    cargo leptos build --release --cargo-locked \
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
# Smoke test: starts the server as the image will run it and asks it what
# the rollout will ask (deploy.yml): `/` is 200 and names PaliLearn, the
# hashed .wasm it references is served, `/review` is gone (404). A broken
# build or a missing hash file fails here, not on Cloud Run. Plain bash
# (/dev/tcp): the image has no curl and gains none.
RUN <<'EOF'
#!/bin/bash
set -eu
get() { exec 3<>"/dev/tcp/127.0.0.1/8080"; printf 'GET %s HTTP/1.0\r\nHost: smoke\r\n\r\n' "$1" >&3; cat <&3; }
status() { (get "$1") | { read -r _ code _; echo "$code"; }; }
./learn_pali &
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
echo "smoke: ok ($wasm)"
EOF
EXPOSE 8080
CMD ["./learn_pali"]
