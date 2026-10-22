# 064 The ahead count in a popup on tap Practice, when nothing is due

status: review · rank: 2 · level: simple · type: feat · request: 062 (the user, 2026-10-22: «à la place du slide … le practice et un icône search»; the count in a popup on tap Practice; the popup only when nothing is due) · light path

## Summary
Rank: difficulty 2 (one popup, the row's layout), impact 3 (the home's main row) → 2.
Today, when nothing is due, the home's CTA row shows 'Practice' and a slider for the count of cards ahead (remembered in the store). Change: the slider leaves the row; tapping Practice when nothing is due opens a Popup (ui-lib DialogSize::Popup) holding the same slider (same bounds, same remembered value) and a 'Start' button that runs the session as today; Escape or ✕ closes without starting. With cards due, Practice starts the review at once, as today. The row keeps a place at Practice's right for the search icon of 066 (nothing rendered yet). en/fr/vi strings for the popup's title ('How many cards ahead?' or the existing wording) and Start; no em dash. Phone: the popup is the Popup size.

## Log
| Step | Result | Output |
| --- | --- | --- |
| Build | done | 81fab8f; red: 3 of 4 new SSR tests failed before the code (the popup absent, the slider in the row; the due-cards test passes by design); mutations: Popup to Sheet, slider back in the row, Practice press not opening: each 1 test red; checks: cargo test 536 passed, clippy ssr/hydrate/csr, fmt, leptosfmt, cargo leptos build all exit 0; captures: 1280x900 and 390x844 (mobile emulation), scrollWidth 1280/1280 and 390/390, Escape closes the popup, PNGs in the 064 scratch folder |
| Check | done | on learn.dev (feat/practice-popup · 05ae9be, deploy 39777119207); captures desktop 1280 and phone 390 (scrollWidth == innerWidth) in scratch/064 |
| Check | done | the user, 2026-10-22: «ça a l'air bien» |
