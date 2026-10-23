//! The PaliCanon API, as the builder reads it: `GET /v1/sutta/{uid}` (the
//! texts a sutta has) and `GET /v1/sutta/{uid}/{lang}` (one text, segment
//! by segment), behind a bearer token.
//!
//! [`Fetch`] is one GET: the body, `None` for a 404, an error otherwise.
//! [`HttpFetch`] makes the call through `curl` (at most four a second, the
//! token in the `Authorization` header and nowhere else: never in a
//! command line, an error, a log or its `Debug`); [`Cached`] answers a path it has seen from memory or from
//! its folder on disk, so a rebuild fetches nothing twice.

use std::collections::HashMap;
use std::fmt;
use std::io::Write;
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

pub use learn_passage::fetch::{
    Fetch, FetchError, Segment, SuttaInfo, SuttaText, TextRef, get_json,
};

/// The user agent of the builder's calls (the API's firewall refuses some
/// generic ones).
pub const USER_AGENT: &str = concat!("learn-pali-build-deck/", env!("CARGO_PKG_VERSION"));

/// The shortest gap between two calls: four a second.
pub const GAP: Duration = Duration::from_millis(250);

/// `GET /v1/sutta/{uid}`: the texts a sutta has.
/// The calls to the API over HTTPS, made by `curl` (no TLS stack in the
/// build: the one `ureq` brings carries a root bundle under a licence
/// `deny.toml` does not allow). The token is written to `curl`'s standard
/// input as a header (`--header @-`), so it is in no command line.
pub struct HttpFetch {
    base: String,
    token: String,
    last: Option<Instant>,
}

impl fmt::Debug for HttpFetch {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("HttpFetch")
            .field("base", &self.base)
            .finish_non_exhaustive()
    }
}

impl HttpFetch {
    /// A client of the API at `base` (`https://api.palicanon.app`).
    pub fn new(base: &str, token: String) -> HttpFetch {
        HttpFetch {
            base: base.trim_end_matches('/').to_string(),
            token,
            last: None,
        }
    }

    /// Waits until [`GAP`] has passed since the last call.
    fn throttle(&mut self) {
        if let Some(wait) = self.last.and_then(|last| GAP.checked_sub(last.elapsed())) {
            std::thread::sleep(wait);
        }
        self.last = Some(Instant::now());
    }

    /// `why`, cleared of the token should it ever hold it.
    fn scrub(&self, why: &str) -> String {
        if self.token.is_empty() {
            why.to_string()
        } else {
            why.replace(&self.token, "***")
        }
    }

    /// `curl`'s arguments for a GET of `path`: the header comes from its
    /// standard input, never from here.
    fn curl_args(&self, path: &str) -> Vec<String> {
        let flags = ["--silent", "--show-error", "--max-time", "30"];
        let mut args: Vec<String> = flags.iter().map(ToString::to_string).collect();
        args.extend(["--user-agent".to_string(), USER_AGENT.to_string()]);
        args.extend(["--header".to_string(), "@-".to_string()]);
        args.extend(["--write-out".to_string(), "\n%{http_code}".to_string()]);
        args.push(format!("{}{path}", self.base));
        args
    }

    /// One GET: the status and the body, or why it did not complete.
    fn call(&self, path: &str) -> Result<(u16, String), String> {
        let mut child = Command::new("curl")
            .args(self.curl_args(path))
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .map_err(|error| format!("cannot run curl: {error}"))?;
        if let Some(mut stdin) = child.stdin.take() {
            writeln!(stdin, "Authorization: Bearer {}", self.token)
                .map_err(|error| format!("cannot write to curl: {error}"))?;
        }
        let output = child
            .wait_with_output()
            .map_err(|error| format!("curl did not finish: {error}"))?;
        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(format!(
                "curl failed ({}): {}",
                output.status,
                stderr.trim()
            ));
        }
        let text =
            String::from_utf8(output.stdout).map_err(|_| "a body not in UTF-8".to_string())?;
        let (body, code) = text.rsplit_once('\n').ok_or("no status from curl")?;
        let code = code
            .trim()
            .parse()
            .map_err(|_| format!("a status that is not one: {code}"))?;
        Ok((code, body.to_string()))
    }
}

impl Fetch for HttpFetch {
    fn get(&mut self, path: &str) -> Result<Option<String>, FetchError> {
        self.throttle();
        match self.call(path) {
            Ok((200, body)) => Ok(Some(body)),
            Ok((404, _)) => Ok(None),
            Ok((code, _)) => Err(FetchError::Status(path.to_string(), code)),
            Err(why) => Err(FetchError::Transport(path.to_string(), self.scrub(&why))),
        }
    }
}

/// A [`Fetch`] that remembers: each path is asked of `inner` once, its
/// answer kept in memory and, with a folder, on disk (a 404 as an empty
/// `.404` file), so the next run reads it from there.
pub struct Cached<F> {
    inner: F,
    dir: Option<PathBuf>,
    seen: HashMap<String, Option<String>>,
}

impl<F: Fetch> Cached<F> {
    /// Remembers `inner`'s answers, on disk in `dir` when given.
    pub fn new(inner: F, dir: Option<PathBuf>) -> Cached<F> {
        Cached {
            inner,
            dir,
            seen: HashMap::new(),
        }
    }
}

impl<F: Fetch> Fetch for Cached<F> {
    fn get(&mut self, path: &str) -> Result<Option<String>, FetchError> {
        if let Some(answer) = self.seen.get(path) {
            return Ok(answer.clone());
        }
        let answer = if let Some(kept) = self.read_disk(path) {
            kept.into_answer()
        } else {
            let answer = self.inner.get(path)?;
            self.write_disk(path, answer.as_deref());
            answer
        };
        self.seen.insert(path.to_string(), answer.clone());
        Ok(answer)
    }
}

impl<F> Cached<F> {
    /// The answer kept on disk, if any.
    fn read_disk(&self, path: &str) -> Option<Kept> {
        let dir = self.dir.as_ref()?;
        let name = cache_name(path);
        if let Ok(body) = std::fs::read_to_string(dir.join(format!("{name}.json"))) {
            return Some(Kept::Body(body));
        }
        dir.join(format!("{name}.404"))
            .exists()
            .then_some(Kept::NotFound)
    }

    /// Keeps an answer on disk; a write that fails only costs a fetch next
    /// time.
    fn write_disk(&self, path: &str, answer: Option<&str>) {
        let Some(dir) = &self.dir else { return };
        let name = cache_name(path);
        let _ = match answer {
            Some(body) => std::fs::write(dir.join(format!("{name}.json")), body),
            None => std::fs::write(dir.join(format!("{name}.404")), ""),
        };
    }
}

/// An answer kept on disk.
enum Kept {
    /// A body (`<name>.json`).
    Body(String),
    /// A 404 (`<name>.404`).
    NotFound,
}

impl Kept {
    fn into_answer(self) -> Option<String> {
        match self {
            Kept::Body(body) => Some(body),
            Kept::NotFound => None,
        }
    }
}

/// The cache file name of a path: `/v1/sutta/sn22.87/en` →
/// `v1_sutta_sn22.87_en`.
pub fn cache_name(path: &str) -> String {
    path.trim_start_matches('/').replace('/', "_")
}

#[cfg(test)]
pub(crate) mod fixture {
    //! A [`Fetch`] over the recorded answers in `fixtures/api/`.

    use super::{Fetch, FetchError, cache_name};

    /// Answers from `fixtures/api/` (a missing file is a 404); counts calls.
    #[derive(Debug, Default)]
    pub(crate) struct Recorded {
        /// Every path asked, in order.
        pub(crate) asked: Vec<String>,
        /// A status every call answers with instead, when set.
        pub(crate) refuse: Option<u16>,
    }

    impl Fetch for Recorded {
        fn get(&mut self, path: &str) -> Result<Option<String>, FetchError> {
            self.asked.push(path.to_string());
            if let Some(code) = self.refuse {
                return Err(FetchError::Status(path.to_string(), code));
            }
            let file = format!(
                "{}/fixtures/api/{}.json",
                env!("CARGO_MANIFEST_DIR"),
                cache_name(path)
            );
            Ok(std::fs::read_to_string(file).ok())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::fixture::Recorded;
    use super::*;

    const SECRET: &str = "tok-SECRET-1234567890";

    #[test]
    fn a_path_has_a_flat_cache_name() {
        assert_eq!(cache_name("/v1/sutta/sn22.87/en"), "v1_sutta_sn22.87_en");
        assert_eq!(cache_name("/v1/sutta/dhp116-128"), "v1_sutta_dhp116-128");
    }

    #[test]
    fn the_recorded_answers_read_as_the_routes_types() {
        let mut fetch = Recorded::default();
        let info: SuttaInfo = get_json(&mut fetch, "/v1/sutta/sn22.87").unwrap().unwrap();
        let langs: Vec<(&str, &str)> = info
            .texts
            .iter()
            .map(|t| (t.lang.as_str(), t.translator.as_str()))
            .collect();
        assert_eq!(
            langs,
            vec![
                ("pli", "ms"),
                ("en", "sujato"),
                ("fr", "christelle"),
                ("vi", "minh_chau")
            ]
        );
        let en: SuttaText = get_json(&mut fetch, "/v1/sutta/sn22.87/en")
            .unwrap()
            .unwrap();
        assert_eq!(en.translator, "sujato");
        assert_eq!(en.segments[0].key, "sn22.87:0.1");
        let none: Option<SuttaText> = get_json(&mut fetch, "/v1/sutta/nope9").unwrap();
        assert_eq!(none, None);
    }

    #[test]
    fn a_body_of_another_shape_is_a_body_error() {
        struct Junk;
        impl Fetch for Junk {
            fn get(&mut self, _: &str) -> Result<Option<String>, FetchError> {
                Ok(Some("{\"nope\":1}".to_string()))
            }
        }
        let error = get_json::<SuttaText>(&mut Junk, "/v1/sutta/x/en").unwrap_err();
        assert!(matches!(error, FetchError::Body(..)), "{error:?}");
        assert!(error.to_string().contains("/v1/sutta/x/en"), "{error}");
    }

    #[test]
    fn each_path_is_asked_once_then_read_from_memory() {
        let mut fetch = Cached::new(Recorded::default(), None);
        for _ in 0..3 {
            assert!(fetch.get("/v1/sutta/sn22.87").unwrap().is_some());
            assert_eq!(fetch.get("/v1/sutta/nope9").unwrap(), None);
        }
        assert_eq!(
            fetch.inner.asked,
            vec!["/v1/sutta/sn22.87", "/v1/sutta/nope9"]
        );
    }

    #[test]
    fn a_second_run_reads_the_folder_and_asks_nothing() {
        let dir = std::env::temp_dir().join(format!("build-deck-cache-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let mut first = Cached::new(Recorded::default(), Some(dir.clone()));
        let body = first.get("/v1/sutta/sn22.87/en").unwrap();
        assert_eq!(first.get("/v1/sutta/nope9").unwrap(), None);
        assert!(dir.join("v1_sutta_sn22.87_en.json").exists());
        assert!(dir.join("v1_sutta_nope9.404").exists());
        let refusing = Recorded {
            refuse: Some(500),
            ..Recorded::default()
        };
        let mut second = Cached::new(refusing, Some(dir.clone()));
        assert_eq!(second.get("/v1/sutta/sn22.87/en").unwrap(), body);
        assert_eq!(second.get("/v1/sutta/nope9").unwrap(), None);
        assert_eq!(second.inner.asked.len(), 0);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn an_error_is_not_remembered() {
        let refusing = Recorded {
            refuse: Some(401),
            ..Recorded::default()
        };
        let mut fetch = Cached::new(refusing, None);
        assert!(fetch.get("/v1/sutta/sn22.87").is_err());
        assert!(fetch.get("/v1/sutta/sn22.87").is_err());
        assert_eq!(fetch.inner.asked.len(), 2);
    }

    /// A base no one listens on: the call fails at once, with no network.
    fn closed() -> HttpFetch {
        HttpFetch::new("http://127.0.0.1:9", SECRET.to_string())
    }

    #[test]
    fn a_refused_call_is_an_error_that_carries_no_token() {
        let error = closed().get("/v1/sutta/sn22.87").unwrap_err();
        assert!(matches!(error, FetchError::Transport(..)), "{error:?}");
        let shown = format!("{error} {error:?}");
        assert!(shown.contains("/v1/sutta/sn22.87"), "{shown}");
        assert!(!shown.contains(SECRET), "{shown}");
        assert!(!shown.contains("Bearer"), "{shown}");
    }

    #[test]
    fn the_token_is_on_no_command_line() {
        let args = closed().curl_args("/v1/sutta/sn22.87");
        assert_eq!(args.last().unwrap(), "http://127.0.0.1:9/v1/sutta/sn22.87");
        assert!(args.contains(&USER_AGENT.to_string()), "{args:?}");
        assert!(args.iter().all(|arg| !arg.contains(SECRET)), "{args:?}");
        assert!(args.iter().all(|arg| !arg.contains("Bearer")), "{args:?}");
    }

    #[test]
    fn the_client_never_shows_its_token() {
        let shown = format!("{:?}", closed());
        assert!(shown.contains("127.0.0.1:9"), "{shown}");
        assert!(!shown.contains(SECRET), "{shown}");
    }

    #[test]
    fn calls_are_spaced_at_least_a_quarter_second_apart() {
        let mut fetch = closed();
        let start = Instant::now();
        for _ in 0..3 {
            let _ = fetch.get("/v1/health");
        }
        assert!(start.elapsed() >= GAP * 2, "{:?}", start.elapsed());
    }
}
