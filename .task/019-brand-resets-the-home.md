# 019 The brand brings the home back to its first view

status: merged · rank: 1 · level: simple · type: fix · path: light · request: learn-pali (user, 2026-10-08: « this one should redisplay the first items to the home », a capture with the Half-yearly cell pressed and 'No terms here yet')

## Summary
A plain click on 'PaliLearn' returns the home to its first view: the stage filter cleared (every term listed, no cell pressed), the term list scrolled to its top, and — as today — out of a session if one runs. Today the click only acts in a session; outside one it navigates to `/`, the page the app is already on, so the filter (page state) stays. `href="/"` stays for a middle or modified click. Rank 1.

## Log
| Step | Result | Output |
| --- | --- | --- |
| Define | done | the user's words and capture |
| Build | done | lane Sonnet: 6e0f2dd reset_home(stage) on every plain brand click (filter cleared, list scrolled to top, session left); 305 + 9 tests; clippy ssr + wasm, fmt, leptosfmt EXIT 0 |

## Merge
3d028e7 (squash, 2026-10-08).

## Retro
One line: a link to the page the app is already on does nothing once state lives in the page — the brand needed an explicit reset (a consequence of 003's no-URL rule).
