# 033 cargo deny passes again: unsafe-libyaml unmaintained

status: merged · rank: 1 · level: simple · type: fix · request: found by the orchestrator during 031's verify, 2026-10-13 · light path

## Summary
Rank: difficulty 1 (one entry in deny.toml), impact 2 (cargo deny, a CI job, fails on main) → 1.
RUSTSEC-2024-0449 (published as unmaintained: the repository dtolnay/unsafe-libyaml is archived; no safe upgrade) now fails `cargo deny check advisories` on main. unsafe-libyaml 0.2.11 is reached only through serde_yaml 0.9 ← leptos_i18n_parser 0.6.2, at build time (the locale files are parsed by the i18n macros), from learn_pali and from palicanon-ui-lib. Fix: ignore it in deny.toml with its reason and a drop condition, as the two leptos ones already ignored; nothing else.

## Log
| Step | Result | Output |
| --- | --- | --- |
| Build | done | d8e9a40; red: RUSTSEC-2024-0449; green: advisories ok, bans ok, licenses ok, sources ok |
| Check | done | orchestrator: one deny.toml entry, its reason and drop condition read; cargo deny check ok per the lane. No visible result |
| Merge | done | e71597f on main (not pushed) |

## Retro
One line (light path): an advisory published between two runs turns a green cargo deny red with no change of ours; the CI job catches it on the next push, nothing to change.
