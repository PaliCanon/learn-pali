# 016 Gold along the frames

status: merged · rank: 1 · level: simple · type: style · path: light · request: learn-pali (user, 2026-10-07: « ajoute un peu de couleur le long des cadres », chose « bordure dorée douce ») · after: [ui-lib 058]

## Summary
Every frame of the home in a soft gold instead of grey: the stage tiles (ui-lib 058, the Tile variant), the term cards (`CARD_CLOSED` in term_list.rs: `border-line` → `border-gold/25`, the hover `border-gold/60`, the open card `border-gold` unchanged), the facet pills not passed (`border-line` → `border-gold/25`); the detail's inner divider stays a hairline in `border-line`. The review's card (RevealCard) and the dialogs keep their look. Rank 1.

## Log
| Step | Result | Output |
| --- | --- | --- |
| Define | done | the user's choice among three previews |
| Build | done | lane Sonnet: 3ecb2d4 CARD_CLOSED and the not-passed pill in border-gold/25; 304 + 9 tests; clippy ssr + wasm, fmt, leptosfmt EXIT 0; after ui-lib 058 |

## Merge
43b308c (squash, 2026-10-07), after ui-lib 058.

## Retro
One line: chosen among three previews; clean.
