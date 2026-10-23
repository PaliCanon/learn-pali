//! The example's translations, fetched as the deck does (062: also at
//! request time on the site): the source mapped to a uid, the Pāli text
//! matched, each of the site's languages asked when the sutta has it.

use std::collections::BTreeMap;

use learn_core::deck::{Example, Translation};

use crate::fetch::{Fetch, FetchError, SuttaInfo, SuttaText, get_json};
use crate::segments::{match_segments, translations_for};
use crate::source::source_uid;

/// The site's languages, asked in this order when the sutta has the text.
pub const LANGS: [&str; 3] = ["en", "fr", "vi"];

/// What one example gave: its translations, and a note per language left out.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Found {
    /// The translations by language.
    pub translations: BTreeMap<String, Translation>,
    /// Why a language the sutta has was not taken.
    pub notes: Vec<String>,
}

/// The translations of one example.
///
/// # Errors
/// Why the example is skipped: its source maps to no uid, the API has no
/// such sutta, a call failed, or the example is not in the Pāli text.
/// Why an example got no passage: the deck's report prints it, the site
/// (062) tells a Miss (the word learns without its line) from a failed call
/// (nothing is added, try again).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Why {
    /// The DPD source maps to no SuttaCentral uid.
    UnmappedSource(String),
    /// The API serves no such sutta.
    NotServed(String),
    /// The sutta has no Pāli text.
    NoPali(String),
    /// The example's words are not found in the sutta's Pāli.
    NotMatched(String),
    /// A call failed (a status, the transport, an unreadable body).
    Fetch(FetchError),
}

impl std::fmt::Display for Why {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Why::UnmappedSource(source) => write!(f, "the source {source} maps to no uid"),
            Why::NotServed(uid) => write!(f, "{uid} not found in the API"),
            Why::NoPali(uid) => write!(f, "{uid} has no Pāli text"),
            Why::NotMatched(uid) => write!(f, "the example is not found in {uid}"),
            Why::Fetch(error) => error.fmt(f),
        }
    }
}

impl std::error::Error for Why {}

/// # Errors
/// [`Why`]: the example cannot be placed, or a call failed.
pub fn translate_example(example: &Example, fetch: &mut dyn Fetch) -> Result<Found, Why> {
    let uid =
        source_uid(&example.source).ok_or_else(|| Why::UnmappedSource(example.source.clone()))?;
    let info: SuttaInfo = get_json(fetch, &format!("/v1/sutta/{uid}"))
        .map_err(Why::Fetch)?
        .ok_or_else(|| Why::NotServed(uid.clone()))?;
    let pli: SuttaText = get_json(fetch, &format!("/v1/sutta/{uid}/pli"))
        .map_err(Why::Fetch)?
        .ok_or_else(|| Why::NoPali(uid.clone()))?;
    let keys = match_segments(&pli.segments, &example.text);
    if keys.is_empty() {
        return Err(Why::NotMatched(uid));
    }
    let mut found = Found::default();
    for lang in LANGS {
        if info.texts.iter().any(|text| text.lang == lang) {
            translate_lang(fetch, &uid, lang, &keys, &mut found);
        }
    }
    Ok(found)
}

/// The `keys` of the sutta's `lang` text into `found`, or a note why not.
fn translate_lang(
    fetch: &mut dyn Fetch,
    uid: &str,
    lang: &str,
    keys: &[String],
    found: &mut Found,
) {
    match get_json::<SuttaText>(fetch, &format!("/v1/sutta/{uid}/{lang}")) {
        Ok(Some(text)) => match translations_for(keys, &text.segments) {
            Some(line) => {
                let translation = Translation {
                    text: line,
                    translator: text.translator,
                };
                found.translations.insert(lang.to_string(), translation);
            }
            None => found.notes.push(format!(
                "{lang}: {uid} ({}) is not segment-aligned with the example",
                text.translator
            )),
        },
        Ok(None) => found
            .notes
            .push(format!("{lang}: {uid} has no {lang} text")),
        Err(error) => found.notes.push(format!("{lang}: {error}")),
    }
}
