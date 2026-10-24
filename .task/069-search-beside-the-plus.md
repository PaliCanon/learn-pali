# 069 The search icon beside the '+', Practice alone on its row

status: merged · rank: 1 · level: simple · type: feat · request: the user, 2026-10-24 (« tu peux me mettre le bouton practice à gauche et search à droite ? »; asked which of three layouts, chose « écartés aux bords »)
worktree: /Users/lsh/projects/palicanon/wt/learn-pali-069/learn-pali (branch `feat/cta-row-edges`, named for the first layout), paired `wt/learn-pali-069/ui-lib` at v0.4.3

## Summary
Rank: difficulty 1 (one class and one wrapper in the home's CTA row), impact 1 (the home's look). Light path, no agents (user, 2026-10-22): the orchestrator builds; one SSR test; the user's check on a capture.

The home's CTA row centred Practice and the search icon side by side. Now the CTA row holds the CTA alone, centred as before 062; the search icon sits beside the '+' under the term list (`data-add-with-search`, the '+' then the loupe, centred), on the full home only (a progress saved); when every term is started the loupe stands there alone. A home with no CTA renders no CTA row, and the empty home and the nothing-started home show no search at all (user, 2026-10-24: « seulement sur l'accueil quand on a tout »). Mobile: the same pair under the list; checked at 390 px.

Ruled out, in order: both at the edges (« je suis pas fan »), Practice centred with the loupe at the right edge of its row (« essaie le search près du add »).

## Log
| Step | Result | Output |
| --- | --- | --- |
| Define | ✓ | one question with three previews, the user chose the edges |
| Build | ✓ | the CTA alone on `CTA_ROW`, no row without a CTA; `NewTerms::plus_with_search` (the '+' and the loupe) on the full home; test `with_a_cta_the_row_spreads_to_its_edges…`, the 062 row test updated; 773 ssr tests, clippy ssr clean |
| Check (round 1) | ✗ | the user, on the empty state: no search icon there, only on the full home → the row renders only with a CTA |
| Check (round 2) | ✗ | the user, on the edges layout: « je suis pas fan ! Est-ce qu'on mettrait pas le practice au milieu » → the three-column grid |
| Check (round 3) | ✗ | the user, on the centred layout: « essaie le search près du add » → the loupe moved beside the '+' under the list |
| Check | ✓ | the user, 2026-10-24, on 127.0.0.1:3068 (branch 9203643, ui-lib v0.4.3): « c'est mieux merge deploy prod » |

## Merge
a8de775 on main (squash), 2026-10-24; bookkeeping 4ef5865 = prod tag v0.1.15, fragment-v0.5.1.

## Retro
No agents: one line. Three user rounds on one row: the first question offered three layouts but the chosen one was rejected on sight, and the place the user wanted (beside the '+') was not among them. Lesson: for a placement, show the candidates on the real page (captures), not as ASCII previews; and when the user names an element to move, ask what it should sit next to.
