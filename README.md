# learn-pali

Learning Pāli — a PaliCanon service, Leptos (SSR + hydrate) on
[`PaliCanon/ui-lib`](https://github.com/PaliCanon/ui-lib) (never `ui-lab`).
Its learning scope (what a learner can do, screen by screen; what it
serves) is defined in a later task under `.task/` before anything is built
(`../docs/conventions/quality.md`, "Define before building"). Today the
frame only: the shell (rail and top bar, theme, language), the rail of
services (Search, Learn), a placeholder home and the page footer.

**Holds**: the service's own code, content, routes and screens; its
infrastructure, CI and deploys.
**Never holds**: shared UI (that is `../ui-lib`), search indexes or
stores (`../engine`), the API gateway (`../api`), secrets.

## Run it

```sh
# Toolchain with the wasm32 target, isolated from the system Rust:
export RUSTUP_HOME=$HOME/.rustup-pc/rustup CARGO_HOME=$HOME/.rustup-pc/cargo PATH=$HOME/.rustup-pc/cargo/bin:$PATH RUSTC_WRAPPER=sccache
# Site on http://127.0.0.1:3201, live reload on 4201 (set in Cargo.toml):
cargo leptos watch
```

It depends on `palicanon-ui-lib` by git (`branch = "main"`), with a local
`[patch]` to `../ui-lib` (a worktree reaches it through the `wt/ui-lib`
symlink) — a change spanning both repositories is seen at once. `ui-lib`'s
`style/theme.css` and `build/placeholders.rs` are read from that sibling
checkout too.

## Checks

```sh
export RUSTUP_HOME=$HOME/.rustup-pc/rustup CARGO_HOME=$HOME/.rustup-pc/cargo PATH=$HOME/.rustup-pc/cargo/bin:$PATH RUSTC_WRAPPER=sccache
cargo fmt --all --check
cargo clippy -p learn_pali --features ssr --all-targets -- -D warnings
cargo clippy -p learn_pali --lib --features hydrate --target wasm32-unknown-unknown -- -D warnings
cargo test -p learn_pali --features ssr
RUSTDOCFLAGS="-D warnings" cargo doc --no-deps -p learn_pali --features ssr
cargo leptos build
cargo deny check advisories bans sources licenses
```

The same run in CI (`.github/workflows/ci.yml`), plus MSRV (`--locked`),
cargo-deny and coverage (cargo-llvm-cov → Codecov). Functions stay under
50 lines (`clippy.toml`).
