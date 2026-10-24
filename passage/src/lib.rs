//! A word's line from the suttas, found the way the deck builds it (062):
//! the DPD source mapped to a SuttaCentral uid ([`source`]), the example
//! matched in the API's Pāli text and its translations ([`segments`],
//! [`translate`]), the headword blanked ([`blank`]). The API is reached
//! through the synchronous [`fetch::Fetch`] trait, which build-deck
//! implements over curl with a cache and the site over an HTTP client in
//! a blocking task; this crate holds no client of its own.

pub mod asset;
pub mod blank;
pub mod fetch;
pub mod segments;
pub mod source;
pub mod translate;
