# 052 CI on main red after 050: clippy (csr) and the web_sync browser test

status: fixed (to merge) · rank: 2 · level: simple · type: fix · request: found by the orchestrator 2026-10-17, CI on main red after 050 · light path (no viewer session, no reviewer)

## Summary
Difficulty 2, impact 2: two CI jobs fail on main since 562b2f7 (run 38994247890, latest 19e9be4); the rank is set from the two failures below, one narrow file each.

1. `clippy (csr)`: `cargo clippy --locked -p learn_pali --features csr --all-targets --target wasm32-unknown-unknown -- -D warnings` fails with `dead_code` on `SyncCycle::set_status` and `set_mounted` (app/src/sync.rs): they are `#[cfg(test)]` but only the `ssr` render tests use them.
2. `test (csr, headless Chrome)`: `app/tests/web_sync.rs:335` finds `window.__drive.revoked` empty where `["fake-token"]` is expected, on GitHub's runner only.

## Log
| Step | Result | Output |
| --- | --- | --- |
| clippy (csr) red | exit 101 on the unchanged tree: `dead_code` on `SyncCycle::set_status` and `set_mounted` (`#[cfg(test)]` but used only by `cfg(all(test, feature = "ssr"))` render tests) | scratchpad clippy-csr.log |
| clippy (csr) fix | the two setters gated `#[cfg(all(test, feature = "ssr"))]`; exit 0 | commit "fix(sync): gate the render-test setters" |
| web_sync red | `left: Ok([])  right: Ok([fake-token])` at web_sync.rs:335 reproduced locally by waiting 4 s before the sign-out (40 ticks); the new assertion "the real script, arriving late, cannot replace the fake" is red on the old fake (`Some(false)` vs `Some(true)`) | scratchpad probe.log, red2.log |
| web_sync fix | FAKE_GIS frozen, `window.google` read-only; green with the 4 s wait and without it; 1 passed | commit "test(sync): the fake Google cannot be replaced" |
| CI commands | fmt, clippy ssr/hydrate/csr/native/core-wasm, test ssr (445 + learn_core/cli/deck), test csr, test web_sync, csr-without-web-store.sh, rustdoc: every exit 0 | scratchpad all.out |
| Check | done | orchestrator, 2026-10-17: diff read (two cfg attributes; the fake Google frozen, window.google read-only); every CI command exit 0 per the lane; the real-script cause inferred, reproduced by a 4 s wait |
| Merge | done | b67eeed on main, pushed |

## Retro
One line (light path): a browser test that injects a real third-party script must freeze its fake (or block the host): it passed locally only by finishing before the script arrived.
