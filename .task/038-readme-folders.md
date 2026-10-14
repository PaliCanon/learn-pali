# 038 the README describes every folder

status: merged · rank: 1 · level: simple · type: docs · request: user, 2026-10-14 ("dans le readme ajoute une description des folders") · light path

## Summary
Rank: difficulty 1 (one README section), impact 1 (docs) → 1.
README.md's "What is here" table lists the four crates and core/fixtures only. It becomes a description of every folder the repository holds, one line each: the crates and their source folders (app/src/components, learn, pages; app/locales, style, public, tests; core/src, fixtures, tests; cli/src, tests; tools/build-deck), infra/ (worker/, scripts/), scripts/, .github/ (workflows/, actions/ui-lib), .task/. Every line says what the folder holds, read from the folder itself; nothing invented.

## Log
| Step | Result | Output |
| --- | --- | --- |
| Build | done | 2e0586f; 28 folders described |
| Check | done | orchestrator: every line read against its folder; three corrected (catalogue.rs is the UI strings, seed.rs the session order seed, learn/window.rs the Context window) |
| Merge | done | 7e68bd5 on main (not pushed) |

## Retro
One line (light path): a lane described three modules from their names, not their docs; the orchestrator read each line against its folder before the merge.
