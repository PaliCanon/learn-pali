# 084 Bump rusqlite for RUSTSEC-2026-0459

status: merged
rank: 1
level: simple
request: the orchestrator, 2026-11-13: CI cargo-deny red on main

## Summary

RUSTSEC-2026-0459 (bundled SQLite < 3.54.0 in libsqlite3-sys 0.38.2) turns the cargo-deny job red on main; the fix is libsqlite3-sys >= 0.39.0, which rusqlite 0.41 pulls in.
`passage` and `tools/build-deck` move from rusqlite 0.40 to 0.41 (lock: rusqlite 0.41.1, libsqlite3-sys 0.39.1); no source change was needed.

## Log

| Check | Result |
| --- | --- |
| `cargo update -p rusqlite --offline` | exit 0; rusqlite 0.40.2 -> 0.41.1, libsqlite3-sys 0.38.2 -> 0.39.1; the palicanon-ui-lib block restored to origin/main |
| `cargo fmt --all --check` | exit 0 |
| `cargo clippy --offline --all-targets --features ssr -p learn_pali -- -D warnings` | exit 0 |
| `cargo test --offline -p learn_passage -p build_deck` | exit 0; build_deck 91 passed, learn_passage 33 passed, 0 failed |
| `cargo deny check advisories` | exit 0; "advisories ok" |
| CI `msrv (1.88)` job | not run: no 1.88 toolchain installed. rusqlite 0.41.1 declares `rust-version = "1.89.0"` and the job runs `cargo check --locked -p build_deck`: expected red until the MSRV or the rusqlite pin is decided |

## Retro

The brief named only `passage/Cargo.toml`, but `tools/build-deck/Cargo.toml` pins rusqlite 0.40 too, so bumping one crate would have kept libsqlite3-sys 0.38 in the lock; and the 0.41 MSRV (1.89) against the workspace's 1.88 was not checked at Define.
| MSRV | rusqlite 0.41.1 declares rust-version 1.89: workspace rust-version and the CI msrv job raised from 1.88 to 1.89 (orchestrator) | ci.yml, Cargo.toml, README |
