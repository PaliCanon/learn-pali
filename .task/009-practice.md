# 009 Practice: study ahead without moving a stage

status: design · rank: 4 · level: standard · type: feature · request: learn-pali (user, 2026-10-06) · after: [008]

## Summary
The orchestrator proposed, the user agreed (2026-10-06): « Oui ça peut être bien, et la practice prend de tous les decks ? »
Decided (to confirm at the user check):
- **A Practice button** when nothing is due (where 'Next review: <date>' shows today, beside it): `[ ▶ Practice ]`, the compact form of Resume (008).
- **What it asks**: the started terms with the nearest next-review dates, from **every stage** (the user's "all decks": the six stages); when a stage is selected in the strip, only that stage's terms. A batch of 5 terms, every facet each has.
- **What it changes**: nothing in the progress — no stage moves, no lapse, no date; the same rule as learn_core's practice questions after a fail (`Question { practice: true }`): « a practice question changes nothing in the state, Pass or Fail ».
- **Where it lives**: learn_core gains `Session::practice(deck, state, terms)` (a session whose every question is practice), tested natively; the home starts it in place like Resume; the summary says "Practice done" with no "moved up" line.
Open: the batch size (5, or the + dialog's choice); Practice also offered when cards are due (today: only when nothing is due).
Rank 4: difficulty 3 (a core session mode, its rule pair with the normal session), impact 4 (the learner's state must stay untouched).

## Log
| Step | Result | Output |
| --- | --- | --- |
| Define | doing | the proposal and the user's yes; built after 008 (same files) |

## Retro
