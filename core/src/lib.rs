//! `learn_core` — the learning loop of learn-pali as a library.
//!
//! **Term and facets.** A [`deck`] is a list of terms, each one DPD headword
//! with its meaning, forms and an example. A term is learned through its
//! [`facet`]s: Meaning (pāli to sense) and Recall (sense to pāli) always,
//! Forms when it has a regular inflected form, Context when its example has a
//! blank.
//!
//! **Stages and the two transitions.** A term sits in one Leitner [`stage`]
//! (Day, Week, Month, Quarter, `HalfYear`, Year). When every facet has been
//! passed at the stage the term moves to the next one and is due again after
//! that stage's interval; one failed facet sends it back to Day.
//! [`session`] holds the queue and the transitions; [`forms`], [`grow`] and
//! [`stats`] hold the rest of the rules.
//!
//! **Sync.** [`merge`] joins two states with no clock: per term the entry
//! further along wins, so devices that sync converge whatever the order.
//!
//! **State and deck JSON.** The [`progress`] state and the deck are plain
//! JSON, versioned, with dates as `YYYY-MM-DD` ([`date`]). Nothing in this
//! crate reads a clock: `today` is always an input, so the same code runs at
//! the terminal and, later, in the browser (it builds for wasm32).

pub mod date;
pub mod deck;
pub mod facet;
pub mod forms;
pub mod grow;
pub mod merge;
pub mod progress;
pub mod session;
mod shuffle;
pub mod stage;
pub mod stats;
pub mod words;
