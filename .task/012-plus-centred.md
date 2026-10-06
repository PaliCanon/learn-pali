# 012 The + at the bottom centre of the list

status: merged · rank: 1 · level: simple · type: feature · path: light · request: learn-pali (user, 2026-10-06: « middle plutôt », after the orchestrator noted the + covering the last card's lapses at the bottom right)

## Summary
The '+' inside the term list's scroll region moves from the bottom right (`justify-end`) to the bottom centre (`justify-center`), still sticky, so it no longer covers the right-hand date and lapses of the card under it. Its other places (no list: where the list would be) centre too. Rank 1.

## Log
| Step | Result | Output |
| --- | --- | --- |
| Define | done | the user's words |
| Build | done | lane Haiku: c762524 justify-end → justify-center for the + (list and no-list places); 295 tests; clippy, fmt, leptosfmt EXIT 0. The lane put its worktree inside the repo (learn-pali/wt/…, with a ui-lib symlink) instead of palicanon/wt — removed after the merge |

## Merge
2d0e954 (squash, 2026-10-06).

## Retro
One line: the Haiku lane created its worktree inside the repository with ui-lib symlinks; the brief named the path, the lane altered it — briefs keep the absolute worktree path and say 'exactly'.
