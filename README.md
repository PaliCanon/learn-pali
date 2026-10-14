# learn-pali

Learning Pāli — a PaliCanon service, Leptos (SSR + hydrate) on
[`PaliCanon/ui-lib`](https://github.com/PaliCanon/ui-lib) (never `ui-lab`).
It holds the site's frame (the shell, the rail of services, the footer),
the **learning loop as a library** (also tried at the terminal) and the
one page built on it: the home, which holds the progress and runs the
review sessions in place.

**Holds**: the service's own code, content, routes and screens; its
infrastructure, CI and deploys.
**Never holds**: shared UI (that is `../ui-lib`), search indexes or
stores (`../engine`), the API gateway (`../api`), secrets.

## What is here

| Folder | What it holds |
| --- | --- |
| `app/` | Crate `learn_pali`, the site: Leptos SSR + hydrate on ui-lib. |
| `app/src/` | Server code: the app shell (app.rs), entry point (main.rs), origin guard (origin.rs), progress store (store.rs), embedded deck (deck.rs), the UI strings' catalogue (catalogue.rs), the seed of a session's random order (seed.rs), build info (git_info.rs). |
| `app/src/pages/` | Page views, the home at `/`. |
| `app/src/components/` | UI components: progress blocks, new terms picker, review session, term list, site footer. |
| `app/src/learn/` | Learning session logic: home state, session run, Leitner stages, card summary, word lookups, the Context front's window around the blank, which translation shows. |
| `app/locales/` | UI translations: en.json, fr.json, vi.json. |
| `app/style/` | Stylesheet (main.css), imports ui-lib's theme. |
| `app/public/` | Static assets: favicon and logo. |
| `app/tests/` | Integration tests: asset load and origin guard. |
| `app/Dockerfile` | Container image build: cargo leptos build, debian runtime, non-root user, port 8080. |
| `core/` | Crate `learn_core`, the learning loop library. `serde` only; builds for native and wasm32. |
| `core/src/` | Learning modules: deck, Leitner stages, session, progress state, facets, word forms, shuffle, stats, date math. |
| `core/fixtures/` | Test fixtures: 50 terms (terms.txt) and the built deck (deck.json). |
| `core/tests/` | Integration tests. |
| `cli/` | Crate `learn_cli`, bin `learn-cli`, the learning loop at the terminal. Commands: review, stats, new N, advance K. |
| `cli/src/` | CLI code: argument parsing, terminal UI, session runner, stats printer, progress store. |
| `cli/tests/` | CLI tests. |
| `tools/build-deck/` | Crate `build_deck`, bin `build-deck`, builds the deck JSON from the Digital Pāḷi Dictionary. Never part of the site's build. |
| `tools/build-deck/src/` | Deck build: DPD parsing, word inflection, form lookup, translation via API, example segment alignment, source tracking. |
| `tools/build-deck/fixtures/` | Test fixtures for build and translation. |
| `infra/` | OpenTofu: Cloud Run service, Secret Manager, Cloudflare Worker and custom domain, per environment. |
| `infra/worker/` | Cloudflare Worker: origin proxy (proxy.js) with node tests. |
| `infra/scripts/` | Origin secret generation script and its test. |
| `scripts/` | Repository scripts: deploy health check and rollout, Cloudflare access redirect pattern. |
| `.github/workflows/` | CI: format, lint, test, doc, MSRV, coverage, deny checks (ci.yml). Deploy: image build, Cloud Run deploy, health check (deploy.yml). |
| `.github/actions/ui-lib/` | Composite GitHub Action: checks out ui-lib at the pinned Cargo.toml tag with the palicanon-ci app token. |
| `.task/` | Task definitions and records: one markdown file per task, its design, log and retro. |

A card is a **term** (one DPD headword) with up to four **facets**: Meaning,
Recall, Forms and Context. A term sits in a Leitner **stage** — Day, Week,
Month, Quarter, Half-year, Year (1, 7, 30, 90, 180, 365 days). Answering
every facet right moves it up; one wrong answer sends it back to Day. New
terms come on demand (`new N`), by rank. The CLI keeps the progress in a JSON
state file; the site keeps it in the browser's `localStorage`.

## The page

| Route | What |
| --- | --- |
| `/` | The home, under the brand (the lotus and PaliLearn name the page): one row with the call to action of its state, Practice ('▶ Practice · N' while cards are due; else a practice ahead, with its batch slider and the next review's date beside). Then the progress: the six stages (Daily to Yearly), each a button that filters the term list in place (pressed again: every term; no URL, the choice is lost on reload), the list (about five rows, scrolling; a term opens its detail) with 'New terms' (a '+') at its bottom right, sticky while the list scrolls, then the terms the deck does not know. 'New terms' and its picker, in a dialog (+5, +10 or a custom number, then Add starts the session), are the way in; with no term started the '+' sits at the right where the list would be. Export and import live in Settings ('Your progress'). A progress this build cannot read is shown as a banner, never overwritten. |

The review runs in place: a call to action swaps the home's content for a
session (one card at a time: the facet, reveal, "Knew it" / "Didn't
know"; saved after every answer; practice cards; the summary at the end;
paused when another tab changes the progress), and the stop dialog or
'Back home' swaps it back. No route, no history entry: the browser's Back
leaves the site. The footer's legal pages are the main site's
(https://palicanon.app/terms, https://palicanon.app/privacy).

The first paint is rendered by the server as a skeleton; the cards and the
counts appear once the browser has loaded the progress.

### The progress store

The progress lives in the browser only, under the `localStorage` key
`pc_learn` (the learning loop's JSON state, versioned). **Not synced across
devices yet**: Settings' 'Your progress' exports it to a file
(`pc_learn-<YYYY-MM-DD>.json`) and imports one back. An import is validated
whole and replaces the progress in that browser; a refused file (not a state,
saved by a newer version, the same term twice) leaves it unchanged. A state
saved by a newer version is read-only and never overwritten.

### The embedded deck

The site teaches the deck `core/fixtures/deck.json`, embedded in the server
binary and the wasm with `include_str!` and parsed once on first use: the
page fetches nothing. The deck is part of the build, so changing it is a new
build of the site.

## Try the loop at the terminal

```sh
export RUSTUP_HOME=$HOME/.rustup-pc/rustup CARGO_HOME=$HOME/.rustup-pc/cargo PATH=$HOME/.rustup-pc/cargo/bin:$PATH RUSTC_WRAPPER=sccache
# Start five terms (the state file is created on the first write):
cargo run -p learn_cli -- --deck core/fixtures/deck.json --state /tmp/state.json new 5
# Review what is due: Enter shows the answer, then y or n. The state is saved
# after every answer; closing the terminal loses nothing.
cargo run -p learn_cli -- --deck core/fixtures/deck.json --state /tmp/state.json review
# Terms per stage, what is due, what is new, lapses:
cargo run -p learn_cli -- --deck core/fixtures/deck.json --state /tmp/state.json stats
# Simulate a week later (the date is an input; the default is today, UTC):
cargo run -p learn_cli -- --deck core/fixtures/deck.json --state /tmp/state.json --today 2026-10-13 review
```

A session asks its questions in a random order. `--seed N` fixes it (the
same seed, the same order; the default comes from the clock, so each run
differs).

Two more commands: `new N` starts N new terms (it only starts them; ask for
them with `review`), and `advance K` brings K long-stage terms (Month or
later) forward after a pause — refused, with the reason, while anything is
due or new. A date before the state's last day is refused. A state file that
cannot be read is never overwritten. `learn-cli --help` lists the options.

## Rebuild the fixture deck

`core/fixtures/deck.json` is generated, not written. It needs the DPD
SQLite database (`dpd.db`, release v0.4.20260926; here, the one of
`pali-embedder`):

```sh
cargo run -p build_deck -- ../pali-embedder/data/dpd.db --terms core/fixtures/terms.txt -o core/fixtures/deck.json --strict
# 50 terms, 41 with forms, 50 with a blank, 0 warnings, 0 notes, empty cells: 3
```

That build makes no call and its examples carry no translation. The
committed deck adds each example's translations (en, fr, vi) from the
PaliCanon API with `--api`, the token read from the keychain into the process
(`curl` gets it on its standard input; nothing prints it); `--cache` keeps the
API's answers, so a rebuild fetches nothing twice:

```sh
PALI_API_TOKEN="$(security find-generic-password -a pali-canon-api -s pali-api-token-prod-site -w)" \
  cargo run -p build_deck -- ../pali-embedder/data/dpd.db --terms core/fixtures/terms.txt \
  -o core/fixtures/deck.json --strict --api https://api.palicanon.app --cache <a folder outside git>
# 50 terms, 41 with forms, 50 with a blank, 0 warnings, 66 notes, empty cells: 3
# translations: en 49, fr 4, vi 9
```

The notes list each example or language left out and why (a text keyed by
paragraph, an example not found in its sutta).

`--strict` turns the parser's warnings into a failure; the output is
byte-for-byte the same on every run (`cmp` against the committed file). The 9
indeclinables have no forms; the 3 empty cells are padding in the DPD's own
inflection tables.

## Data

`core/fixtures/deck.json` is derived from the
[Digital Pāḷi Dictionary v0.4.20260926](https://digitalpalidictionary.github.io/)
and is licensed **CC BY-NC-SA 4.0** (attribution, non-commercial,
share-alike): that licence applies to the deck data and to anything built
from it. The code in this repository is **MIT OR Apache-2.0**. The deck's
`source` field repeats the line (the site does not display it yet).

## Run the site

```sh
# Toolchain with the wasm32 target, isolated from the system Rust:
export RUSTUP_HOME=$HOME/.rustup-pc/rustup CARGO_HOME=$HOME/.rustup-pc/cargo PATH=$HOME/.rustup-pc/cargo/bin:$PATH RUSTC_WRAPPER=sccache
# Site on http://127.0.0.1:3201, live reload on 4201 (set in Cargo.toml):
cargo leptos watch
```

A local run is open: `ORIGIN_SECRET` unset means no check, and the boot line
says `origin: open (local)`. A deployed build (Cloud Run's `K_SERVICE` set, or
`LEPTOS_ENV` prod, as the image sets it) needs `ORIGIN_SECRET` and refuses to
start without it; a blank value is an error even locally. It then answers only
the requests that carry that value in `x-origin-auth` (the Cloudflare Worker
adds it), with a 403 and an empty body for every other one (`app/src/origin.rs`).
The secret is in no log line and no message.

It depends on `palicanon-ui-lib` by git (`branch = "main"`), with a local
`[patch]` to `../ui-lib` (a worktree reaches it through the `wt/ui-lib`
symlink) — a change spanning both repositories is seen at once. `ui-lib`'s
`style/theme.css` and `build/placeholders.rs` are read from that sibling
checkout too.

## Checks

```sh
export RUSTUP_HOME=$HOME/.rustup-pc/rustup CARGO_HOME=$HOME/.rustup-pc/cargo PATH=$HOME/.rustup-pc/cargo/bin:$PATH RUSTC_WRAPPER=sccache
cargo fmt --all --check
cargo clippy -p learn_pali --features ssr --all-targets -- -D warnings
cargo clippy -p learn_pali --lib --features hydrate --target wasm32-unknown-unknown -- -D warnings
cargo clippy -p learn_core -p learn_cli -p build_deck --all-targets -- -D warnings
cargo clippy -p learn_core --lib --target wasm32-unknown-unknown -- -D warnings
cargo test -p learn_pali --features ssr
cargo test -p learn_core -p learn_cli -p build_deck
RUSTDOCFLAGS="-D warnings" cargo doc --no-deps -p learn_pali --features ssr -p learn_core -p learn_cli -p build_deck
cargo leptos build
cargo deny check advisories bans sources licenses
```

The same run in CI (`.github/workflows/ci.yml`), plus MSRV (`--locked`, on
1.88), cargo-deny and coverage (cargo-llvm-cov over the four crates →
Codecov). The app has `ssr` and `hydrate` features that do not build
together, so there is no `--workspace`; the three other crates have none.
Functions stay under 50 lines (`clippy.toml`).

## Deploy

Written, **not run yet** (task 021): nothing below has been applied, built in
CI or pushed.

| Where | What | Host |
| --- | --- | --- |
| dev | a push to `main` | `learn.dev.palicanon.app` (Cloud Run `learn-pali-dev`) |
| prod | a tag `v*` | `learn.palicanon.app` (Cloud Run `learn-pali-prod`, once added) |

- `app/Dockerfile` builds the server and the site in one image (`cargo leptos
  build --release`, then `debian:bookworm-slim`, a non-root user, port 8080).
  Its build context is the folder holding `learn-pali/` **and** `ui-lib/`, and
  takes no credential (ui-lib is COPYed): the command is in the file's header.
  It ends with a smoke test: the server starts and answers.
- `infra/` is the Cloud Run service, its service account and the Cloudflare
  Worker and route per environment, in their own tofu state; see
  `infra/README.md`.
- `.github/workflows/deploy.yml` computes the image tag `h-<hash>` (the tracked
  `app`, `core`, `Cargo.toml`, `Cargo.lock` and the ui-lib commit), builds it
  only when the registry lacks it, deploys it with no traffic under a tag,
  checks it, then moves the traffic. The checks: `/` answers 200 and names
  PaliLearn; the hashed `.wasm` that `/` references answers 200; the retired
  review route answers 404.

Before the first deploy, the user:

1. Runs the first `tofu apply` from `infra/` (`infra/README.md`: gcloud
   application-default login, `CLOUDFLARE_API_TOKEN` from the keychain). It
   creates **dev only**: the service on a placeholder image, its Worker and
   route. Prod comes later (step 4).
2. The repository **variables** `WIF_PROVIDER`, `DEPLOYER_SA` and
   `GCP_PROJECT` are set (2026-10-09, the API's values, read from
   `PaliCanon/api`); the secrets `APP_ID` and `APP_PRIVATE_KEY` (the GitHub App
   `palicanon-ci`, which checks ui-lib out) are what CI and the deploy need.
   The deployer holds `roles/run.developer` and Artifact Registry writer
   (checked). The repository is allowed to use the deploy identity (the site's
   `github_repos`, applied 2026-10-09; `infra/README.md`).
3. Decides the open points: the host names, the maximum instances. Decided
   (034): dev is public behind the origin guard, like search-dev; Cloudflare
   Access applies only to an env that sets `access` in `infra/`. Also known:
   the Cloud Run origin answers only the Worker (403 without its secret
   header, task 031); the one exception is the bootstrap hello the first
   `tofu apply` leaves, served before the first deploy.
4. Pushes `main` (dev), checks `https://learn.dev.palicanon.app`, then adds
   prod (a second `tofu apply`, prod is in variables.tf's default since 036) and tags `v0.x.y`.
