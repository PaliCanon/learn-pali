# 008 The term card holds its detail, the strip three by three, the + opens a dialog, the page centred

status: merged · rank: 3 · level: standard · type: feature · path: lane + orchestrator's capture · request: learn-pali (user, 2026-10-06) · after: [ui-lib 055]

## Summary
The user, 2026-10-06 (with a capture of an open row whose detail sits outside the card): « les informations d'une carte devraient être dans la carte, avec un peu plus de design. Les 6 times deck mets-les 3 par 3, un peu plus gros. Et centre toute la page un peu plus en middle. Je vois pas de bouton resume, comment apprendre les cartes là ? Enfin le + devrait ouvrir une pop-up pour ajouter des termes. »
Decided:
- **The card holds its detail.** A term row is one bordered card (the Tile surface, `TILE_SURFACE` from ui-lib) holding the toggle (the PaliTerm, a full-width transparent button) and, when open, the detail under a hairline divider inside the same card: the four facets as small pills (Meaning · Recall · Forms · Context, the ones the term has; passed ones gold with a check, the others muted), then 'Stage' and 'Next review' and 'Lapses' as label/value pairs in two columns; the open card keeps its gold border.
- **The strip** in three columns at every width (ui-lib 055 `Columns::Three`), at the default size (Md), not compact.
- **The + opens a dialog** (ui-lib `Dialog`, title 'New terms'): the picker +5 · +10 · Custom inside, 'Add' in the footer (Primary) with 'Cancel' (Ghost); Add closes the dialog and starts the session in place. No inline panel any more.
- **The page centred**: on `sm` and up the column sits in the middle of the viewport height when it is shorter (`min-h` of the page area, `justify-center`); a long page scrolls as before; a phone stays top-aligned.
- **The + on the CTA row** (the user: « passe le bouton + au niveau du resume »), at its right.
- **A compact Resume** (the user chose it among three layouts: a full-width card, a compact button, a cell in the strip): `[ ▶ Resume · 15 ]` at the '+' height, Primary, Sm, the count of cards due; its accessible name 'Resume, 15 cards'; 'Review the next stage' the same form.
- **Nothing due**: the CTA row shows 'Next review: <date>' (the earliest due date of the started terms), muted, where Resume would be, so the learner knows when cards come back; studying ahead is asked to the user.
Rank 3: difficulty 3 (a card with a nested toggle, a dialog, vertical centring), impact 3.

## Log
| Step | Result | Output |
| --- | --- | --- |
| Define | done | the user's words and capture; the Resume design chosen by the user among three |
| Capture | done | orchestrator, :3201 on the branch (dark, desktop 1280, phone 390): compact Resume + '+' on one row, strip 3x3 Md, the open card holds pills + Stage / Next review / Lapses inside its gold border, the + dialog (+5 +10 Custom, Cancel, Add) adds and starts the session, stop and back home, no console error, scrollWidth 390 |
| Build | done | lane Opus: 5553c2a the card holds its detail, 163f86d the + on the CTA row opens a dialog + compact Resume, 495f4f1 next review date, 7100512 strip 3x3 + page centred (after ui-lib 055), 1800bdd drop detail_facets; 284 + 9 tests; checks EXIT 0. Accepted departures: local card classes (TILE_SURFACE is crate-private), Nav Lg as the toggle, interpolate_display in build.rs, sm:flex-1 sm:justify-center |

## Merge
6ca0a19 (squash, 2026-10-06), after ui-lib 055.

## Retro
Two user messages folded in mid-lane (the + on the CTA row, the compact Resume chosen among three previews); a sibling ui-lib field held the last commit until its merge (brief line: a field from a sibling task lands last). The orchestrator's claim that leptos_i18n had no interpolate_display was wrong: it is a codegen option (corrected in 003's process).
