# 010 Housekeeping: cache-proof assets, a plural, the workspace tests, a dead flag

status: merged · rank: 2 · level: simple · type: fix · path: light · request: found during 008–009 (2026-10-06)

## Summary
Four small things found while building, none started:
- **Cache-proof assets**: the server sends `pkg/learn-pali.wasm` and the CSS under fixed names without cache headers, so a browser kept the old build after a merge (the user saw neither the 3x3 strip nor the + beside Resume until a hard reload). Turn on cargo-leptos `hash-files = true` (hashed file names, the HTML pointing at them) and check the server serves them.
- **A plural**: 'Practice, 1 cards due' — `cta_practice_label` needs `_1` / `_n` forms (leptos_i18n plurals with the `_1/_n` naming 003 used).
- **The workspace tests**: `cargo test --workspace` and `cargo clippy --workspace` fail on main without `--features ssr` (test code in `app/src/app.rs:147,156` uses axum and tokio); gate that test module on `ssr` so the workspace commands pass.
- **A dead flag**: `home_state.rs` still computes `advance_possible` (and calls into `Session::advance`) though the home no longer offers 'Review the next stage' (009); remove it with its tests, or keep `advance` in core if the CLI uses it.
Rank 2: four one-file changes.

## Log
| Step | Result | Output |
| --- | --- | --- |
| Define | done | collected from 008 and 009's hand-backs |
| Build | done | lane Opus: 379d62c hashed assets (HashedStylesheet; /pkg/learn-pali.<hash>.{js,css,wasm} served 200, unhashed 404), 334661f '1 card due', c31ba23 every SSR-rendering test module gated on ssr (cargo test/clippy --workspace exit 0), 435137c advance_possible dropped (Session::advance kept: the CLI uses it); 398 workspace + 289 ssr tests; clippy, rustdoc, fmt, deny EXIT 0; leptosfmt red on site_footer.rs only (main, PR #1) |

## Merge
8c5f268 (squash, 2026-10-06).

## Retro
One line: the brief guessed one gated module, the truth was seven (a runtime failure, not a compile one); 'Practice ahead, 1 terms' and README l. 36 carried to 011.
