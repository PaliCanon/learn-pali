# 005 Deck management

status: design · rank: — (ranked at Define) · level: — · type: feature · request: learn-pali (user, 2026-10-06) · session: learn-pali-005-decks · after: [003 screens, 004 deck pipeline]

Not started: defined later with the user ("ce serait bien plus tard qu'on puisse gérer des decks").

## Summary

Asked (user, 2026-10-06, during 003's check): « créer une task gestion de decks, ce serait bien plus tard qu'on puisse gérer des decks ». Today the site has one deck, built from the DPD by `build_deck` and embedded in the wasm (003); 004 builds the real one (curated order, the "why now" line, fr glosses). The progress (`pc_learn`) keys terms by id and knows no deck.

**The user's answers so far (2026-10-11, relayed by a lane):** the learner **creates a deck and adds cards to it**; a deck has **a defined format to export and import it**, so decks can be shared when wanted; **everything stays local** — decks and progress in the browser, no server.

Questions for Define (`docs/ask-yourself/`):
- What is a deck to the learner: a theme (the four noble truths, a sutta's vocabulary), a level, a source (a nikāya), their own list?
- Who makes decks: the learner (answered: yes, they create decks and add cards); does the project still ship one (004's built deck as a starting deck, or a sample)?
- One progress across decks (a term known once, whatever deck shows it) or one per deck? The tag in a term row today shows the stage; a deck name would need its own place.
- Where decks live: in the browser (answered: local, no server) — the same storage as the progress (`pc_learn`) or its own key; how a card links to its progress (term ids across decks).
- The deck format: its fields (a card = a DPD term id? or free text front/back?), its version, an invalid or duplicate import (the progress import's rules as the model: refuse, say why, never overwrite silently).
- The UI: a deck picker on the home (one dynamic page, no URL state: 003's rule), per-deck counts in the strip?
- State versioning: `State` v1 has no deck field; a v2 and its migration.

Ruled out for now: anything before 003 and 004 merge.
