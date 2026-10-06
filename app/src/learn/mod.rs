//! The pure parts of the screens, tested natively: what the pages compute
//! from the state, the deck and `today` before they render anything.
//!
//! - [`window`]: the Context facet's line, windowed around the blank.
//! - [`words`]: parts of speech, form labels and dates in words.
//! - [`home_state`]: which of its states the home shows.
//! - [`summary`]: what a finished session changed, counted in terms.
//! - [`key`]: the card's presentation key and the grade keys' rules.
//! - [`run`]: a review session as the review page drives it.
//! - [`stages`]: the stages' names and counts.

pub(crate) mod home_state;
pub(crate) mod key;
pub(crate) mod run;
pub(crate) mod stages;
pub(crate) mod summary;
pub(crate) mod window;
pub(crate) mod words;
