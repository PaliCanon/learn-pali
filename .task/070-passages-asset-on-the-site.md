# 070 The site reads the passages asset

status: review · rank: 4 · level: standard · type: feat · request: the user, 2026-10-24 (« learn pali fix les 5 calls »), from `followup-site-passages-asset` (068)
session: learn-pali-070-passages-asset-on-the-site
worktree: /Users/lsh/projects/palicanon/wt/learn-pali-070/learn-pali (branch `feat/passages-asset-on-the-site`), paired `wt/learn-pali-070/ui-lib` at v0.4.3

## Summary
Rank: difficulty 4 (one SQLite read on the server, the asset in the image by hash, a deploy step), impact 4 (every added word: one local read instead of five API calls and an 8 s budget). No agents (user, 2026-10-22): the orchestrator builds; one review round with @deep.

Adding a word on the site ran the deck's method live: `/v1/sutta/{uid}` then the texts, five API calls, 8 s at worst. The asset built by 068 (`passages-2026-10-24.db`, 30.3 MB, schema 1) holds that answer for every entry. Now:
- **`learn_passage::asset`**: the reader of a passages asset, shared with the tools: `open(path)` checks `info.schema = 1` and `info.complete`, `row(lemma)` gives the line, its blank, each language's line and translator, or the miss by name; a lemma with no row is `None`.
- **The server** (`dictionary::server::passage`): when `LEARN_PALI_PASSAGES` names an asset that opens, the entry's lemma is read there (found, or the miss by name); a lemma the asset has no row for, an asset that does not open or is not configured: today's live method, unchanged. The outcome is logged with its source (`asset` / `live`).
- **The image**: `app/assets.lock` pins the asset's object and SHA-256 (under `app/`, so in the image's input hash); the deploy workflow fetches it into `learn-pali/assets/` (gitignored) before the build and checks the digest; the Dockerfile copies it to `/app/passages.db` and sets `LEARN_PALI_PASSAGES`.

Ruled out: the API serving the asset (one more hop); shipping the corpora.
Open: a dictionary newer than the asset (an entry with no row) falls back to live; the asset is rebuilt with the dictionary.

## Log
| Step | Result | Output |
| --- | --- | --- |
| Define | ✓ | 068's design (2), the site node; the user's order |
| Build | ✓ | d60c50d `learn_passage::asset` (2 tests), `server::passage_from_asset` (1 test), `app/assets.lock`, `scripts/fetch-passages.sh` + test (in ci.yml), Dockerfile COPY + ENV, deploy step |
| Matrix | ✓ | CI's commands on d60c50d: fmt, clippy ssr/native/hydrate/csr, test ssr/native, doc, csr check, deny, the three script tests (browser tests in CI) |
| Review | ✓ | two rounds with @deep (one empty answer first): round 1 five points taken, one ruled; round 2 one taken, two ruled; see Reviews |
| Check | ✓ | orchestrator (no visible change): the branch served on 127.0.0.1:3070 with `LEARN_PALI_PASSAGES` = the local asset: `POST /api/passage_for` for `dhamma 1.01` answers Found from the asset (log: `found (asset)`); a lemma with no row falls to the live method (log: `no row … live`). The image build itself is checked by the dev deploy after the merge. After round 1: a bad `LEARN_PALI_PASSAGES` stops the server at start with the reason (checked); the good asset logs `passages: <path>` and answers `found (asset)` |

## Reviews
### Round 1 — 2026-10-24 (@deep, after one empty answer; review entry `d59631834e91845`)
1. A broken asset passed for a missing row (every error fell to the live method, one log line per request) — TAKEN: the asset is opened once at startup (`init_passages`, `main.rs`): a configured asset that does not open, is of another schema or is not complete stops the server (the rollout check then refuses the revision); the request path reads the open asset; only "no row" and "retry" fall to the live method.
2. A build-time fetch failure frozen as a permanent miss — TAKEN: `fetch` (and any name the reader does not know) is `Row::Retry`: the live method runs.
3. Stringly-typed miss names on both sides — TAKEN: `learn_passage::asset::MissName` (as_str / parse) used by the writer and the reader; the desktop app keeps its own mapping over the same strings (another repository: noted for app).
4. A `JoinError` swallowed — TAKEN: logged, then live.
5. Δ1/Δ2 and no timeout on the asset path — TAKEN with 1: opened once; the read runs under `PASSAGE_BUDGET` like the live method.
6. The path test compares strings — closed with reason: the build context is the parent of the checkout (every COPY says `learn-pali/…`), and the dev deploy builds the image for real after the merge.
Tests: T1 routing (the asset answers, the fake API is not asked; no row asks it), the startup refusal; T2 the fetch row retried live; T4 an unknown name retried live; T6 the fetch script (no passages line refused, a stale file replaced). T3 (a panic in the blocking read) and T5 (64 concurrent reads) not added: the read is a Mutex'd SQLite query under the budget; noted.

### Round 2 — 2026-10-24 (@deep, the fix diff)
1, 2, 3, 4 closed by the voice. Still open for it: (a) `init_passages` with no call site — it is called in `main.rs` (the diff's `app/src/main.rs` hunk, cut by the paste's length): the server exits on `Err`, and `tests/origin.rs` pins the boot lines; closed. (b) The budget does not cancel the blocking read, a wedged file piles threads on the mutex — ruled out with the reason: the read is one row by primary key in a local SQLite file opened read-only; a wedge there is a disk failure, where the live method is no better; the budget bounds the answer the learner waits for, which is what 062 set it for. (c) Worst case 2× the budget — TAKEN: one deadline, the live method gets the remainder. Minor: `init_passages` twice keeps the first asset (startup only); a request-time query error logs per request (rare, wanted loud). Two rounds at rank 4: the loop ends.

## Retro
