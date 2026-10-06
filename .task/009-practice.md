# 009 Practice: study ahead without moving a stage

status: building · rank: 4 · level: standard · type: feature · request: learn-pali (user, 2026-10-06) · after: [008, ui-lib 056]

## Summary
The orchestrator proposed, the user agreed (2026-10-06): « Oui ça peut être bien, et la practice prend de tous les decks ? »
Decided (the user, 2026-10-06: « pourquoi ne pas avoir que practice comme bouton ? on resume s'il reste des cards, sinon practice ? pourquoi pas 5 de base, et après un slider pour augmenter / diminuer ? »):
- **One button, Practice**: `[ ▶ Practice · 15 ]` while cards are due — it runs the normal review (stages move, as Resume did); `[ ▶ Practice ]` when nothing is due — it runs a practice session ahead. 'Resume' and 'Review the next stage' go (the next-stage advance becomes the practice-ahead path: the nearest dates, every stage). The accessible name says which: 'Practice, 15 cards due' / 'Practice ahead, 5 terms'.
- **How many, when practising ahead**: 5 terms by default, a slider beside the button (1 to the number of started terms, step 1, the value shown), its value kept for the session (page state). While cards are due the slider is hidden (the due cards are the batch).
- **What practice ahead asks**: the started terms with the nearest next-review dates, from every stage (the six stages are what the user calls "decks"); when a stage is selected in the strip, only that stage. Every facet each term has.
- **What it changes**: nothing in the progress — no stage moves, no lapse, no date (learn_core's practice rule: « a practice question changes nothing in the state, Pass or Fail »).
- **Where it lives**: learn_core `Session::practice(deck, state, terms)` (every question practice), tested natively; the slider is generic → ui-lib (task 056 `Slider`: a styled range input with its value, tokens only, keyboard and touch, a gallery section); the summary of a practice says "Practice done", no "moved up".
Rank 4: difficulty 3 (a core session mode, its rule pair with the normal session), impact 4 (the learner's state must stay untouched).

## Log
| Step | Result | Output |
| --- | --- | --- |
| Define | done | the proposal, the user's yes, then one Practice button and a slider (the user's words); after 008 (merged e1ac116) and ui-lib 056 (the slider) |

## Retro
