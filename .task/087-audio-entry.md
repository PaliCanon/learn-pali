# 087 Audio entry

status: review, rank: 2 — difficulty 1 (one entry, one string, the same in every PaliCanon site), impact 2 (a link to the new audio site)
level: simple
request: the user, 2026-11-16: «oui, mais il faut aussi dans about je crois, et dans le site vitrine»
repository: learn-pali (light path: no Studio conversation)

## Summary
The rail gains Audio after Map: href https://audio.palicanon.app, IconName::Motion, label key nav_audio (en Audio, fr Audio, vi Âm thanh), added to the catalogue test's SAME_AS_ENGLISH list; a rail test, red first.

## Log
| Step | Result | Output |
|---|---|---|
| Build | green | 000851c feat(app): add the Audio entry to the rail |
| Verify | green | fmt, leptosfmt, clippy, tests, doc and release build exit 0 on the branch (see the hand-back of the sibling-sites lead, 2026-11-17); Cargo.lock unchanged; ui-lib stays at v0.4.11. |
| Open | — | Not run: the csr browser tests (chromedriver). |

## Retro
One line: the brief assumed an about page with a services list; none exists in the Rust sites (their footers link to palicanon.app/acknowledgements), so the brief was checked against the code before cutting.
