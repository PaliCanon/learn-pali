# 063 Decks: several decks in Learn, the Devanāgarī alphabet first, Anki import later

status: merged · rank: 6 · level: hard · type: feat · session: learn-pali-063-decks · request: the user, 2026-10-22 («j'aimerais bien apprendre le pali, l'alphabet. Devanāgarī. Est-ce que learn peut gérer des decks ?» · «on commence par devanagari seule d'abord. Regarde Anki … ce serait bien d'être compatible» · «je pensais surtout à importer. Mais commençons simple et lance un design que ça ne se perde pas») · after: [059]

## Summary
Rank: difficulty 6 (the progress file's version 3 with a migration, a deck kind with its own facets, the home's chooser, a new builder command and an inventory, the sync per deck), impact 5 (every learner's progress file; a new way to learn) → 6.
Learn gets several decks: a deck has a kind that names its facets (words: today's four; script: read and write; basic: front/back for imported notes later); one schedule for every kind; the progress file goes to version 3 with a per-deck key (a version-2 file reads as the DPD deck's progress); the home shows the selected deck with a chooser. The first new deck is the Devanāgarī alphabet (vowels, consonants, niggahīta, vowel signs, the frequent Pāli conjuncts counted in our own text), built by build-deck from a reviewed inventory table. Anki: import of Basic-note .apkg packages (Legacy 1/2, a hand-written reader, no AGPL code) as the compatibility target, a task of its own after the model; export later. Research: discussion 14524. Waits for 059's merge (the progress file and the sync change there).
Consensus 2026-10-22 (v4: deep 14539/14560/14565, sage thin; gemini 429; summary 14568). The user answered: a first visit lands on Words (the chooser offers Devanāgarī); the inventory table (8 vowels, 33 consonants, the niggahīta, the vowel signs, 40 conjuncts counted in our text) is reviewed by the user BEFORE the build. Cut: 063a the model (the Book of per-deck progress, the kind, the selected deck, the chooser hidden while one deck, the CLI --deck; no visible change), 063b the Devanāgarī deck (the conjunct counter, the `devanagari` builder subcommand, the 4-choice question, the font), 063c the Anki import (Basic notes, its own diagram). All after 059's merge; the desktop app bumps its pin after each. Ruling 2026-10-22 (063a, the CLI): `--deck <file>` stays the deck file path; `--deck-id <id>` (default `words`) names the Book entry the state file's commands act on; assumptions A1-A3 accepted (the Book in ProgressStore, Backend maps it to the selected deck's State, the selected deck a second localStorage key, the desktop app absorbs the change at its pin bump).

## Log
| Step | Result | Output |
| --- | --- | --- |
| Research | done | researcher 14524 (Anki formats, crates, decks, the Pāli inventory, sync precedents) |
| Define | done | design v4 at consensus; the user's answers 2026-10-22 (Words first; the inventory reviewed before the build) |
| Plan | done | 6 lanes (L1 Book, L2 kind+facets, L3 store on the Book, L6 CLI, L4 sync per deck, L5 chooser+screens), lead 2a3fd2a; the lead stopped by the user's order (no more agents) after integrating every lane |
| Build | done | 0133f9f core Book; 0ce8ac4 kind and facet rule; 3efa92c CLI --deck-id; bd7b639 store/backend on the Book; 38b9c25 sync per deck; 2404773 chooser and screens; 02fc326 README |
| Verify | done | on 02fc326, ui-lib v0.4.0 (wt/063/ui-lib): test_core 0, test_app (ssr) 0, test_cli 0, clippy_ssr 0, clippy_core 0, fmt 0 (orchestrator, scratch/063/check.sh) |

## Design
Viewer: learn-pali-063-decks/design (d36db2a9ba9f444), v4, consensus 2026-10-22.
```mermaid
flowchart TB
  subgraph model["Decks in Learn (request 063, the user 2026-10-22: start simple, do not lose it) - task 063a"]
    deck["a deck = an id, a name per language, a kind, its items. `words` and `devanagari` have fixed ids and are compiled into the site (include_str!, one JSON file per deck; the Deck file gains `kind`, absent = words). An item is today's Term: for a script item lemma = the sign, pos = its group label, meaning = its romanisation, rank = its order, id = the sign itself (stable, nothing to mint). An imported deck (later) has an id derived from a hash of its notes' guids"]
    kind["kind = `words` | `script`, in learn_core beside deck.rs and facet.rs (build-deck validates what it writes); `basic` joins with the Anki import task, additively. The KIND owns the facet rule: `facets_of(deck, term)`: words = today's rule; script = EXACTLY {Meaning = read: the sign shown, the romanisation revealed, self-judged; Recall = write: the romanisation shown, 4 signs offered, the app judges} whatever the item carries (an example with a blank gives a script item no Context). The Facet enum does not grow; FacetSet and the merge are unchanged"]
    sched["one schedule for every kind: the six stages, a card = (deck, item, facet). A Session takes ONE deck and that deck's State (Session::start, nearest, new_left and grown are unchanged); the home's counts, 'ahead' and the new-terms controls act on the selected deck; every chooser chip shows its own deck's due count, so cards due in the other deck are seen"]
    state["the progress file = a Book: `{version, decks: {deck_id: {last_today, progress, added}}}`; each entry is exactly today's State body, so session, merge, home and the CLI keep working on one State (a wrapper, not a rewrite). Stamp: the Book is WRITTEN in today's flat shape, stamp 1 or 2, iff every entry other than `words` has an empty `progress` and an empty `added` (an entry holding only a last_today stays flat); stamp 3 otherwise. Under SYNC progress only grows (the join), so the rule is monotone; Settings' import is an explicit REPLACE by the whole file, as today (an old flat backup returns the Book to flat), and its dialog says it replaces every deck. An older build meets a stamp 3 file with `from_json_upto(text, 2)`, stops the sync and never overwrites; a test runs that call on a v3 Book. A flat v1/v2 file reads as `decks: {words: it}`. Merge = today's join per deck id; a deck only one side has is kept whole; an unknown deck id is kept as it is. last_today and added stay per deck (added words belong to `words`). Settings' export writes the Book; import reads any version and wraps it"]
    sel["the selected deck: a DEVICE setting in the store, not in the Book and not synced (phone on the alphabet, desktop on Words is fine); one key read by the site and by `learn --deck <id>` (the desktop app follows in its own pin-bump task and reads the same key); default `words` (U1); an id the build does not know falls back to `words`"]
    home["the home shows the selected deck: a chooser at the top (chips Words, Devanagari, each with its due count; hidden while only one deck exists), the six stages' counts of that deck, Practice, ahead, the new-terms controls. Phone: chips wrap, no hover-only interaction. en/fr/vi, no em dash"]
  end
  subgraph deva["The Devanagari deck (first new deck; Devanagari only, the user) - task 063b"]
    items["items: the 8 vowels (a ā i ī u ū e o; independent forms and signs), the 33 consonants in their groups, the niggahīta ṃ, the vowel signs on क, then the top N frequent conjuncts (N=40 unless the user says); each: the sign (Unicode), the IAST romanisation, a group label, optionally an example Pāli word in both scripts"]
    order["order: vowels, consonants by group, vowel signs on क, conjuncts by frequency; 'new' takes them in that order"]
    count["`build-deck conjuncts --out table.json`: counts the conjuncts in our Devanagari text through the API (cached with --cache like today) and writes a proposal table with `source: api` and a count per row; the user and the lead review it and commit it into the inventory. The deck build never calls the API. If the API serves no Devanagari text, the list is hand-picked and the inventory says `source: manual`; --strict validates it: `api` requires every row's count above 0, `manual` carries no counts. The counting arithmetic is tested on a fixture text; the API only through a pinned --cache directory; no --check against the live API (it would need the network in CI)"]
    data["`build-deck devanagari` (a subcommand; the DPD command is unchanged) builds the deck from the committed inventory table, offline and reproducible. --strict and a test pin: the size, ids unique, each sign one Devanagari cluster (a consonant (virama consonant)* [vowel sign] [niggahīta], by our own check, not a grapheme-segmentation crate), exactly two facets per item (a negative fixture: a script item whose example carries a blank must get no Context), every item has 3 distractors"]
    quiz["the write facet: 4 signs, the right one and 3 distractors from `distractors(item, deck, seed)`, a PURE function in core (same group first, topped up by the nearest items by rank when the group is small, as for the niggahīta; never the answer, never a repeated sign; a golden test pins the triple per seed); a wrong choice is Answer::Fail, a right one Pass (the core's Answer is unchanged, the view maps the choice). Typing the sign: later"]
    font["the full Noto Sans Devanagari woff2 (OFL, credited in the deck's source line), served by the site with unicode-range U+0900-097F so it loads only where Devanagari appears, so conjuncts form; tests: the stylesheet names it, and the font file's cmap covers every codepoint of the inventory (a dev-dependency cmap reader); the ligatures themselves are looked at in the browser capture"]
  end
  subgraph anki["Anki compatibility (the user: import first; after the model) - task 063c"]
    imp["import an .apkg: Legacy 1/2 packages (zip + SQLite schema 11) read by a hand-written reader (zip + rusqlite, no AGPL code); Basic note types only (Front, Back) become a `basic` deck stored in the Book next to its progress (so it syncs); item id = the note guid, deck id = hash of the guids, so two devices importing the same package converge. 063c brings its own diagram and fixtures (a Legacy 2 package, a newest one for the hint, a non-Basic one). Other note types refused with a clear line; the newest anki21b refused with the hint 'Support older Anki versions'"]
    exp["export a deck as .apkg (cards only, no schedule, stable guids; genanki-rs, MIT): later, on demand"]
  end
  cut["cut: 063a model (Book, kind, selected deck, chooser hidden while one deck, CLI --deck; ships invisible) then 063b the alphabet deck (after 063a; if the API serves no Devanagari text, the alphabet ships without conjuncts and they follow) then 063c the import; the desktop app follows each by a pin bump in its own task; all wait for 059's merge"]
  tests["tests: the migration flat to Book and back byte-for-byte for a words-only learner; `devanagari: {last_today, progress: [], added: []}` stays flat byte-for-byte, one progress entry gives stamp 3; the stamp-3 file refused by from_json_upto(.., 2); merge(local{devanagari}, flat) keeps `devanagari` whole; an import of a flat file replaces the Book; the merge laws per deck (commutative, associative, idempotent); a session per selected deck; the selection fallback; the chooser; script facets exactly two; the distractors golden per seed; the inventory pins; the conjunct counter on a fixture text; csr/browser: choose the deck, practice a sign, the count on the home"]
  deck --> kind --> sched --> state --> home
  sel --> home
  items --> order --> data
  count --> items
  data --> quiz
  data --> font
  model --> deva
  model -.-> anki
  cut -.-> model
  tests -.-> model
  tests -.-> deva
```

## Reviews
### Round 1 — 2026-10-22 (orchestrator; voice deep 14649/14650 in review-discussion d42415c460cee4a)
No agents (the user's order): the orchestrator read the diff against the design's `model` subgraph and posted it to deep. Deep answered with design questions, not a diff review; each is settled by the code and its tests:
1. 063c's `basic` deck storing its items in the Book breaks "the entry is exactly today's State body": DEFERRED to 063c (an optional `items` key the stamp rule ignores, decided there; the byte-for-byte tests then take a fixture).
2. "Hidden while one deck" counts the decks the build compiles (`Backend::decks().len() > 1`, `offered`): from 063b the chooser shows, as intended; a Book id the build does not know gets no chip and is kept whole by the merge (`an_unknown_deck_id_is_kept_as_it_is`, `an_unknown_deck_on_the_drive_survives_the_cycle_and_is_pushed_back`).
3. The selected-deck key: `pc_learn_deck` in the site's store (web.rs); the CLI has no store and takes `--deck-id` (default `words`); the desktop app reads the same key at its pin bump (its task). An unknown stored id falls back to words (`a_stored_id_the_registry_lacks_presses_the_words_chip`).
4. A deck absent from the Book reads as an empty State and writes nothing until a term starts (`the_state_of_an_absent_deck_is_empty`, `selecting_writes_no_book`, `devanagari_holding_only_a_last_today_stays_flat`).
5. The flag is `--deck-id` (ruling 2026-10-22 in the Summary); the design's `learn --deck <id>` wording is superseded.
6. Tests asked: unknown id through merge (`an_unknown_id_is_kept_by_a_merge`), stamp 3 from `added` (`one_added_word_in_devanagari_gives_stamp_3`), the fallback (3.) all exist.
Findings: none open. Δ: none against the `model` subgraph beyond 5. (the flag name, ruled).

## Lanes
Worktrees `wt/063/<lane>/learn-pali`, branch `feat/decks-<lane>`, each from the task tip, ui-lib paired by symlink to `wt/063/ui-lib` (v0.4.0). Briefs: one file per lane (lead scratch). Baseline (cargo test, rebased tip 3ea62d9): learn_core lib 213 + fixture 5, learn_cli 67 + 19, build_deck 112, learn_pali ssr lib 688 + assets 9 + cfg_gates 9 + no_build_git 3 + origin 6.
| Lane | Files | Tests | Code | Wave | Done |
| --- | --- | --- | --- | --- | --- |
| L1 the Book | core/src/book.rs, core/src/lib.rs | flat/stamp-3 read and write, 2 to 3 migration, older build refusal, merge per deck + laws | DeckId, Book, merge_books | 1 | ✓ |
| L2 kind + facets | core/src/kind.rs, deck.rs, facet.rs, progress.rs, session.rs, added.rs, forms.rs, tools/build-deck/src/build.rs, app home_state.rs, term_list.rs | script facets exactly two, kind serde, session over a script deck | DeckKind, facets_of(deck, term), effective_passed(progress, deck, term) | 1 | ✓ |
| L3 app store on the Book | app/src/store.rs, backend.rs, web.rs, test_support.rs, deck.rs, data_block.rs, save_strip.rs, fragment.rs | Load<Book>, facade leaves other decks alone, selection key and fallback, export/import the Book | ProgressStore on the Book, Backend load/save on the selected deck, registry, `pc_learn_deck` | 2 | ✓ |
| L6 CLI | cli/src/*.rs, cli/tests/cli.rs, README.md | --deck-id, flat stays flat, another deck intact | load_state/save_state on the Book | 2 | ✓ |
| L4 sync per deck | app/src/sync.rs (and sync/gate.rs only if it breaks) | join per deck, unknown deck kept, count per deck, refusals unchanged | local()/pulled()/join() on Books | 3 | ✓ |
| L5 chooser + screens | app/src/pages/home.rs, components/session.rs, add_word.rs, term_list.rs, learn/run.rs, learn/adding.rs, new component deck_chooser.rs, locales | chooser hidden while one deck, chips with due counts, a session per selected deck, fallback | selected deck used by every screen | 3 | ✓ |
| Review | done | round 1: deep's six points answered by the code and its tests (Reviews); 063c note; no finding open |
| Check | done | orchestrator on learn.dev (feat/decks · 95bed39, run 39781988052): headless Chrome dump-dom: data-hydrated set, no panic, no chooser in the markup (the one role=group is ui-lib's hidden language menu), the empty home as before; no visible change by design |
| Merge 063a | done | be2465d on main (squash); 063b (the Devanāgarī deck) continues on this file, branch feat/devanagari-deck; prod tag with 063b |

## Merge
063a: be2465d
| Build 063b | done | orchestrator by hand (no agents): the builder's devanagari subcommand and the inventory table (test first in script.rs: 6 tests, red by compile), the second compiled deck, the names and group words; 4 tests adapted to a two-deck build |
| Verify 063b | done | test_core 0, test_app 0 (746), test_cli 0, build_deck 118, clippy ssr/rest 0, fmt 0, doc (CI's command) 0; deviation from the design, ruled by the orchestrator (the user: start simple): the write facet is a reveal card like the words' Recall, the 4-choice answer and the shipped Noto font are follow-ups (followup-script-choice-answer, followup-devanagari-font); the example word in both scripts not built |
| Check 063b | doing | on learn.dev (feat/devanagari-deck · dd994ae, run 39785300562): headless Chrome: hydrated, no panic, the chooser with Words (pressed) and Devanāgarī; waiting for the user |
| Fix 063b (check) | done | the New terms dialog a popup (dae89d0); ui-lib v0.4.1 (7169752 af45254 ): Pāli one size bigger, the + a drawn icon centred; 746 app tests, clippy ssr, fmt green |
| Check 063b | doing | on learn.dev (feat/devanagari-deck · 9aec18e, runs 39796681223 and 39797603446): the chooser, the Devanāgarī deck, the New terms popup, Pāli one size bigger, the drawn +; waiting for the user |
| Check 063b | done | the user, 2026-10-22: «tag et prod !» on 9aec18e |
| Merge 063b | done | 935cc33 on main (squash); prod tag v0.1.12; 063c (the Anki import) becomes its own task file when it starts |

## Retro
By the orchestrator alone (the user's order 2026-10-22: no more agents). 063a: the lead and six lanes cost 256k + lanes before the order; the lead's plan read code for nine minutes (now: a lead opens no source file; then no lead at all). 063b by hand: ~1 hour, three check fixes from the user (popup, Pāli size, the + icon) in the same deploy. Errors: the DeckKind import path guessed wrong once; a golden fixture folder in ui-lib held the old classes (found by the test); four tests assumed a one-deck build (a test registry fixed them). Lessons: a check on dev brings the user's eye to everything on the screen, not only the task (three unrelated asks landed in one round: fine, each a commit); the inventory came from counting our own text through the API, no hand-picking. Retro 063 validated.

## Merge
063a: be2465d · 063b: 935cc33
