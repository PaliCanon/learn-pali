# 068 The passages asset and the fragment's dictionary port

status: merged · rank: 6 · level: hard · type: feat · request: the user, 2026-10-23 («dico pas dans l'app ?», «on a pas besoin de taper l'API, on a déjà tout en local», «lance dictionnaire et app») · session: learn-pali-068-passages-and-dictionary-port

## Summary
Rank: difficulty 6 (a pipeline over three corpus databases, a new asset, a port in the fragment with two implementations), impact 5 (the app gets the dictionary; the site's Add stops calling the API five times) → 6. No agents (the user's order): the orchestrator builds.
(1) `build-deck passages`: for every dictionary entry whose source maps to a SuttaCentral uid (38,707 of 52,114 examples), the deck's own method run over the LOCAL corpus databases (pāli + en, fr, vi: the API's assets, read with rusqlite through a `Fetch` that answers the API's JSON shapes from SQLite, so `learn_passage::translate_example` runs unchanged): the matched line, its blank on the marked word, the en/fr/vi lines with their translators. Output `passages-<date>.db` (lemma → passage), ≈ 15–20 MB, uploaded to the bucket, pinned by hash. (2) The fragment's `Dictionary` port (backend.rs, like `SyncTransport`): `search(q)`, `passage(lemma)`, `lookup(form)` with callbacks; `LearnProvider` takes `dictionary: Option<SharedDictionary>`; the search screen and the live lookup call the port. The site's implementation (hydrate, web.rs) calls the server functions; `passage_for` on the server reads the passages asset when configured (env `LEARN_PALI_PASSAGES`), else the live method. (3) The app (task app 008): ships dictionary.db, dpd.db and passages.db by hash (xtask), Tauri commands over SQLite, the front's port implementation.
Open: the asset's exact schema; the app's size (≈ 55 MB more).

## Log
| Step | Result | Output |
| --- | --- | --- |
| Build: pipeline | ✓ | 4fec8fc `build-deck passages`: CorpusFetch over the corpus SQLite files in the API's shapes; 3 tests; clippy, doc clean |
| Build: asset | ✓ | 50,852 entries, 34,186 found (en 33,581, fr 1,488, vi 1,837); misses unmapped_source 12,266, not_matched 4,117, text_not_served 283; 48 s; `passages-2026-10-23.db` 30.3 MB (superseded after round 1 by `passages-2026-10-24.db`, schema 1 + complete, same counts), uploaded to `gs://palicanon-app-assets/passages/` (no-clobber) |
| Build: port | ✓ | 14fbd99 `Dictionary` trait, `LearnProvider dictionary=`, `ServerDictionary` on the site, `search_unavailable` in en/fr/vi; 772 lib tests, clippy ssr/hydrate, csr check, doc clean |
| Worktree | note | moved to `wt/learn-pali-068/learn-pali` paired with `wt/learn-pali-068/ui-lib` at v0.4.3: `wt/068/ui-lib` belongs to ui-lib 068 (0.3.0), the `[patch]` was unused and Cargo.lock rewritten; restored |
| Matrix | ✓ | CI's commands on 690ea88: fmt, clippy ssr/hydrate/csr/native/core-wasm, test ssr (772) and native (249+27+5+76+90, after `cargo clean` of the moved worktree: `env!(CARGO_MANIFEST_DIR)` was baked at the old path), doc, msrv checks, deny, csr-without-web-store, rollout-check, release-values, origin-secret, api-token, lifecycle, proxy, tofu fmt; the browser tests run in CI (no chromedriver locally) |
| Check | ✓ | orchestrator (no visible change on the site, only the port behind it): the branch served on 127.0.0.1:3068 against api-dev, headless Chrome: 'Add a word' opens focused, 'dhamm' lists 10 entries through ServerDictionary, Add on the first answers 'Added' (passage found live); console: no panic. Branch 690ea88, ui-lib v0.4.3 (430ff07) |
| Define | ✓ | the user's words; the measures (52,114 examples, 38,707 served; dictionary.db 23 MB; corpus 77+34+58 MB) |
## Reviews
### Round 1 — 2026-10-24 (@deep, review entry `dd76663eab1004c`)
1. Two passage paths, nothing compares them — same code on both paths (`blank_example` + `translate_example`), only the Fetch differs; no test added, the switch is `followup-site-passages-asset`.
2. Nothing pins the asset's schema — TAKEN: `info.schema = 1`, `info.complete` written last, `check_asset()`, the app's reader refuses otherwise; T2 read-back test.
3. The row does not say its language — ruled out: one column per language with its translator.
4. The callback contract is a comment — the trait doc now states it; both hosts honour it; the lookup map is keyed by form (065 behaviour unchanged).
5. `build` writes in place — TAKEN: `<out>.part` then rename, checked before the move.

### Round 2 — 2026-10-24 (@deep, the fix diff)
Points 2 and 5 closed as mechanism; residual: `check_asset` pinned nothing but the row count — TAKEN: `COLUMNS` (what `SCHEMA` stands for) compared to `PRAGMA table_info`, one found row read through every column, the file reopened read-only after the write (not the writing connection), its counts compared to the build's; T2 extended (a dropped column is refused). Footnote on `rename` over an existing `out`: `out` is refused before the build, said in a comment. Rulings 1, 3, 4 stand (Deep: "do close what I raised"). The rebuilt asset is byte-identical to `passages-2026-10-24.db`. Two rounds: the loop ends (rank 4–7).

## Merge
966db20 on main (squash of 40398b0), 2026-10-24; tag `fragment-v0.5.0` for the app.

## Retro
No agents (user, 2026-10-22): one line. What cost time: a worktree folder shared with another repository's task of the same number (`wt/068/ui-lib` was ui-lib 068's, at 0.3.0: the `[patch]` went unused, Cargo.lock was rewritten, and after the move `env!(CARGO_MANIFEST_DIR)` stayed baked in the test binaries until a `cargo clean`); gcloud's sliced download crashing on 77 MB files (curl with an access token worked). What the review caught: nothing pinned the asset's schema and a stopped build left a whole-looking file. Lesson: a worktree folder carries the repository's name when two repositories can share a task number (`wt/<repo>-<NNN>/`).
