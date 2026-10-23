# 066 CI green on main again

status: merged · rank: 2 · level: simple · type: fix · request: found 2026-10-23 while bumping the desktop app to fragment-v0.4.0 (CI on main red at f87390b: four jobs) · light path

## Summary
Rank: difficulty 2 (test fixtures, a licence line, a cfg, a crate bump), impact 3 (CI, the app's build) → 2.
Four failures on main, each from a merged task whose local matrix did not run that job: (1) `rollout-check.test.sh` pinned the deploy's `--update-env-vars` value as ending at the sha; 062 appended the API URL: the regexes accept a following comma. (2) cargo-deny: `webpki-roots` (under rustls, under ureq, 062) is CDLA-Permissive-2.0, not in the allow list, and 1.0.9 carries RUSTSEC-2026-0381: the licence allowed with its reason, the crate bumped. (3) The csr headless test asserted no deck chooser; 063b ships two decks: the test now asserts the chooser with a pressed chip. (4) `SyncCycle::set_status/set_synced/set_mounted` were `cfg(all(test, feature = "ssr"))` while csr test builds use them: `cfg(test)`. Plus (5), found by the app: a native build of the fragment without `ssr` kept the server functions' bodies, which name the server: the bodies are gated, and CI checks the native csr build. Lesson: the local matrix must be CI's matrix (`cargo deny check` whole, the script tests, clippy csr `--all-targets`, the headless csr tests).

## Log
| Step | Result | Output |
| --- | --- | --- |
| Build | done | 5092e6a 3f4a26a 0e21dd3 ; rollout-check.test.sh 1259 ok; cargo deny check 0; native csr check 0; clippy csr --all-targets wasm 0, hydrate 0, ssr 0; 770 ssr tests; the headless csr tests 18 ok (wasm-bindgen-test-runner, as CI) |
| Check | done | orchestrator: CI's own commands, locally; CI on main read after the push |
| Merge | done | 1a6be2f on main (squash); CI on main read after the push |

## Retro
Light path by the orchestrator alone. Four tasks had merged on green local matrices while CI on main was red: the local matrix must be CI's own commands (now in quality.md and a lesson), and main's CI is read after every push. Retro 066 validated.

## Merge
1a6be2f
