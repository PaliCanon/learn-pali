# 065 Tappable words everywhere a Pāli line shows

status: merged · rank: 4 · level: standard · type: feat · request: the user, 2026-10-23 («pourquoi je ne peux pas avoir la définition des mots du sutta ?» on an added word's line; «mot tapable partout») · after: [062]

## Summary
Rank: difficulty 4 (a server function over the API's exact-form lookup, the word panel fed live, two screens), impact 4 (every passage not compiled in the deck) → 4. No agents (the user's order): the orchestrator builds.
Today a Pāli line is tappable only where the deck compiled the entries of its words (`Deck.words`, built from the DPD inflections by build-deck): the review's context facet of a deck term. A line fetched live (a word added by search, 062) and the word panel's example have no entries, so nothing opens. Change: a tap on a word of ANY Pāli line asks the server (`lookup_word(form)`: the API's `GET /v1/dpd/{form}`, exact surface form, the site's token as in 062) and opens the same word panel with the senses found (the deck's compiled entries stay the first source: no call when the line has them); no entry → the panel says so; a failed call → a quiet line with Retry. Where: the review card's context line of an added word, the word panel's EXAMPLE line, the 'Add a word' page's rows' example when shown. Phone: the same panel (the sheet of 054). Open: the panel's content shape for live senses (the API's DpdEntry vs the deck's WordEntry: map to WordEntry {headword, tag, meaning}); the cache (none; the API caches at the edge).

## Log
| Step | Result | Output |
| --- | --- | --- |
| Define | done | the user's request; session learn-pali-065-tappable-words (design v1 as built) |
| Build | done | ui-lib 076 (v0.4.3: PaliText lookup, empty_label); 0bf25c7 lookup_word server function + LiveWords on the session's lines; strings en/fr/vi; by the orchestrator |
| Verify | done | test_app 770, test_rest, clippy ssr/hydrate/csr/rest, fmt, doc, deny, scripts: all 0 (scratch/065/check.sh) |
| Fix 065 | done | the per-call timeout 5 s (a cold api-dev timed out the first lookup at 2 s; the dictionary tests unchanged) |
| Check 065 | doing | on learn.dev (feat/tappable-words · 5cb2016, run 39903309392): lookup_word answers senses once the API is warm; the idle probe runs; waiting for the user |
| Check 065 | done | the user, 2026-10-23: «tag alors» on 5cb2016; the idle probe: senses in 9.4 s on a cold API |
| Merge | done | 43231eb on main (squash); prod tag v0.1.14 |

## Retro
By the orchestrator alone (no agents), one morning: the ui-lib addition first (two optional props, the golden markup kept by rendering the empty-panel row as a list item, not an Option), then the server function and the lines. Errors: the ui-lib task number and tag collided with another session's work the same morning (075 / v0.4.2: renumbered 076, released v0.4.3; a lesson in memory); the first lookup after idle timed out at 2 s on the cold API (found by the dev probe, 5 s now). Open: an added word's live senses have no Add button in the panel (the panel's action knows the deck's entries only); the search screen's rows show no line. Retro 065 validated.

## Merge
43231eb
