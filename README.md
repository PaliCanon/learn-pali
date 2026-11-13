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
| `app/src/` | The site's frame (app.rs: the document, the rail, the footer in app/site_footer.rs), entry point (main.rs), origin guard (origin.rs); the Learn fragment (fragment.rs: LearnRoutes, LearnProvider, LearnSettings), the host's plug-ins (backend.rs: the ProgressStore and FileTransfer traits; web.rs: the site's localStorage store and file transfer), the progress decisions (store.rs: Load, classify, validate_import), embedded deck (deck.rs), the UI strings' catalogue (catalogue.rs), the seed of a session's random order (seed.rs), the release the footer badge shows (release.rs: `ReleaseInfo`, read from `LEARN_PALI_ENV` / `LEARN_PALI_SHA` at start, written as the metas `learn-release-env` / `learn-release-sha` in the SSR head, read back by the hydrate entry). |
| `app/src/pages/` | Page views, the home at `/`. |
| `app/src/components/` | UI components: progress blocks, the 'not saved' strip, new terms picker, review session, term list, the SuttaCentral badge of a passage (passage_badge.rs), 'Add to practice' under each entry of a passage's word panel (add_word.rs). |
| `app/src/learn/` | Learning session logic: home state, session run, Leitner stages, card summary, word lookups, the Context front's window around the blank, which translation shows. |
| `app/locales/` | UI translations: en.json, fr.json, vi.json. |
| `app/style/` | Stylesheet (main.css), imports ui-lib's theme. |
| `app/public/` | Static assets: favicon and logo. |
| `app/tests/` | Integration tests: asset load, origin guard, the grep gates of the feature names (cfg_gates.rs) and of the build (no_build_git.rs: the image reads no git), the release read from the document, the fragment, the sync's browser triggers and the hint in a real browser (release_document.rs, csr.rs, sync_triggers.rs, sync_hint.rs; wasm32 only). |
| `app/Dockerfile` | Container image build: cargo leptos build, debian runtime, non-root user, port 8080. |
| `core/` | Crate `learn_core`, the learning loop library. `serde` only; builds for native and wasm32. |
| `core/src/` | Learning modules: deck, Leitner stages, session, progress state, facets, word forms, shuffle, stats, date math, the SuttaCentral link of a passage by language (suttacentral.rs). |
| `core/fixtures/` | Test fixtures: 50 terms (terms.txt), the built deck (deck.json) and the checked SuttaCentral uids and translators of its sources (suttacentral.tsv). |
| `core/tests/` | Integration tests. |
| `cli/` | Crate `learn_cli`, bin `learn-cli`, the learning loop at the terminal. Commands: review, stats, new N, advance K. |
| `cli/src/` | CLI code: argument parsing, terminal UI, session runner, stats printer, progress store. |
| `cli/tests/` | CLI tests. |
| `tools/build-deck/` | Crate `build_deck`, bin `build-deck`, builds the deck JSON from the Digital Pāḷi Dictionary. Never part of the site's build. |
| `tools/build-deck/src/` | Deck build: DPD parsing, word inflection, form lookup, translation via API, example segment alignment, source tracking. |
| `tools/build-deck/fixtures/` | Test fixtures for build and translation. |
| `infra/` | OpenTofu: the API token secret in Secret Manager (read by the server) and the Drive API switch. Cloud Run is gone. |
| `infra/scripts/` | The API token script and its test (api-token.test.sh). |
| `scripts/` | Repository scripts: the release values the deploy summary prints (release-values.sh), the passages fetch (fetch-passages.sh), and their tests, plus the tests of the image (image.test.sh) and of deploy.yml (deploy-workflow.test.sh). |
| `.github/workflows/` | CI: format, lint, test, doc, MSRV, coverage, deny checks (ci.yml). Deploy: a tag builds the image and pushes it to GHCR (deploy.yml). |
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
| `/` | The home, under the brand (the lotus and PaliLearn name the page): one row with the call to action of its state, Practice ('▶ Practice · N' while cards are due; opens a popup to choose the number of cards/questions, default 10, or All; otherwise a practice ahead, with its batch slider and the next review's date beside). Short reviews save every answer and leave unanswered cards due for another session; the summary shows the remaining due count. Then the progress: the six stages (Daily to Yearly), each a button that filters the term list in place (pressed again: every term; no URL, the choice is lost on reload), the list (about five rows, scrolling; a term opens its detail) with 'New terms' (a '+') at its bottom right, sticky while the list scrolls, then the terms the deck does not know. 'New terms' and its picker, in a dialog (+5, +10 or a custom number, then Add starts the session), are the way in; with no term started the '+' sits at the right where the list would be. Export and import live in Settings ('Your progress'). A progress this build cannot read is shown as a banner, never overwritten. |

The review runs in place: a call to action swaps the home's content for a
session (one card at a time: the facet, reveal, "Knew it" / "Didn't
know"; saved after every answer; practice cards; the summary at the end;
paused when another tab changes the progress), and the stop dialog or
'Back home' swaps it back. No route, no history entry: the browser's Back
leaves the site. The footer's legal pages are the main site's
(https://palicanon.app/terms, https://palicanon.app/privacy).

The first paint is rendered by the server as a skeleton; the cards and the
counts appear once the browser has loaded the progress.

### The Learn fragment (the PaliCanon app)

Learn also runs inside the PaliCanon app (Tauri, client side, offline), which
mounts it under `/learn` in its own shell with Search and the Map. The crate
exposes it in `learn_pali::fragment`: `LearnRoutes` (one relative route, the
home, mounted by the parent: the site at the root, the app in a `ParentRoute`
at `/learn`; the brand link is built from `use_matched()` of that route),
`LearnProvider` (given the host's `ProgressStore` and `FileTransfer`, both
required; one per page, above the `Router`) and `LearnSettings` ('Your
progress' for the host's Settings). No shell, rail, footer or title is in it;
the rustdoc of the module lists what it needs from its host.

Three build modes, by feature of the `learn_pali` crate:

| Mode | Features | Who |
| --- | --- | --- |
| server | `ssr` | the site's server (renders the first paint) |
| hydrate | `hydrate` (implies `web-store`) | the site's browser build |
| csr | `csr` alone | the app: no server, no `localStorage` store, the host plugs its own |

`browser` (the DOM glue) is private to the two client modes; `web-store`
holds the `localStorage` store and the Blob / file input transfer
(`web_store()`, `web_files()`, `src/web.rs`). The file input is created on
demand when Import is pressed and removed after the change or the cancel event,
or by the next pick at the latest (which also makes the earlier chooser inert);
the page's markup holds none. A chooser that cannot be made, or a file that
cannot be read, shows its own message. `scripts/csr-without-web-store.sh`
and `app/tests/cfg_gates.rs` keep `web-store` out of a csr build.

### The progress store

On the site the progress lives in the browser only, under the `localStorage`
key `pc_learn` (the learning loop's JSON state, versioned); in the app it is
the host's `ProgressStore` (a file), which says through a status signal when
it is not saved (the page then shows one strip, 'Your progress is not saved',
with Export) and answers `Load::Unreadable` when it could not read it (an
error and a Reload; nothing is ever written over it). **Not synced across
devices yet**: Settings' 'Your progress' exports it to a file
(`pc_learn-<YYYY-MM-DD>.json`) and imports one back. An import is validated
whole and replaces the progress in that browser; a refused file (not a state,
saved by a newer version, the same term twice) leaves it unchanged. A state
saved by a newer version is read-only and never overwritten.

### The progress Book and the selected deck

The progress file is a `Book` (`core/src/book.rs`): one State body per deck
id (`{version: 3, decks: {words: {...}, <id>: {...}}}`). It is written in the
flat format every build reads (version 1 or 2, the `words` deck's State alone)
as long as every other deck holds no progress and no added word, and as
version 3 once one does; a flat file reads as `{words: it}`. A build that
reads up to version 2 refuses a version-3 file and never overwrites it. The
host's `ProgressStore` holds the Book; the `Backend` maps it to the selected
deck's State, so the screens and the session work on one State. The selected
deck is a device setting (`localStorage[pc_learn_deck]`, not in the Book, not
synced; an unknown id falls back to `words`). The home shows a deck chooser
only when more than one deck is registered (`app/src/deck.rs`: today one).
Settings' export writes the whole Book; an import replaces every deck. The
sync (`app/src/sync.rs`) joins the two Books deck by deck. The CLI acts on one
entry with `--deck-id` (default `words`).

### The embedded deck

The site teaches the deck `core/fixtures/deck.json`, embedded in the server
binary and the wasm with `include_str!` and parsed once on first use: the
page fetches nothing. The deck is part of the build, so changing it is a new
build of the site.

A second deck, the Devanāgarī alphabet for Pāli (`core/fixtures/devanagari.json`,
063b), is embedded the same way and offered by the home's deck chooser. It is a
`script` deck (two facets: read the sign, write it) built from the reviewed
inventory table `tools/build-deck/devanagari.tsv` (8 vowels, 32 consonants and
the niggahīta, 7 vowel signs on क, the 40 conjuncts most frequent in the Pāli
of 20 suttas served by the API):

```sh
cargo run -p build_deck -- devanagari --inventory tools/build-deck/devanagari.tsv -o core/fixtures/devanagari.json
```

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

`--deck-id ID` (default `words`) names the deck of the state file the
commands act on: the file is a Book with one entry per deck, and `--deck`
stays the deck file. A file with only `words` progress stays in the flat
format every build reads; a file with progress in another deck is stamp 3,
which an older build refuses and never overwrites.

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

`ORIGIN_SECRET` is optional, including when `LEPTOS_ENV=PROD`: absent means no
origin check and startup logs `origin.checked: false`; a value enables the
guard. Prod runs on the user's server behind a private Cloudflare tunnel: it
sets neither `K_SERVICE` nor `ORIGIN_SECRET`, and sets
`LEARN_PALI_DEPLOYMENT_ENV=production` for production log metadata. Dead since
Cloud Run left (task 083), kept: the branch that requires a non-blank secret
when `K_SERVICE` is present (even empty), and the `-prod` / `-dev` suffix
reading below. A configured blank secret is invalid everywhere.

It depends on `palicanon-ui-lib` by git (`branch = "main"`), with a local
`[patch]` to `../ui-lib` (a worktree reaches it through the `wt/ui-lib`
symlink) — a change spanning both repositories is seen at once. `ui-lib`'s
`style/theme.css` and `build/placeholders.rs` are read from that sibling
checkout too.

## Operational logs

The site emits newline-delimited JSON through `tracing`: stdout on the
server, the browser's console for the hydrated site. No telemetry backend,
credentials or network connection are required. The container runtime captures stdout
as structured logs. Browser diagnostics stay in that browser;
they are not uploaded or persisted. The CSR fragment emits tracing events
for its host's subscriber; it does not install one automatically.

Every application event includes `event.name`, `severity`, a UTC timestamp,
`service.namespace: palicanon`, `service.name: learn`, `runtime` (server or
browser), `service.version` and `service.commit`. **The version is the
deployed service's tag**, e.g. `v0.1.20`, from `LEARN_PALI_ENV`, the same
release the footer shows. A branch deploy uses its branch name and separate
short commit; a local run uses `dev`. It is not the Cargo package version.
Hydration reads the release metas written by the server, so both halves
report the same tag and commit.

`LEARN_PALI_DEPLOYMENT_ENV` sets the deployment environment independently
of the release (e.g. `production`, `development`, `staging`). Without it,
`K_SERVICE` ending in `-prod` / `-dev` supplies production / development;
an unrecognized deployed service uses `unknown`, a local server development.
The server passes this value to hydration in `learn-deployment-environment`.

The default level is INFO. Set `LEARN_PALI_LOG_LEVEL=debug` on the server
for API attempt timings, retries and passage results. Only the exact
application tracing target `learn` is collected; third-party libraries
cannot print request URLs or credentials through these sinks.

| Events | Level and contents |
| --- | --- |
| `service.started`, `service.stopped`, `service.failed` | INFO / ERROR; startup duration, origin guard state, passages asset availability, failure stage. SIGTERM / Ctrl-C drains requests before the stop event. |
| `http.request.completed` | INFO; method, route template, status and time to response headers (streamed body time excluded). 5xx are ERROR; HEAD and successful static/fallback requests DEBUG. The span contains a server-generated request id; incoming ids are ignored. |
| `origin.refused` | WARNING; a fixed refusal reason, at most once per minute per process. No request data. |
| `api.config.invalid`, `api.disabled`, `api.failed` | Configuration state and bounded failure categories; authentication refusal ERROR, other failures WARNING. |
| `api.request.completed`, `api.retry`, `passage.completed`, `passage.asset.fallback` | DEBUG; operation, outcome, attempt timing, retry or absence reason. |
| `passage.failed`, `passage.asset.failed` | WARNING / ERROR; timeout, failed task or unreadable asset. |
| `progress.load.*`, `progress.save.*` | Read/write failure, protected newer format, and recovery; emitted on state changes. Successful writes are silent. |
| `progress.import.*`, `progress.export.requested` | Import refusal or file-transfer handoff. A requested transfer is not a confirmed disk write. |
| `sync.completed`, `sync.failed`, `sync.sign_in.failed`, `sync.authorization.lost`, `sync.timer.failed` | INFO / WARNING; duration, phase and bounded outcome/error category. Offline and voluntary cancellation are DEBUG. |
| `browser.started`, `runtime.panic` | INFO / ERROR; runtime lifecycle. Panic payloads are suppressed. |

Application logs contain no headers, cookies, tokens, email, Drive file ids, query
strings, words, progress contents or raw error messages. HTTP routes are
normalized, including unknown paths. There are no per-answer events.

These are structured logs with OpenTelemetry resource naming, ready for
a future collection pipeline. This change installs no OTLP exporter, trace
provider, metrics backend or Grafana dashboard. An HTTP request id is a
local correlation id, not an OpenTelemetry trace id.

## Checks

```sh
export RUSTUP_HOME=$HOME/.rustup-pc/rustup CARGO_HOME=$HOME/.rustup-pc/cargo PATH=$HOME/.rustup-pc/cargo/bin:$PATH RUSTC_WRAPPER=sccache
cargo fmt --all --check
cargo clippy -p learn_pali --features ssr --all-targets -- -D warnings
cargo clippy -p learn_pali --lib --features hydrate --target wasm32-unknown-unknown -- -D warnings
cargo clippy -p learn_pali --lib --features csr --target wasm32-unknown-unknown -- -D warnings
cargo clippy -p learn_core -p learn_cli -p build_deck --all-targets -- -D warnings
cargo clippy -p learn_core --lib --target wasm32-unknown-unknown -- -D warnings
cargo test -p learn_pali --features ssr
cargo test -p learn_core -p learn_cli -p build_deck
# The fragment in the app's mode, in a headless Chrome (wasm-bindgen-cli at
# Cargo.lock's wasm-bindgen version, a chromedriver matching Chrome):
CARGO_TARGET_WASM32_UNKNOWN_UNKNOWN_RUNNER=wasm-bindgen-test-runner CHROMEDRIVER=<chromedriver> \
  cargo test -p learn_pali --features csr --target wasm32-unknown-unknown --test csr
# The site's Google Drive sync against a fake Google, same runner (task 050):
CARGO_TARGET_WASM32_UNKNOWN_UNKNOWN_RUNNER=wasm-bindgen-test-runner CHROMEDRIVER=<chromedriver> \
  cargo test -p learn_pali --features csr,web-sync --target wasm32-unknown-unknown --test web_sync
# The release read from the document's metas, same runner (task 060):
CARGO_TARGET_WASM32_UNKNOWN_UNKNOWN_RUNNER=wasm-bindgen-test-runner CHROMEDRIVER=<chromedriver> \
  cargo test -p learn_pali --features csr --target wasm32-unknown-unknown --test release_document
# The sync's browser triggers, the real listeners and the tick driven by
# dispatched events: the open, visibilitychange, focus and online ask, a hidden
# page asks nothing and gets no tick, a second install adds no listener (task 059):
CARGO_TARGET_WASM32_UNKNOWN_UNKNOWN_RUNNER=wasm-bindgen-test-runner CHROMEDRIVER=<chromedriver> \
  cargo test -p learn_pali --features csr --target wasm32-unknown-unknown --test sync_triggers
# The one-time sync hint (shown after the first finished session, answered for
# good by a sign-in), in a page of its own (task 059):
CARGO_TARGET_WASM32_UNKNOWN_UNKNOWN_RUNNER=wasm-bindgen-test-runner CHROMEDRIVER=<chromedriver> \
  cargo test -p learn_pali --features csr --target wasm32-unknown-unknown --test sync_hint
bash scripts/csr-without-web-store.sh   # the csr build holds nothing of the web store
bash scripts/release-values.test.sh && bash scripts/deploy-workflow.test.sh && bash scripts/image.test.sh
bash scripts/fetch-passages.test.sh && bash infra/scripts/api-token.test.sh
RUSTDOCFLAGS="-D warnings" cargo doc --no-deps -p learn_pali --features ssr -p learn_core -p learn_cli -p build_deck
cargo leptos build
cargo deny check advisories bans sources licenses
```

The same run in CI (`.github/workflows/ci.yml`), plus MSRV (`--locked`, on
1.88), cargo-deny and coverage (cargo-llvm-cov over the four crates →
Codecov). The app has `ssr`, `hydrate` and `csr` features that do not build
together, so there is no `--workspace`; the three other crates have none.
Functions stay under 50 lines (`clippy.toml`).

## Deploy

Prod runs on the user's server ssh.lsh.tech (repo `Lsh0x/lsh-tech`,
`stacks/palicanon/prod`), behind a Cloudflare tunnel, at
`learn.palicanon.app`. There is no hosted environment to try a change before a
release: run it locally (above) and rely on CI. Cloud Run is gone (task 083).

A tag `v*` builds the image and pushes it to GHCR; nothing else happens. The
stack pulls the image **by digest**, so updating prod is a change of the digest
in the stack, by hand.

- `app/Dockerfile` builds the server and the site in one image (`cargo leptos
  build --release`, then `debian:bookworm-slim`, a non-root user, port 8080).
  Its build context is the folder holding `learn-pali/` **and** `ui-lib/`, and
  takes no credential (ui-lib is COPYed): the command is in the file's header.
  It ends with a smoke test: the server starts and answers.
- `.github/workflows/deploy.yml` computes the image tag `h-<hash>` (the tracked
  `app`, `core`, `Cargo.toml`, `Cargo.lock` and the ui-lib commit), builds it
  only when GHCR lacks it (`docker buildx imagetools inspect`), pushes
  `ghcr.io/palicanon/learn-pali:h-<hash>` and the release tag
  (`ghcr.io/palicanon/learn-pali:v0.1.8`), labelled
  `org.opencontainers.image.source` so the package stays linked to the
  repository. The job summary has the digest
  (`ghcr.io/palicanon/learn-pali@sha256:…`), the tags and the release values.
  The login is the job's `GITHUB_TOKEN` (`packages: write`). GCP is still used
  for one read: the passages asset in `gs://palicanon-app-assets`
  (`scripts/fetch-passages.sh`, with the repository variables `WIF_PROVIDER`
  and `DEPLOYER_SA`).
- `infra/` holds the API token secret the server reads; see `infra/README.md`.

The footer badge is the release the stack gives the container, not the image's:
the image is identical across refs (`app/tests/no_build_git.rs`: the build
reads no git). The server reads `LEARN_PALI_ENV` (the tag, e.g. `v0.1.8`) and
`LEARN_PALI_SHA` (empty for a tag); the deploy summary prints both
(`scripts/release-values.sh`). A local run shows `dev`; a page whose metas are
missing shows `unknown` in the browser.

Each release, the user: tags `v0.x.y`, reads the digest from the job summary and
puts it, with the release values, in the lsh-tech stack.
