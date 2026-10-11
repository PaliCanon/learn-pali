# 026 Tap a word of the example, see its meaning

status: review · rank: 5 · level: standard · type: feature · request: learn-pali (user, 2026-10-11: « in the definition, I want to be able to click and see the meaning of the word », a capture of the 'ayaṃ' Recall card's example, 'atha' as the example) · after: [ui-lib 059]

## Summary
On a card's reveal, every word of the example's Pāli line can be pressed to see its meaning (DPD). Decided:
- **Offline, in the deck** (the site has no backend and the deck is embedded): `build_deck` resolves every word of every example with the DPD's `lookup` table (`lookup_key` → `headwords` ids → `dpd_headwords`: `lemma_1`, `pos`, `meaning_1` (or `meaning_2`), and `lookup.grammar` for the form's inflection, e.g. āmantesi → 'āmanteti · verb · pr 2nd sg'); sandhi via `lookup.deconstructor` (`sampādethā'ti` → sampādetha + iti: both parts' entries); normalised keys (lower case, ṁ → ṃ, quotes stripped). The deck gains one shared `words: { form: [entry…] }` map (deduplicated across examples, at most 3 entries per form, the deck's own terms first); `Deck` reads old decks without it. Size measured and reported (the deck is 224 kB today).
- **The UI**: ui-lib's `PaliText` (059) renders the example lines on the reveal (Meaning's 'Example', Context's filled line, Recall's example); a word with entries is pressable; desktop popover, phone bottom sheet (059). Not on the Context front (its line is the question).
- **Language**: the DPD meanings are English; fr/vi when 004 brings glosses.
- **Licence**: DPD CC BY-NC-SA 4.0, as the deck already is (credit line already a follow-up).
Rank 5: a pipeline step, a deck format field, a ui-lib dependency, mobile behaviour.

## Log
| Step | Result | Output |
| --- | --- | --- |
| Build | done | lane Opus: core Deck.words + word_key, build-deck lookups (454/454 forms; sandhi with an apostrophe shows its parts), the fixture deck as compact JSON (238,812 B), dee93ac the reveal's examples through ui-lib PaliText (Context line: the stretches between blanks, the gold answer not clickable); 327 ssr + workspace tests; clippy ×4, fmt, leptosfmt, rustdoc, deny EXIT 0 |
| Capture | done | orchestrator, :3201 on the branch: no word button before the reveal; 16 after; 'sutaṃ' opens 'suta 1.1 · pp · nt nom sg… · heard'; grade keys still work with a panel open; no page error. Seen: the panel is clipped by the card's scrolling body near its bottom; a console warning from ui-lib's catalogue (a signal read outside tracking) — both noted, not blocking |
| Define | done | the DPD lookup table checked (atha → 2 headwords, āmantesi grammar, sampādethāti deconstructor); offline chosen (no backend, no token at run time) |

## Retro
