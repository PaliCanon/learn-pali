# 023 The cards come in a random order

status: merged · rank: 3 · level: simple · type: feature · path: lane + orchestrator's check · request: learn-pali (user, 2026-10-11: « les cartes devraient être random dans le deck quand on practice »)

## Summary
A session's cards come in a random order — both kinds of Practice: the due review and practice ahead — instead of the deck's order (term by term, facet by facet). Decided:
- **Where**: learn_core shuffles the queue it builds (`Session::start`, `Session::practice`) with a **seed passed in** (`u64`), so core stays deterministic and tested (nothing in core reads a clock or a random source, as `today` is an input); a small dependency-free generator in core (xorshift64* or SplitMix64), Fisher–Yates over the queue.
- **The seed**: the app draws it in the browser when a session starts (`js_sys::Math::random()` → u64), a fixed seed in tests; the CLI takes `--seed` (default: from the time).
- **What stays ordered**: the practice cards a failure adds ("to see again") still come at the end of the session, after the shuffled queue; the summary and the counts are unchanged.
- **Two facets of one term may come back to back**: accepted (a true shuffle); spreading them apart is not asked.
Rank 3: a core behaviour changed (the order), its rule pair with the failure re-ask, two consumers (app, CLI).

## Log
| Step | Result | Output |
| --- | --- | --- |
| Define | done | the user's words |
| Build | done | lane Sonnet: 910bb4c core shuffle (SplitMix64 + Fisher–Yates, seed in; re-ask still last; outcome independent of the seed), f0e9bb0 app seed from Math.random, 9d24c4b CLI --seed; cargo test/clippy --workspace, hydrate wasm, core wasm, fmt, rustdoc, deny EXIT 0 |

## Merge
5e3707d (squash, 2026-10-11).

## Retro
One line: randomness kept out of core by a seed input, like the date; tests that read an order needed a known seed.
