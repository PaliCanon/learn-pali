# 019 The brand brings the home back to its first view

status: building · rank: 1 · level: simple · type: fix · path: light · request: learn-pali (user, 2026-10-08: « this one should redisplay the first items to the home », a capture with the Half-yearly cell pressed and 'No terms here yet')

## Summary
A plain click on 'PaliLearn' returns the home to its first view: the stage filter cleared (every term listed, no cell pressed), the term list scrolled to its top, and — as today — out of a session if one runs. Today the click only acts in a session; outside one it navigates to `/`, the page the app is already on, so the filter (page state) stays. `href="/"` stays for a middle or modified click. Rank 1.

## Log
| Step | Result | Output |
| --- | --- | --- |
| Define | done | the user's words and capture |

## Retro
