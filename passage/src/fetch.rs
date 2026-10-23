//! What the API serves and how it is asked: the sutta's texts and segments,
//! the synchronous [`Fetch`] trait every caller implements (curl with a
//! cache in build-deck, an HTTP client in a blocking task on the site),
//! and [`get_json`], one typed GET.

use std::fmt;

use serde::Deserialize;

/// `GET /v1/sutta/{uid}`: which texts (languages, translators) the sutta has.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct SuttaInfo {
    /// The texts, one per language and translator.
    #[serde(default)]
    pub texts: Vec<TextRef>,
}

/// One text a sutta has: its language and translator.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct TextRef {
    /// The language code (`pli`, `en`, `fr`, `vi`).
    pub lang: String,
    /// The translator's id (`ms`, `sujato`).
    pub translator: String,
}

/// `GET /v1/sutta/{uid}/{lang}`: one text, segment by segment.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct SuttaText {
    /// The translator's id.
    pub translator: String,
    /// The segments, in order.
    pub segments: Vec<Segment>,
}

/// One segment: its key (`sn22.87:3.1`, or `p24` for a paragraph) and text.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct Segment {
    /// The segment key.
    pub key: String,
    /// The segment's text.
    pub text: String,
}

/// Why a GET gave no body.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FetchError {
    /// The API answered another status than 200 or 404 (401: the token).
    Status(String, u16),
    /// The call did not complete (connection, timeout, body).
    Transport(String, String),
    /// The body is not what the route returns.
    Body(String, String),
}

impl fmt::Display for FetchError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            FetchError::Status(path, code) => write!(f, "GET {path}: HTTP {code}"),
            FetchError::Transport(path, why) => write!(f, "GET {path}: {why}"),
            FetchError::Body(path, why) => write!(f, "GET {path}: unexpected body: {why}"),
        }
    }
}

impl std::error::Error for FetchError {}

/// One GET on the API: the body, `None` for a 404.
pub trait Fetch {
    /// Fetches `path` (`/v1/sutta/sn22.87/en`).
    ///
    /// # Errors
    /// Any status but 200 and 404, or a call that did not complete.
    fn get(&mut self, path: &str) -> Result<Option<String>, FetchError>;
}

/// Fetches `path` and reads its JSON.
///
/// # Errors
/// The fetch's error, or [`FetchError::Body`] when the JSON is not a `T`.
pub fn get_json<T: serde::de::DeserializeOwned>(
    fetch: &mut dyn Fetch,
    path: &str,
) -> Result<Option<T>, FetchError> {
    let Some(body) = fetch.get(path)? else {
        return Ok(None);
    };
    serde_json::from_str(&body)
        .map(Some)
        .map_err(|error| FetchError::Body(path.to_string(), error.to_string()))
}
