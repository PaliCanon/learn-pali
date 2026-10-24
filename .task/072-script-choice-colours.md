# 072 The script deck's cards answered by choice on both facets, the choice in colour

status: review · rank: 3 · level: standard · type: feat · request: the user, 2026-10-24 (« le résultat plutôt en vert quand c'est bon, sinon en rouge et le bon en vert » ; « du pali vers l'alphabet pourquoi on n'a pas 4 choix aussi ? »)
worktree: /Users/lsh/projects/palicanon/wt/learn-pali-072/learn-pali (branch `feat/script-choice-colours`), paired `wt/learn-pali-072/ui-lib` at v0.4.5 (078: the success token)

## Summary
Rank: difficulty 3 (a state enum with its classes, the read facet joining the choice card, a ui-lib token), impact 3 (every card of the Devanāgarī deck). Light path, no agents; one review round with @deep; the user checks on the phone in prod.

Two changes on 071's choice card. (1) Colours: a choice is a plain `<button>` whose whole look comes from one of four states (`ChoiceState`, `choice_classes`): a chip while open; once answered, the answer green (`success`, a new ui-lib token: 078, v0.4.5), a wrong tap red (`danger`), the rest dimmed; `aria-pressed` and `data-right` as before. (2) The read facet (Meaning: the sign shown) is a choice card too: four romanisations to tap, the same distractors; the verdict names the romanisation. `ReviewRun::choices` answers for both facets of a script deck.

Ruled out: colouring through ui-lib's Button variants (the choice's states are the quiz's; one small component per behaviour, its states defined once).

## Log
| Step | Result | Output |
| --- | --- | --- |
| Build | ✓ | ui-lib 078 merged and tagged v0.4.5; run.rs both facets; session.rs ChoiceState / choice_classes / choice_state, the read card's romanisations; tests |

## Retro

| Matrix | ✓ | fmt, leptosfmt, clippy ssr/csr/hydrate, test ssr (locked on ui-lib v0.4.5) |
| Check (orchestrator) | ✓ | headless phone 390×844: a read card (अ: i e a ā), a wrong tap red, the answer green, the rest dimmed, Next focused; a write card (i: उ ओ ए इ) the same; captures `scratchpad/072/quiz-read-answered.png`, `quiz-write-answered.png`; no exception |

## Reviews
### Round 1 — 2026-10-24 (@deep, lasting session entry `review-072-script-choice-colours`; the only round at rank 3)
1. "meaning.en is no romanisation" — closed with the deck's definition (063: a script item's meaning is its romanisation); one helper `choice_text(term, facet)` for the buttons and the verdict (also 7).
2. No perceivable feedback once tapped — TAKEN in part: `aria-disabled` keeps the focus, the tap is spent in the handler; the verdict is 071's live region; `aria-pressed` kept.
3. AA on composited pairs — closed with reason: 10 percent tints over the card, the text tokens keep their AA; `Other` is dimmed on purpose.
4. `tapped` a single slot — closed with the code: a scored card is never shown again (the key carries the counter).
5. The Chip look forked — TAKEN as a note: the open chip copies ui-lib's Chip classes; to fold into a ui-lib variant when the choice component moves there (concepts).
6. A paused card looks tappable — closed with the code: the card box is `inert` while paused.
Also, from the capture: the read card's wrong caption now says "It reads" (`quiz_wrong_read`, three locales).
