# 028 The repository's licence is CC-BY-NC-4.0

status: merged · rank: 1 · level: simple · type: chore · request: user, 2026-10-11 ("les repos PaliCanon devraient être sous la licence CC-BY-NC-4.0") · light path

## Summary
Rank: difficulty 1 (a LICENSE file and the workspace `license` field), impact 1 (no behaviour) → 1. Decision: docs fefd385. Every crate inherits the workspace field.

## Log
| Step | Result | Output |
| --- | --- | --- |
| Build | done | the LICENSE (official legal code) and `license = "CC-BY-NC-4.0"`; `cargo metadata` 0 |
| Check | done | orchestrator: no visible result; every crate's field read |
| Merge | done | squash on main |

## Retro
One line (light path): a repository-wide metadata change; nothing to carry.
