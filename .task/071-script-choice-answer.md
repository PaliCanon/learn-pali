# 071 The script deck's write facet answered by choice

status: merged · rank: 4 · level: standard · type: feat · request: the user, 2026-10-24 (« réponse à 4 choix me paraît bien ! »), from `followup-script-choice-answer` (063b)
session: learn-pali-071-script-choice-answer
worktree: /Users/lsh/projects/palicanon/wt/learn-pali-071/learn-pali (branch `feat/script-choice-answer`), paired `wt/learn-pali-071/ui-lib` at v0.4.3

## Summary
Rank: difficulty 4 (a pure function in core with a golden test, one new card component in the session, the grade given by the app), impact 4 (every write card of the Devanāgarī deck, the phone first). No agents (user, 2026-10-22): the orchestrator builds; one review round with @deep; the user checks on the phone, on prod (dev is not reachable from the phone: 018).

063's design (node `quiz`): the script deck's Recall facet (write: the romanisation shown) offers four signs, the right one and three distractors from `distractors(item, deck, seed)`, a pure function in core: the same group first (the deck's `pos` label), topped up by the nearest signs by rank when the group is small (the niggahīta), never the answer, never a sign twice; `choices` mixes the answer in, in an order fixed by the seed (`seed_for(term, day)`: the same row across re-renders, another row another day); a golden test pins it. 063b shipped the reveal card instead (« commençons simple »). Now a `ChoiceCard` replaces the reveal card for that facet only: the romanisation and the group as today, then four sign buttons in a row (large Devanāgarī, `Pali` display size); a tap judges at once: the tapped sign is marked (`aria-pressed`), the right one named (`data-right`), a caption says right or not, and one button, Next, gives the grade (Pass when the tap was right, Fail otherwise) the way the grade buttons do. No 'Show the answer' on that card. The words deck and the script deck's Meaning facet keep the reveal card. Mobile: four buttons in one row at 390 px (a sign is one or two clusters), wrapping to two rows when wider signs need it.

Ruled out: revealing through ui-lib's RevealCard (its reveal is its own button or key; the quiz reveals on a tap, a different behaviour: one small component per behaviour); keyboard digits (a phone interaction first; Enter gives Next).
Open: nothing.

## Design
Viewer: learn-pali-071-script-choice-answer/design, from 063's `quiz` node.

## Log
| Step | Result | Output |
| --- | --- | --- |
| Define | ✓ | 063's design node, the user's order |
| Matrix | ✓ | CI's commands on the branch: fmt, leptosfmt, clippy ssr/hydrate/csr/native/core-wasm, test ssr and native, doc, csr check |
| Check (orchestrator) | ✓ | headless phone (390×844): the Devanāgarī deck started, read cards answered until a write card: four vowels in one row, a wrong tap pressed, the right sign named, Next moved on; scrollWidth 390; no exception; captures `scratchpad/071/quiz-before.png`, `quiz-after.png` |
| Build | ✓ | 304dce3 core `quiz` (distractors, choices, seed_for; 5 tests); `ReviewRun::choices` (2 tests); `choice_card` + `choice_button` + `choice_verdict` + `choice_next` in session.rs, `FacetCard` switches on it; strings `quiz_right/wrong/next/choices` in en/fr/vi; SSR tests: four signs and Next with no reveal, the read card keeps the reveal card, the strings |

## Reviews
### Round 1 — 2026-10-24 (@deep, review entry `d960935abd38446`)
1. The golden row printed, not pinned — TAKEN: `["ङ", "घ", "ख", "क"]` for क on seed 1.
2. The grade keys after a tap could give another grade — TAKEN: a choice card hands no gate (`revealed` stays `None`): the tap judges, a key does nothing; Enter on the focused Next gives the grade.
3. `tapped` keyed by (card key, term id) and a requeued card — closed with the fact: the key carries the run's counter, so a failed card comes back under a new key (test `a_failed_choice_card_comes_back_under_a_new_key_untapped`).
4. A 1-of-4 tap counted as a full Recall pass — by design (063's quiz node: the script deck's write facet IS the choice among four; the app judges); a slip is one Fail, the card comes back today.
5. The answer among the distractors, duplicates, short decks — the devanagari tests assert never-the-answer and no duplicate; TAKEN for short decks: fewer than four terms give fewer choices in core (tested) and the run then keeps the reveal card (`choices()` None under four).
6. Two terms with one romanisation in a group — TAKEN as a deck invariant test: no two signs of a group share a romanisation (the inventory holds).
7. Focus and announcement — TAKEN: the focus moves to Next after the tap (browser), the verdict is `aria-live="polite"` and names the sign in text; `data-right` is for the eye and the tests.

### Round 2 — 2026-10-24 (@deep, the fix diff)
1. `focus_next()` ran in the tap's handler while Next was still disabled — TAKEN: an Effect keyed on `answered`, after the render.
2. The live region created with its content — TAKEN: rendered from the start, empty (`role="status"`, `aria-live="polite"`), filled on the tap; the SSR test asserts it.
3. D1 asserted, not tested — closed with the code: the key gate reads `revealed` (`learn/key.rs`, `grade_key` answers None when it is false, tested there); a choice card sets it to None; `handed` feeds only the Context front.
4. The deck invariant test not visible in the cut diff — `core/src/quiz.rs::no_two_signs_of_a_group_share_a_romanisation`.
Two rounds at rank 4: the loop ends.

## Retro

## Merge
03f1e4c on main (squash), 2026-10-24; prod v0.1.17 (the user checks on the phone in prod: dev is not reachable from it).

## Retro
No agents: one line. The review's two rounds caught what a headless check cannot: a focus call that could not fire, a live region that would not announce, a golden test that printed instead of asserting. Four failed viewer posts were one missing brace, blamed twice on the tooling before the error's column was read.
