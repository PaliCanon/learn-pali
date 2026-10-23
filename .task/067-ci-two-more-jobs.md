# 067 CI green on main: two more jobs

status: merged · rank: 1 · level: simple · type: fix · request: CI on main after 066 (f6becba): two jobs still red · light path

## Summary
Rank: difficulty 1, impact 2 → 1. (1) `scripts/release-values.test.sh` pinned the deploy step's `--update-env-vars "$(scripts/release-values.sh)"` as the whole value; 062 appended the API URL inside the quotes: the regex allows a suffix. (2) The headless step `--test release_document` had no `CARGO_TARGET_WASM32_UNKNOWN_UNKNOWN_RUNNER` (`Exec format error`): the env block the other three steps carry.

## Log
| Step | Result | Output |
| --- | --- | --- |
| Build | done | release-values.test.sh 206 ok locally; ci.yml parses |
| Check | done | orchestrator; CI on main read after the push |
| Merge | done | 9507b72 on main (squash) |

## Retro
Light path by the orchestrator. The 066 sweep read four red jobs and missed the two that fail later in the same jobs' lists (a job stops at its first red step): read every job's every step. Retro 067 validated.

## Merge
9507b72
