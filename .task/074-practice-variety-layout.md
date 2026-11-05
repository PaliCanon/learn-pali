# 074 Practice variety and compact reading layout

status: validated, publishing · rank: 3 · request: user, 2026-11-05

## Summary

Extra practice draws up to the requested number of distinct started terms
from the selected stage, including added words, then shuffles their facets.
Due review and progress transitions are unchanged. Consecutive random draws
can overlap; selecting the whole pool necessarily repeats its words.

Practice cards fit their content and sit centred beneath the brand. Long
answers scroll with the page. Prompts, Pāli passages, translations and answers
have separate blocks. Linguistic details are compact labelled rows; Recall
no longer repeats its prompt's meaning or literal. Grade buttons appear only
after reveal. Dictionary entries have clearer spacing and small Chip actions.
The sync hint separates its question and instruction in all three locales.

## Validation

- User reviewed iterations locally in dark/light themes and mobile layouts,
  accepted the result and requested dev and prod deployment on 2026-11-05.
- 257 core and 786 app unit tests passed, including sampled subset variation,
  stage filtering, unchanged progress and grade actions hidden before reveal.
- SSR/core Clippy (all targets, warnings denied), formatting and whitespace
  checks passed. Full Leptos SSR/hydrate build passed.
- Checked in an isolated source copy paired with pinned ui-lib v0.4.5;
  sibling ui-lib development checkout and Cargo.lock remain unchanged.
- Automated visual capture unavailable (browser host unavailable); visual
  acceptance came from the user's local checks and screenshots.
- Old counter test assumed nearest-term selection; its fixture now explicitly
  contains the term whose facets it counts.

## Publication

PR checks, dev rollout, then production release tag v0.1.20.
