# 008 The term card holds its detail, the strip three by three, the + opens a dialog, the page centred

status: building · rank: 3 · level: standard · type: feature · path: lane + orchestrator's capture · request: learn-pali (user, 2026-10-06) · after: [ui-lib 055]

## Summary
The user, 2026-10-06 (with a capture of an open row whose detail sits outside the card): « les informations d'une carte devraient être dans la carte, avec un peu plus de design. Les 6 times deck mets-les 3 par 3, un peu plus gros. Et centre toute la page un peu plus en middle. Je vois pas de bouton resume, comment apprendre les cartes là ? Enfin le + devrait ouvrir une pop-up pour ajouter des termes. »
Decided:
- **The card holds its detail.** A term row is one bordered card (the Tile surface, `TILE_SURFACE` from ui-lib) holding the toggle (the PaliTerm, a full-width transparent button) and, when open, the detail under a hairline divider inside the same card: the four facets as small pills (Meaning · Recall · Forms · Context, the ones the term has; passed ones gold with a check, the others muted), then 'Stage' and 'Next review' and 'Lapses' as label/value pairs in two columns; the open card keeps its gold border.
- **The strip** in three columns at every width (ui-lib 055 `Columns::Three`), at the default size (Md), not compact.
- **The + opens a dialog** (ui-lib `Dialog`, title 'New terms'): the picker +5 · +10 · Custom inside, 'Add' in the footer (Primary) with 'Cancel' (Ghost); Add closes the dialog and starts the session in place. No inline panel any more.
- **The page centred**: on `sm` and up the column sits in the middle of the viewport height when it is shorter (`min-h` of the page area, `justify-center`); a long page scrolls as before; a phone stays top-aligned.
- **Nothing due**: the CTA row shows 'Next review: <date>' (the earliest due date of the started terms), muted, where Resume would be, so the learner knows when cards come back; studying ahead is asked to the user.
Rank 3: difficulty 3 (a card with a nested toggle, a dialog, vertical centring), impact 3.

## Log
| Step | Result | Output |
| --- | --- | --- |
| Define | done | the user's words and capture |

## Retro
