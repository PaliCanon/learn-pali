//! Add a word by search (062): the dictionary entries and a found word's
//! line from the suttas, fetched by the server from the PaliCanon API.
//!
//! Two server functions. [`search_words`] asks `GET /v1/dictionary?q=&n=10`
//! (the full DPD headwords by prefix) and answers the entries, or
//! [`SearchAnswer::Unreachable`] when the API cannot be reached: an empty
//! list is a plain answer, never an error. [`passage_for`] runs the deck's
//! own method on one entry (`learn_passage`: the DPD source mapped to a
//! SuttaCentral uid, the example matched in the sutta's Pāli, each language's
//! line) and answers [`PassageAnswer::Found`] with the example and its
//! translations, a [`PassageAnswer::Miss`] (the word learns without its
//! line) or [`PassageAnswer::Transient`] (nothing is added; try again).
//!
//! The token lives on the server only: `LEARN_PALI_API_URL` and
//! `LEARN_PALI_API_TOKEN` are read once from the environment, both set or
//! neither (then every search is Unreachable, with one log line); the
//! browser never sees it and no answer or log line carries it. Every call
//! has a 5 s timeout (a scale-to-zero API answers its first call in a few
//! seconds: 2 s made the first lookup after idle fail, 065); a search
//! retries once, silently, on a connection
//! failure, a 503 or a timeout (never on 401 or 429); the whole passage has
//! 8 s, after which it is Transient (a slow day must not store a word
//! without its line for good). The calls are synchronous (`ureq`, the
//! `learn_passage::fetch::Fetch` trait) and run in a blocking task.

use learn_core::deck::Example;
use leptos::prelude::*;
use leptos::server_fn::codec::Json;
use serde::{Deserialize, Serialize};

/// One dictionary entry, as the API's `/v1/dictionary` serves it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Entry {
    /// The word as written (`dhamma`), shown to the learner.
    pub headword: String,
    /// The DPD lemma with its homonym number (`dhamma 1.01`): the entry's
    /// identity, and the term's id when added.
    pub lemma: String,
    /// The DPD part of speech.
    pub pos: String,
    /// The English meaning.
    pub meaning: String,
    /// The DPD's first example, when it has one.
    #[serde(default)]
    pub example: Option<String>,
    /// The example's source (`SN22.87`).
    #[serde(default)]
    pub source: Option<String>,
    /// The example's sutta name.
    #[serde(default)]
    pub sutta: Option<String>,
}

/// What a search answers.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum SearchAnswer {
    /// The matching entries, at most ten; empty when nothing matches.
    Entries(Vec<Entry>),
    /// The dictionary cannot be reached right now (a failed call, a
    /// timeout, a refused token: the last one is logged at error level).
    Unreachable,
}

/// Why a found word gets no line from the suttas.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Miss {
    /// The DPD has no example for this headword.
    NoExample,
    /// The example's source maps to no SuttaCentral uid.
    UnmappedSource,
    /// The API does not serve that sutta's text.
    TextNotServed,
    /// The example is not found in the sutta's Pāli.
    NotMatched,
}

/// What the passage answers.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum PassageAnswer {
    /// The example, its source and sutta, and the translations the texts gave.
    Found(Example),
    /// No line: the word is added without its context facet.
    Miss(Miss),
    /// A call failed or the budget ran out: nothing is added.
    Transient,
}

/// One sense of a word looked up by its exact form (065), as the API's
/// `/v1/dpd/{word}` serves it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Sense {
    /// The DPD lemma, with its homonym number.
    pub lemma: String,
    /// The part of speech.
    pub pos: String,
    /// The English meaning.
    pub en: String,
    /// The French meaning, when the DPD has it.
    #[serde(default)]
    pub fr: Option<String>,
}

/// What a lookup answers.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum LookupAnswer {
    /// The senses found; empty when the form is unknown.
    Senses(Vec<Sense>),
    /// The dictionary cannot be reached right now.
    Unreachable,
}

/// The dictionary entries whose headword starts with `q`.
///
/// # Errors
/// Only the transport between the browser and this server.
#[server(prefix = "/api", endpoint = "search_words")]
pub async fn search_words(
    /// The letters typed, at least two.
    q: String,
) -> Result<SearchAnswer, ServerFnError> {
    #[cfg(feature = "ssr")]
    {
        Ok(server::search(q).await)
    }
    // A native build without `ssr` (the app's tests build the fragment for
    // the host): the macro keeps the body there, and no server exists.
    #[cfg(not(feature = "ssr"))]
    {
        let _ = q;
        Ok(SearchAnswer::Unreachable)
    }
}

/// The line from the suttas for `entry`, as the deck would build it. The
/// entry travels as JSON (a struct, not form fields).
///
/// # Errors
/// Only the transport between the browser and this server.
#[server(prefix = "/api", endpoint = "passage_for", input = Json)]
pub async fn passage_for(
    /// The entry whose line is wanted.
    entry: Entry,
) -> Result<PassageAnswer, ServerFnError> {
    #[cfg(feature = "ssr")]
    {
        Ok(server::passage(entry).await)
    }
    // A native build without `ssr` (the app's tests build the fragment for
    // the host): the macro keeps the body there, and no server exists.
    #[cfg(not(feature = "ssr"))]
    {
        let _ = entry;
        Ok(PassageAnswer::Transient)
    }
}

/// The senses of `form`, the word as written in a line (065): the API's
/// exact-form lookup.
///
/// # Errors
/// Only the transport between the browser and this server.
#[server(prefix = "/api", endpoint = "lookup_word")]
pub async fn lookup_word(
    /// The word as written, as the passage cuts it.
    form: String,
) -> Result<LookupAnswer, ServerFnError> {
    #[cfg(feature = "ssr")]
    {
        Ok(server::lookup(form).await)
    }
    // A native build without `ssr` (the app's tests build the fragment for
    // the host): the macro keeps the body there, and no server exists.
    #[cfg(not(feature = "ssr"))]
    {
        let _ = form;
        Ok(LookupAnswer::Unreachable)
    }
}

/// The site's dictionary (068): the server functions, called from the
/// browser (or run in place on the server). A transport failure answers
/// Unreachable or Transient, as the server would. A build with no server
/// (the native csr check) has them answer Unreachable at once.
pub struct ServerDictionary;

impl crate::backend::Dictionary for ServerDictionary {
    fn search(&self, q: String, on_done: Box<dyn FnOnce(SearchAnswer)>) {
        leptos::task::spawn_local(async move {
            on_done(search_words(q).await.unwrap_or(SearchAnswer::Unreachable));
        });
    }

    fn passage(&self, entry: Entry, on_done: Box<dyn FnOnce(PassageAnswer)>) {
        leptos::task::spawn_local(async move {
            on_done(passage_for(entry).await.unwrap_or(PassageAnswer::Transient));
        });
    }

    fn lookup(&self, form: String, on_done: Box<dyn FnOnce(LookupAnswer)>) {
        leptos::task::spawn_local(async move {
            on_done(lookup_word(form).await.unwrap_or(LookupAnswer::Unreachable));
        });
    }
}

/// The site's dictionary as a shared handle, for its `LearnProvider`.
#[must_use]
pub fn web_dictionary() -> crate::backend::SharedDictionary {
    std::rc::Rc::new(ServerDictionary)
}

#[cfg(feature = "ssr")]
pub mod server {
    //! The server side: the API configuration, the `ureq` client behind
    //! `learn_passage`'s `Fetch`, and the two calls.

    use std::sync::{Mutex, OnceLock};

    use learn_passage::asset::Asset;
    use std::time::Duration;

    use learn_core::deck::Example;
    use learn_passage::blank::blank_example;
    use learn_passage::fetch::{Fetch, FetchError};
    use learn_passage::translate::{Why, translate_example};

    use super::{Entry, LookupAnswer, Miss, PassageAnswer, SearchAnswer, Sense};

    /// The variable holding the API's base URL.
    pub const URL_VAR: &str = "LEARN_PALI_API_URL";
    /// The variable holding the API's token.
    pub const TOKEN_VAR: &str = "LEARN_PALI_API_TOKEN";
    /// One call's timeout: wide enough for a cold API's first answer.
    pub const CALL_TIMEOUT: Duration = Duration::from_secs(5);
    /// The whole passage's budget.
    pub const PASSAGE_BUDGET: Duration = Duration::from_secs(8);
    const N: usize = 10;

    /// Where the API is and how to be let in. `Debug` shows no token.
    #[derive(Clone)]
    pub struct ApiConfig {
        /// The base URL, no trailing slash.
        pub base: String,
        /// The bearer token.
        pub token: String,
    }

    impl std::fmt::Debug for ApiConfig {
        fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            f.debug_struct("ApiConfig")
                .field("base", &self.base)
                .finish_non_exhaustive()
        }
    }

    impl ApiConfig {
        /// The configuration `get` gives: both variables set and non-blank,
        /// or none (`None`); one alone is `None` too, with its name logged.
        pub fn read(get: impl Fn(&str) -> Option<String>) -> Option<ApiConfig> {
            let clean = |name: &str| {
                get(name)
                    .map(|v| v.trim().to_string())
                    .filter(|v| !v.is_empty())
            };
            match (clean(URL_VAR), clean(TOKEN_VAR)) {
                (Some(base), Some(token)) => Some(ApiConfig {
                    base: base.trim_end_matches('/').to_string(),
                    token,
                }),
                (None, None) => None,
                (Some(_), None) => {
                    eprintln!(
                        "learn-pali: dictionary: {URL_VAR} is set but {TOKEN_VAR} is not: off"
                    );
                    None
                }
                (None, Some(_)) => {
                    eprintln!(
                        "learn-pali: dictionary: {TOKEN_VAR} is set but {URL_VAR} is not: off"
                    );
                    None
                }
            }
        }
    }

    fn config() -> Option<&'static ApiConfig> {
        static CONFIG: OnceLock<Option<ApiConfig>> = OnceLock::new();
        CONFIG
            .get_or_init(|| {
                let config = ApiConfig::read(|name| std::env::var(name).ok());
                if config.is_none() {
                    eprintln!("learn-pali: dictionary: off ({URL_VAR} and {TOKEN_VAR} unset)");
                }
                config
            })
            .as_ref()
    }

    /// The `ureq` client as `learn_passage`'s `Fetch`: a 404 is `None`, any
    /// other status an error, the token in the header and nowhere else.
    pub struct UreqFetch {
        agent: ureq::Agent,
        config: ApiConfig,
    }

    impl UreqFetch {
        /// A client for `config`, with the call timeout.
        #[must_use]
        pub fn new(config: ApiConfig) -> UreqFetch {
            let agent = ureq::Agent::new_with_config(
                ureq::Agent::config_builder()
                    .timeout_global(Some(CALL_TIMEOUT))
                    .http_status_as_error(true)
                    .build(),
            );
            UreqFetch { agent, config }
        }
    }

    impl Fetch for UreqFetch {
        fn get(&mut self, path: &str) -> Result<Option<String>, FetchError> {
            let url = format!("{}{path}", self.config.base);
            let bearer = format!("Bearer {}", self.config.token);
            match self.agent.get(&url).header("Authorization", &bearer).call() {
                Ok(mut response) => response.body_mut().read_to_string().map(Some).map_err(|e| {
                    FetchError::Body(path.to_string(), scrub(&e.to_string(), &self.config.token))
                }),
                Err(ureq::Error::StatusCode(404)) => Ok(None),
                Err(ureq::Error::StatusCode(code)) => {
                    Err(FetchError::Status(path.to_string(), code))
                }
                Err(e) => Err(FetchError::Transport(
                    path.to_string(),
                    scrub(&e.to_string(), &self.config.token),
                )),
            }
        }
    }

    /// A message with the token blanked, should a library ever echo it.
    fn scrub(message: &str, token: &str) -> String {
        message.replace(token, "[token]")
    }

    /// The search, through the configured API; `Unreachable` when none.
    pub async fn search(q: String) -> SearchAnswer {
        let Some(config) = config() else {
            return SearchAnswer::Unreachable;
        };
        let config = config.clone();
        tokio::task::spawn_blocking(move || search_in(&config, &q))
            .await
            .unwrap_or(SearchAnswer::Unreachable)
    }

    /// The search against `config`: one silent retry on a connection
    /// failure, a 503 or a timeout; a 400 (q too short or long) is no entry.
    pub fn search_in(config: &ApiConfig, q: &str) -> SearchAnswer {
        let mut fetch = UreqFetch::new(config.clone());
        let path = format!("/v1/dictionary?q={}&n={N}", percent(q));
        let first = fetch.get(&path);
        let result = match &first {
            Err(FetchError::Status(_, 503) | FetchError::Transport(..)) => fetch.get(&path),
            _ => first,
        };
        match result {
            Ok(Some(body)) => parse_entries(&body),
            Ok(None) => {
                eprintln!("learn-pali: dictionary: no_dictionary (404)");
                SearchAnswer::Unreachable
            }
            Err(FetchError::Status(_, 400)) => SearchAnswer::Entries(Vec::new()),
            Err(FetchError::Status(_, code @ (401 | 403))) => {
                eprintln!("learn-pali: dictionary: ERROR the API refused the token (HTTP {code})");
                SearchAnswer::Unreachable
            }
            Err(error) => {
                eprintln!("learn-pali: dictionary: unreachable: {}", kind(&error));
                SearchAnswer::Unreachable
            }
        }
    }

    fn parse_entries(body: &str) -> SearchAnswer {
        #[derive(serde::Deserialize)]
        struct Response {
            items: Vec<Entry>,
        }
        if let Ok(response) = serde_json::from_str::<Response>(body) {
            SearchAnswer::Entries(response.items)
        } else {
            eprintln!("learn-pali: dictionary: unreachable: body");
            SearchAnswer::Unreachable
        }
    }

    /// The error's kind for a log line: never its text (a text can echo a URL).
    fn kind(error: &FetchError) -> String {
        match error {
            FetchError::Status(_, code) => format!("HTTP {code}"),
            FetchError::Transport(..) => "transport".to_string(),
            FetchError::Body(..) => "body".to_string(),
        }
    }

    /// `q` as a query value: the Pāli letters percent-encoded.
    fn percent(q: &str) -> String {
        let mut out = String::new();
        for byte in q.bytes() {
            match byte {
                b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                    out.push(byte as char);
                }
                _ => {
                    use std::fmt::Write as _;
                    let _ = write!(out, "%{byte:02X}");
                }
            }
        }
        out
    }

    /// The lookup, through the configured API; `Unreachable` when none.
    pub async fn lookup(form: String) -> LookupAnswer {
        let Some(config) = config() else {
            return LookupAnswer::Unreachable;
        };
        let config = config.clone();
        tokio::task::spawn_blocking(move || lookup_in(&config, &form))
            .await
            .unwrap_or(LookupAnswer::Unreachable)
    }

    /// The lookup against `config`: `GET /v1/dpd/{form}`; a 404 is no
    /// sense; one silent retry on a connection failure, a 503 or a timeout.
    pub fn lookup_in(config: &ApiConfig, form: &str) -> LookupAnswer {
        let mut fetch = UreqFetch::new(config.clone());
        let path = format!("/v1/dpd/{}", percent(form.trim()));
        let first = fetch.get(&path);
        let result = match &first {
            Err(FetchError::Status(_, 503) | FetchError::Transport(..)) => fetch.get(&path),
            _ => first,
        };
        match result {
            Ok(Some(body)) => parse_senses(&body),
            Ok(None) => LookupAnswer::Senses(Vec::new()),
            Err(FetchError::Status(_, code @ (401 | 403))) => {
                eprintln!("learn-pali: lookup: ERROR the API refused the token (HTTP {code})");
                LookupAnswer::Unreachable
            }
            Err(error) => {
                eprintln!("learn-pali: lookup: unreachable: {}", kind(&error));
                LookupAnswer::Unreachable
            }
        }
    }

    fn parse_senses(body: &str) -> LookupAnswer {
        #[derive(serde::Deserialize)]
        struct Response {
            senses: Vec<Sense>,
        }
        if let Ok(response) = serde_json::from_str::<Response>(body) {
            LookupAnswer::Senses(response.senses)
        } else {
            eprintln!("learn-pali: lookup: unreachable: body");
            LookupAnswer::Unreachable
        }
    }

    /// The passage, through the configured API, within the budget.
    pub async fn passage(entry: Entry) -> PassageAnswer {
        passage_with(passages(), config(), entry).await
    }

    /// [`passage`] over a given asset and config (tests).
    pub async fn passage_with(
        asset: Option<&'static Mutex<Asset>>,
        config: Option<&'static ApiConfig>,
        entry: Entry,
    ) -> PassageAnswer {
        // One deadline for the whole answer (review 070, round 2): what the
        // asset read takes is taken from the live method's budget.
        let started = std::time::Instant::now();
        if let Some(asset) = asset {
            let lemma = entry.lemma.clone();
            let read = tokio::task::spawn_blocking(move || passage_from_asset(asset, &lemma));
            match tokio::time::timeout(PASSAGE_BUDGET, read).await {
                Ok(Ok(Some(answer))) => return answer,
                Ok(Ok(None)) => {}
                Ok(Err(join)) => {
                    eprintln!("learn-pali: passage: the asset read failed ({join}), live");
                }
                Err(_) => eprintln!("learn-pali: passage: the asset read passed the budget, live"),
            }
        }
        let Some(config) = config else {
            return PassageAnswer::Transient;
        };
        let config = config.clone();
        let work = tokio::task::spawn_blocking(move || passage_in(&config, &entry));
        let remaining = PASSAGE_BUDGET.saturating_sub(started.elapsed());
        match tokio::time::timeout(remaining, work).await {
            Ok(Ok(answer)) => answer,
            Ok(Err(_)) => PassageAnswer::Transient,
            Err(_) => {
                eprintln!("learn-pali: passage: transient (budget)");
                PassageAnswer::Transient
            }
        }
    }

    /// The passages asset the server reads first (070): the path of a file
    /// `build-deck passages` wrote, set by the image (`/app/passages.db`).
    pub const PASSAGES_VAR: &str = "LEARN_PALI_PASSAGES";

    /// The passages asset, opened once at startup by [`init_passages`]
    /// (070, review: a broken asset must not pass for a missing row).
    fn passages() -> Option<&'static Mutex<Asset>> {
        PASSAGES.get().and_then(Option::as_ref)
    }

    static PASSAGES: OnceLock<Option<Mutex<Asset>>> = OnceLock::new();

    /// Opens the asset [`PASSAGES_VAR`] names, once, at startup: the server
    /// then reads it first for every passage. No variable: the live method
    /// only (`Ok(None)`).
    ///
    /// # Errors
    /// The variable names a file that does not open, is of another schema
    /// or is not complete: the server must not start on it (the deploy's
    /// rollout check refuses the revision).
    pub fn init_passages(get: impl Fn(&str) -> Option<String>) -> Result<Option<String>, String> {
        let Some(path) = get(PASSAGES_VAR) else {
            PASSAGES.get_or_init(|| None);
            return Ok(None);
        };
        let asset = Asset::open(std::path::Path::new(&path))
            .map_err(|why| format!("{PASSAGES_VAR}={path}: {why}"))?;
        PASSAGES.get_or_init(|| Some(Mutex::new(asset)));
        Ok(Some(path))
    }

    /// The entry's answer from the open asset (070): a row found or missed;
    /// `None` when the asset has no row for the lemma, or asks a retry (a
    /// fetch that failed at build time, a name this reader does not know),
    /// so the live method runs.
    pub fn passage_from_asset(asset: &Mutex<Asset>, lemma: &str) -> Option<PassageAnswer> {
        use learn_passage::asset::{MissName, Row};
        let asset = asset
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let answer = match asset.row(lemma) {
            Ok(Some(Row::Found(example))) => PassageAnswer::Found(example),
            Ok(Some(Row::Miss(name))) => PassageAnswer::Miss(match name {
                MissName::UnmappedSource => Miss::UnmappedSource,
                MissName::TextNotServed | MissName::NoPali => Miss::TextNotServed,
                MissName::NotMatched => Miss::NotMatched,
            }),
            Ok(Some(Row::Retry(name))) => {
                eprintln!(
                    "learn-pali: passage: the asset asks a retry for {lemma:?} ({name}), live"
                );
                return None;
            }
            Ok(None) => {
                eprintln!("learn-pali: passage: no row in the asset for {lemma:?}, live");
                return None;
            }
            Err(why) => {
                eprintln!("learn-pali: passage: ERROR the asset could not be read ({why}), live");
                return None;
            }
        };
        let outcome = match &answer {
            PassageAnswer::Found(_) => "found".to_string(),
            PassageAnswer::Miss(miss) => format!("miss {miss:?}"),
            PassageAnswer::Transient => "transient".to_string(),
        };
        eprintln!("learn-pali: passage: {outcome} (asset)");
        Some(answer)
    }

    /// The passage against `config`: the deck's `translate_example`, its
    /// reasons sorted into a Miss or a Transient (the live method, 062).
    pub fn passage_in(config: &ApiConfig, entry: &Entry) -> PassageAnswer {
        let (Some(text), Some(source)) = (entry.example.as_deref(), entry.source.as_deref()) else {
            return PassageAnswer::Miss(Miss::NoExample);
        };
        // The DPD marks the word in its example with <b>…</b>: the deck's
        // own rule strips the markup and blanks that word.
        let blanked = blank_example(text, &entry.headword, &[]);
        let example = Example {
            text: blanked.text,
            source: source.to_string(),
            sutta: entry.sutta.clone().unwrap_or_default(),
            blank: blanked.blank,
            translations: std::collections::BTreeMap::new(),
        };
        let mut fetch = UreqFetch::new(config.clone());
        let answer = match translate_example(&example, &mut fetch) {
            Ok(found) => PassageAnswer::Found(Example {
                translations: found.translations,
                ..example
            }),
            Err(Why::UnmappedSource(_)) => PassageAnswer::Miss(Miss::UnmappedSource),
            Err(Why::NotServed(_) | Why::NoPali(_)) => PassageAnswer::Miss(Miss::TextNotServed),
            Err(Why::NotMatched(_)) => PassageAnswer::Miss(Miss::NotMatched),
            Err(Why::Fetch(FetchError::Status(_, code @ (401 | 403)))) => {
                eprintln!("learn-pali: passage: ERROR the API refused the token (HTTP {code})");
                PassageAnswer::Transient
            }
            Err(Why::Fetch(error)) => {
                eprintln!("learn-pali: passage: transient: {}", kind(&error));
                PassageAnswer::Transient
            }
        };
        let outcome = match &answer {
            PassageAnswer::Found(_) => "found".to_string(),
            PassageAnswer::Miss(miss) => format!("miss {miss:?}"),
            PassageAnswer::Transient => "transient".to_string(),
        };
        eprintln!("learn-pali: passage: {outcome} (live)");
        answer
    }

    #[cfg(test)]
    mod tests {
        //! Against a fake API on a local port: the token is checked there and
        //! never appears in an answer or a log line.
        fn fixture_asset(name: &str) -> &'static Mutex<Asset> {
            let path =
                std::env::temp_dir().join(format!("pc-passages-{name}-{}.db", std::process::id()));
            let _ = std::fs::remove_file(&path);
            learn_passage::asset::write_fixture(&path);
            Box::leak(Box::new(Mutex::new(Asset::open(&path).unwrap())))
        }

        #[test]
        fn the_asset_answers_found_missed_or_asks_the_live_method() {
            let asset = fixture_asset("rows");
            let Some(PassageAnswer::Found(example)) = passage_from_asset(asset, "dhamma 1.01")
            else {
                panic!("found from the asset")
            };
            assert_eq!(example.source, "SN22.87");
            assert_eq!(example.translations["en"].translator, "sujato");
            assert_eq!(
                passage_from_asset(asset, "vakkali"),
                Some(PassageAnswer::Miss(Miss::NotMatched))
            );
            assert_eq!(
                passage_from_asset(asset, "kho"),
                None,
                "a failed fetch: live"
            );
            assert_eq!(
                passage_from_asset(asset, "ca"),
                None,
                "an unknown name: live"
            );
            assert_eq!(passage_from_asset(asset, "nobody"), None, "no row: live");
        }

        #[tokio::test(flavor = "multi_thread")]
        async fn a_passage_from_the_asset_asks_the_api_nothing_and_a_missing_row_asks_it() {
            // The fake API finds vakkali's line in sn22.87 (the live method
            // answers Found); the asset says not_matched for `vakkali` and
            // has no row for `nobody`: the answers tell which path ran.
            let config: &'static ApiConfig = Box::leak(Box::new(serve(fake()).await));
            let asset = fixture_asset("routing");
            let missed = passage_with(Some(asset), Some(config), vakkali()).await;
            assert_eq!(
                missed,
                PassageAnswer::Miss(Miss::NotMatched),
                "from the asset"
            );
            let mut nobody = vakkali();
            nobody.lemma = "nobody".into();
            let live = passage_with(Some(asset), Some(config), nobody).await;
            assert!(
                matches!(live, PassageAnswer::Found(_)),
                "no row in the asset: the live method found it: {live:?}"
            );
            let none = passage_with(None, Some(config), vakkali()).await;
            assert!(
                matches!(none, PassageAnswer::Found(_)),
                "no asset: live: {none:?}"
            );
        }

        #[test]
        fn a_configured_asset_that_does_not_open_refuses_to_start() {
            let nowhere = std::env::temp_dir().join("pc-passages-nowhere.db");
            let get = |name: &str| (name == PASSAGES_VAR).then(|| nowhere.display().to_string());
            let why = init_passages(get).unwrap_err();
            assert!(why.contains(PASSAGES_VAR) && why.contains("open"), "{why}");
        }

        use std::sync::Arc;
        use std::sync::atomic::{AtomicUsize, Ordering};

        use axum::extract::{Path, Query, State};
        use axum::http::{HeaderMap, StatusCode};
        use axum::routing::get;

        use super::*;

        const TOKEN: &str = "secret-token-xyz";

        #[derive(Clone)]
        struct Fake {
            calls: Arc<AtomicUsize>,
            fail_first: Arc<AtomicUsize>,
            refuse: bool,
        }

        fn authorised(headers: &HeaderMap) -> bool {
            headers
                .get("authorization")
                .and_then(|v| v.to_str().ok())
                .is_some_and(|v| v == format!("Bearer {TOKEN}"))
        }

        async fn dictionary(
            State(fake): State<Fake>,
            headers: HeaderMap,
            Query(q): Query<std::collections::HashMap<String, String>>,
        ) -> (StatusCode, String) {
            fake.calls.fetch_add(1, Ordering::SeqCst);
            if fake.refuse || !authorised(&headers) {
                return (
                    StatusCode::UNAUTHORIZED,
                    r#"{"error":"unauthorized"}"#.into(),
                );
            }
            if fake.fail_first.load(Ordering::SeqCst) > 0 {
                fake.fail_first.fetch_sub(1, Ordering::SeqCst);
                return (StatusCode::SERVICE_UNAVAILABLE, "starting".into());
            }
            let q = q.get("q").cloned().unwrap_or_default();
            if q.chars().count() < 2 {
                return (
                    StatusCode::BAD_REQUEST,
                    r#"{"error":"query_too_short"}"#.into(),
                );
            }
            let body = if q.starts_with("vak") {
                r#"{"items":[{"headword":"vakkali","lemma":"vakkali","pos":"masc","meaning":"name of a monk","example":"alaṃ <b>vakkali</b>","source":"SN22.87","sutta":"Vakkalisutta"},{"headword":"vakka","lemma":"vakka 1","pos":"nt","meaning":"kidney","example":null,"source":null,"sutta":null}]}"#
            } else {
                r#"{"items":[]}"#
            };
            (StatusCode::OK, body.to_string())
        }

        async fn sutta(Path(uid): Path<String>, headers: HeaderMap) -> (StatusCode, String) {
            if !authorised(&headers) {
                return (StatusCode::UNAUTHORIZED, String::new());
            }
            if uid == "sn22.87" {
                (
                    StatusCode::OK,
                    r#"{"texts":[{"lang":"en","translator":"sujato"}]}"#.into(),
                )
            } else {
                (StatusCode::NOT_FOUND, String::new())
            }
        }

        async fn text(
            Path((uid, lang)): Path<(String, String)>,
            headers: HeaderMap,
        ) -> (StatusCode, String) {
            if !authorised(&headers) {
                return (StatusCode::UNAUTHORIZED, String::new());
            }
            match (uid.as_str(), lang.as_str()) {
                ("sn22.87", "pli") => (
                    StatusCode::OK,
                    r#"{"translator":"ms","segments":[{"key":"sn22.87:1.1","text":"Alaṃ, vakkali."},{"key":"sn22.87:1.2","text":"Aññaṃ."}]}"#.into(),
                ),
                ("sn22.87", "en") => (
                    StatusCode::OK,
                    r#"{"translator":"sujato","segments":[{"key":"sn22.87:1.1","text":"Enough, Vakkali."},{"key":"sn22.87:1.2","text":"Other."}]}"#.into(),
                ),
                _ => (StatusCode::NOT_FOUND, String::new()),
            }
        }

        async fn dpd(Path(word): Path<String>, headers: HeaderMap) -> (StatusCode, String) {
            if !authorised(&headers) {
                return (StatusCode::UNAUTHORIZED, String::new());
            }
            if word == "dhammo" {
                (
                    StatusCode::OK,
                    r#"{"word":"dhammo","senses":[{"lemma":"dhamma 1.01","pos":"masc","en":"nature; character","fr":"nature"},{"lemma":"dhamma 1.02","pos":"masc","en":"quality","fr":null}]}"#.into(),
                )
            } else {
                (StatusCode::NOT_FOUND, r#"{"error":"not_found"}"#.into())
            }
        }

        async fn serve(fake: Fake) -> ApiConfig {
            let app = axum::Router::new()
                .route("/v1/dpd/{word}", get(dpd))
                .route("/v1/dictionary", get(dictionary))
                .route("/v1/sutta/{uid}", get(sutta))
                .route("/v1/sutta/{uid}/{lang}", get(text))
                .with_state(fake);
            let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
            let addr = listener.local_addr().unwrap();
            tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
            ApiConfig {
                base: format!("http://{addr}"),
                token: TOKEN.to_string(),
            }
        }

        fn fake() -> Fake {
            Fake {
                calls: Arc::new(AtomicUsize::new(0)),
                fail_first: Arc::new(AtomicUsize::new(0)),
                refuse: false,
            }
        }

        async fn blocking<T: Send + 'static>(f: impl FnOnce() -> T + Send + 'static) -> T {
            tokio::task::spawn_blocking(f).await.unwrap()
        }

        #[tokio::test(flavor = "multi_thread")]
        async fn a_prefix_answers_its_entries_with_the_token_in_the_header_only() {
            let config = serve(fake()).await;
            let answer = blocking(move || search_in(&config, "vak")).await;
            let SearchAnswer::Entries(entries) = answer else {
                panic!("{answer:?}");
            };
            assert_eq!(entries.len(), 2);
            assert_eq!(entries[0].lemma, "vakkali");
            assert_eq!(
                entries[1].lemma, "vakka 1",
                "the numbered lemma is the identity"
            );
            assert_eq!(entries[0].source.as_deref(), Some("SN22.87"));
            assert_eq!(entries[1].example, None);
            assert!(!format!("{entries:?}").contains(TOKEN));
        }

        #[tokio::test(flavor = "multi_thread")]
        async fn nothing_matching_is_an_empty_list_and_a_short_q_too() {
            let config = serve(fake()).await;
            let c = config.clone();
            assert_eq!(
                blocking(move || search_in(&c, "zzz")).await,
                SearchAnswer::Entries(Vec::new())
            );
            assert_eq!(
                blocking(move || search_in(&config, "z")).await,
                SearchAnswer::Entries(Vec::new())
            );
        }

        #[tokio::test(flavor = "multi_thread")]
        async fn a_503_is_retried_once_then_answers() {
            let fake = fake();
            fake.fail_first.store(1, Ordering::SeqCst);
            let calls = fake.calls.clone();
            let config = serve(fake).await;
            let answer = blocking(move || search_in(&config, "vak")).await;
            assert!(
                matches!(answer, SearchAnswer::Entries(ref e) if e.len() == 2),
                "{answer:?}"
            );
            assert_eq!(calls.load(Ordering::SeqCst), 2);
        }

        #[tokio::test(flavor = "multi_thread")]
        async fn two_503s_are_unreachable_and_a_refused_token_is_not_retried() {
            let fake = fake();
            fake.fail_first.store(2, Ordering::SeqCst);
            let calls = fake.calls.clone();
            let config = serve(fake).await;
            assert_eq!(
                blocking(move || search_in(&config, "vak")).await,
                SearchAnswer::Unreachable
            );
            assert_eq!(calls.load(Ordering::SeqCst), 2);
            let refused = Fake {
                refuse: true,
                ..self::fake()
            };
            let calls = refused.calls.clone();
            let config = serve(refused).await;
            assert_eq!(
                blocking(move || search_in(&config, "vak")).await,
                SearchAnswer::Unreachable
            );
            assert_eq!(calls.load(Ordering::SeqCst), 1, "a 401 is never retried");
        }

        #[tokio::test(flavor = "multi_thread")]
        async fn a_dead_port_is_unreachable() {
            let config = ApiConfig {
                base: "http://127.0.0.1:9".to_string(),
                token: TOKEN.to_string(),
            };
            assert_eq!(
                blocking(move || search_in(&config, "vak")).await,
                SearchAnswer::Unreachable
            );
        }

        fn vakkali() -> Entry {
            Entry {
                headword: "vakkali".into(),
                lemma: "vakkali".into(),
                pos: "masc".into(),
                meaning: "name of a monk".into(),
                example: Some("alaṃ <b>vakkali</b>".into()),
                source: Some("SN22.87".into()),
                sutta: Some("Vakkalisutta".into()),
            }
        }

        #[tokio::test(flavor = "multi_thread")]
        async fn the_passage_is_found_with_its_english_line() {
            let config = serve(fake()).await;
            let answer = blocking(move || passage_in(&config, &vakkali())).await;
            let PassageAnswer::Found(example) = answer else {
                panic!("{answer:?}");
            };
            assert_eq!(example.text, "alaṃ vakkali", "the markup stripped");
            assert!(
                example.blank.is_some(),
                "the marked word blanked, as the deck does"
            );
            assert_eq!(example.source, "SN22.87");
            assert_eq!(example.translations["en"].text, "Enough, Vakkali.");
            assert_eq!(example.translations["en"].translator, "sujato");
        }

        #[tokio::test(flavor = "multi_thread")]
        async fn each_miss_has_its_name_and_no_example_makes_no_call() {
            let fake = fake();
            let calls = fake.calls.clone();
            let config = serve(fake).await;
            let c = config.clone();
            let none = Entry {
                example: None,
                source: None,
                ..vakkali()
            };
            assert_eq!(
                blocking(move || passage_in(&c, &none)).await,
                PassageAnswer::Miss(Miss::NoExample)
            );
            assert_eq!(calls.load(Ordering::SeqCst), 0);
            let c = config.clone();
            let unmapped = Entry {
                source: Some("KHP5".into()),
                ..vakkali()
            };
            assert_eq!(
                blocking(move || passage_in(&c, &unmapped)).await,
                PassageAnswer::Miss(Miss::UnmappedSource)
            );
            let c = config.clone();
            let unserved = Entry {
                source: Some("SN1.1".into()),
                ..vakkali()
            };
            assert_eq!(
                blocking(move || passage_in(&c, &unserved)).await,
                PassageAnswer::Miss(Miss::TextNotServed)
            );
            let elsewhere = Entry {
                example: Some("natthi idha".into()),
                ..vakkali()
            };
            assert_eq!(
                blocking(move || passage_in(&config, &elsewhere)).await,
                PassageAnswer::Miss(Miss::NotMatched)
            );
        }

        #[tokio::test(flavor = "multi_thread")]
        async fn a_refused_token_or_a_dead_api_is_transient_not_a_miss() {
            let config = serve(Fake {
                refuse: true,
                ..fake()
            })
            .await;
            let bad = ApiConfig {
                token: "wrong".into(),
                ..config
            };
            assert_eq!(
                blocking(move || passage_in(&bad, &vakkali())).await,
                PassageAnswer::Transient
            );
            let dead = ApiConfig {
                base: "http://127.0.0.1:9".into(),
                token: TOKEN.into(),
            };
            assert_eq!(
                blocking(move || passage_in(&dead, &vakkali())).await,
                PassageAnswer::Transient
            );
        }

        #[tokio::test(flavor = "multi_thread")]
        async fn a_form_is_looked_up_exactly_and_an_unknown_one_has_no_sense() {
            let config = serve(fake()).await;
            let c = config.clone();
            let answer = blocking(move || lookup_in(&c, "dhammo")).await;
            let LookupAnswer::Senses(senses) = answer else {
                panic!("{answer:?}");
            };
            assert_eq!(senses.len(), 2);
            assert_eq!(senses[0].lemma, "dhamma 1.01");
            assert_eq!(senses[0].fr.as_deref(), Some("nature"));
            assert_eq!(senses[1].fr, None);
            let c = config.clone();
            assert_eq!(
                blocking(move || lookup_in(&c, "zzz")).await,
                LookupAnswer::Senses(Vec::new())
            );
            let dead = ApiConfig {
                base: "http://127.0.0.1:9".into(),
                token: TOKEN.into(),
            };
            assert_eq!(
                blocking(move || lookup_in(&dead, "dhammo")).await,
                LookupAnswer::Unreachable
            );
        }

        #[test]
        fn the_configuration_needs_both_variables_and_shows_no_token() {
            let both = |name: &str| match name {
                URL_VAR => Some("https://api.example/".to_string()),
                TOKEN_VAR => Some(" tok ".to_string()),
                _ => None,
            };
            let config = ApiConfig::read(both).unwrap();
            assert_eq!(config.base, "https://api.example");
            assert_eq!(config.token, "tok");
            assert!(!format!("{config:?}").contains("tok"));
            assert!(ApiConfig::read(|_| None).is_none());
            assert!(ApiConfig::read(|n| (n == URL_VAR).then(|| "x".to_string())).is_none());
            assert!(ApiConfig::read(|n| (n == TOKEN_VAR).then(|| "x".to_string())).is_none());
            assert!(ApiConfig::read(|_| Some("  ".to_string())).is_none());
        }

        #[test]
        fn the_query_is_percent_encoded() {
            assert_eq!(percent("ṭhāna 2"), "%E1%B9%ADh%C4%81na%202");
            assert_eq!(percent("dham"), "dham");
        }
    }
}
