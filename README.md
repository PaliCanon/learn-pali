# learn-pali

Learning Pāli — a PaliCanon service. Its scope (what a learner can do,
screen by screen; what it serves) is defined in its first task under
`.task/` before anything is built (`../docs/conventions/quality.md`,
"Define before building"); until then the crate is a placeholder carrying
the repository's rules and CI.

**Holds**: the service's own code, content, routes and screens; its
infrastructure, CI and deploys.
**Never holds**: shared UI (that is `../ui-lib`), search indexes or
stores (`../engine`), the API gateway (`../api`), secrets.

## Checks

```sh
# Isolated toolchain, as every PaliCanon repository:
export RUSTUP_HOME=$HOME/.rustup-pc/rustup CARGO_HOME=$HOME/.rustup-pc/cargo PATH=$HOME/.rustup-pc/cargo/bin:$PATH RUSTC_WRAPPER=sccache
cargo fmt --check
cargo clippy --all-targets --all-features -- -D warnings
cargo test --all-features
RUSTDOCFLAGS="-D warnings" cargo doc --no-deps --all-features
```

The same run in CI (`.github/workflows/ci.yml`), plus MSRV (`--locked`),
cargo-deny and coverage (cargo-llvm-cov → Codecov). Functions stay under
50 lines (`clippy.toml`).
