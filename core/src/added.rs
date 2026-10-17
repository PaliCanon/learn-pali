//! The words a learner adds to the practice from a passage.
//!
//! [`added_term`] builds the [`Term`] from what the popover of a passage
//! shows: the dictionary entry the learner pressed ([`WordEntry`]), the
//! passage it came from (the card's [`Example`]) and the form pressed. It is
//! a deck term in every respect, so it is asked, counted and summarised like
//! one: its id is the entry's headword exactly as the DPD writes it (the same
//! namespace as a deck [`TermId`]), its rank is `u32::MAX` so it sorts after
//! every deck term, its meaning is the DPD's short English sense (the deck
//! has no French or Vietnamese one either), and it has no forms, root or
//! construction. Its example is the passage with a blank on the first whole
//! word equal to the pressed form: Context is asked when the blank exists.

use std::fmt;

use crate::date::Date;
use crate::deck::{Blank, Deck, Example, Meaning, Pos, Term, TermId};
use crate::progress::{Progress, State};
use crate::session::{Session, unpassed_questions};
use crate::words::WordEntry;

/// Why a word was not added to the practice. The state is unchanged.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AddError {
    /// The word is already started (a deck term or an added word): it is
    /// in the practice.
    AlreadyStarted(TermId),
}

impl fmt::Display for AddError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            AddError::AlreadyStarted(id) => write!(f, "{id} is already started"),
        }
    }
}

impl std::error::Error for AddError {}

impl State {
    /// Adds `term` to the practice and **starts** it on `today`, as
    /// [`Session::request_new`] starts a deck term: a
    /// [`Progress`] in stage Day, due `today`.
    ///
    /// A word outside the deck is kept among the added words (after the
    /// ones already added); a deck term not started yet is only started (the
    /// deck's term is the one asked: it shadows `term`, [`State::term`]);
    /// an added word an older build left unstarted is only started.
    ///
    /// # Errors
    /// [`AddError::AlreadyStarted`] when the word already has progress.
    pub fn start_word(&mut self, deck: &Deck, term: Term, today: Date) -> Result<(), AddError> {
        if self.get(&term.id).is_some() {
            return Err(AddError::AlreadyStarted(term.id));
        }
        let kept = deck.term(&term.id).is_some() || self.added.iter().any(|h| h.id == term.id);
        self.progress.push(Progress::new(term.id.clone(), today));
        if !kept {
            self.added.push(term);
        }
        Ok(())
    }
}

impl Session<'_> {
    /// Adds `term` to the practice through the running session and starts
    /// it, as [`Session::request_new`] starts a deck term: due today, its
    /// facets queued after the ones already queued, kept by the next
    /// answer's [`Session::snapshot`] ([`State::start_word`]).
    ///
    /// # Errors
    /// [`AddError::AlreadyStarted`] when the word is already started.
    pub fn start_word(&mut self, term: Term) -> Result<(), AddError> {
        let (deck, today) = (self.deck, self.today);
        let id = term.id.clone();
        self.state.start_word(deck, term, today)?;
        if let (Some(term), Some(progress)) = (self.state.term(deck, &id), self.state.get(&id)) {
            self.queue.extend(unpassed_questions(term, progress));
        }
        Ok(())
    }

    /// Whether `id` is started: it has progress in the session's state.
    pub fn is_started(&self, id: &TermId) -> bool {
        self.state.get(id).is_some()
    }

    /// Takes back a word [`Session::start_word`] just started: the store
    /// refused to keep it, so the session must not count it. Its progress,
    /// its queued questions and, outside the deck, its added word go.
    /// Returns whether `id` was started.
    pub fn withdraw_word(&mut self, id: &TermId) -> bool {
        let Some(at) = self.state.progress.iter().position(|p| p.term == *id) else {
            return false;
        };
        self.state.progress.remove(at);
        if self.deck.term(id).is_none() {
            self.state.added.retain(|held| held.id != *id);
        }
        self.queue.retain(|question| question.term != *id);
        true
    }
}

/// The term for `entry` (pressed as `pressed` in `passage`); see the module
/// documentation.
pub fn added_term(entry: &WordEntry, passage: &Example, pressed: &str) -> Term {
    Term {
        id: TermId(entry.headword.clone()),
        lemma: lemma_of(&entry.headword).to_string(),
        pos: Pos(entry.pos.clone()),
        meaning: Meaning {
            en: entry.meaning.clone(),
            fr: None,
            vi: None,
            literal: None,
        },
        construction: None,
        root: None,
        example: Some(Example {
            blank: blank_on(&passage.text, pressed),
            ..passage.clone()
        }),
        forms: Vec::new(),
        rank: u32::MAX,
    }
}

/// The headword without its homonym number (`abhiññā 1` → `abhiññā`,
/// `sati 1.1` → `sati`).
fn lemma_of(headword: &str) -> &str {
    match headword.rsplit_once(' ') {
        Some((lemma, number))
            if !number.is_empty() && number.chars().all(|c| c.is_ascii_digit() || c == '.') =>
        {
            lemma
        }
        _ => headword,
    }
}

/// The text split around the first whole word equal to `pressed`, if any.
fn blank_on(text: &str, pressed: &str) -> Option<Blank> {
    let wanted: Vec<char> = pressed.chars().map(fold).collect();
    if wanted.is_empty() {
        return None;
    }
    let chars: Vec<(usize, char)> = text.char_indices().collect();
    let end_of = |at: usize| chars.get(at).map_or(text.len(), |(offset, _)| *offset);
    (0..chars.len())
        .filter(|&at| at + wanted.len() <= chars.len())
        .find(|&at| {
            let same = chars[at..at + wanted.len()]
                .iter()
                .zip(&wanted)
                .all(|((_, c), w)| fold(*c) == *w);
            same && !continues(&chars, at.checked_sub(1), true)
                && !continues(&chars, Some(at + wanted.len()), false)
        })
        .map(|at| {
            let (start, end) = (end_of(at), end_of(at + wanted.len()));
            Blank {
                before: text[..start].to_string(),
                answer: text[start..end].to_string(),
                after: text[end..].to_string(),
            }
        })
}

/// A character compared case-folded, the niggahita written with the dot
/// below (as [`crate::words::word_key`] does).
fn fold(c: char) -> char {
    let lower = c.to_lowercase().next().unwrap_or(c);
    if lower == 'ṁ' { 'ṃ' } else { lower }
}

/// A letter, a digit or a combining mark.
fn is_word(c: char) -> bool {
    c.is_alphanumeric() || ('\u{0300}'..='\u{036F}').contains(&c)
}

/// Whether the word goes on at `index` (the character just before the match
/// when `backwards`, just after otherwise): a word character, or a sandhi
/// apostrophe with a word character behind it.
fn continues(chars: &[(usize, char)], index: Option<usize>, backwards: bool) -> bool {
    let Some(&(_, c)) = index.and_then(|i| chars.get(i)) else {
        return false;
    };
    if is_word(c) {
        return true;
    }
    let beyond = index.and_then(|i| {
        if backwards {
            i.checked_sub(1)
        } else {
            Some(i + 1)
        }
    });
    matches!(c, '\'' | '’')
        && beyond
            .and_then(|i| chars.get(i))
            .is_some_and(|&(_, n)| is_word(n))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::deck::Translation;
    use crate::facet::{Facet, facets_of};

    fn entry(headword: &str) -> WordEntry {
        WordEntry {
            headword: headword.to_string(),
            pos: "adj".to_string(),
            grammar: Some("nom sg".to_string()),
            meaning: "blameworthy; at fault".to_string(),
        }
    }

    fn passage(text: &str) -> Example {
        Example {
            text: text.to_string(),
            source: "AN 3.66".to_string(),
            sutta: "an3.66".to_string(),
            blank: None,
            translations: [(
                "en".to_string(),
                Translation {
                    text: "A translated line.".to_string(),
                    translator: "sujato".to_string(),
                },
            )]
            .into_iter()
            .collect(),
        }
    }

    fn blank_of(term: &Term) -> Option<(&str, &str, &str)> {
        let blank = term.example.as_ref()?.blank.as_ref()?;
        Some((&blank.before, &blank.answer, &blank.after))
    }

    #[test]
    fn the_term_is_the_entry_as_a_deck_term() {
        let term = added_term(
            &entry("sānuvajja"),
            &passage("so sānuvajjo hoti"),
            "sānuvajjo",
        );
        assert_eq!(term.id, TermId("sānuvajja".to_string()));
        assert_eq!(term.lemma, "sānuvajja");
        assert_eq!(term.pos, Pos("adj".to_string()));
        assert_eq!(
            term.meaning,
            Meaning {
                en: "blameworthy; at fault".to_string(),
                fr: None,
                vi: None,
                literal: None,
            }
        );
        assert_eq!(term.rank, u32::MAX);
        assert_eq!(term.forms.len(), 0);
        assert_eq!(term.root, None);
        assert_eq!(term.construction, None);
    }

    #[test]
    fn the_id_keeps_the_homonym_number_and_the_lemma_drops_it() {
        for (headword, lemma) in [
            ("abhiññā 1", "abhiññā"),
            ("sati 1.1", "sati"),
            ("ca 2.1", "ca"),
            ("sānuvajja", "sānuvajja"),
            ("a b", "a b"),
        ] {
            let term = added_term(&entry(headword), &passage("x"), "x");
            assert_eq!(term.id.0, headword);
            assert_eq!(term.lemma, lemma, "{headword}");
        }
    }

    #[test]
    fn the_example_is_the_passage_copied_with_its_translations() {
        let passage = passage("so sānuvajjo hoti");
        let term = added_term(&entry("sānuvajja"), &passage, "sānuvajjo");
        let example = term.example.unwrap();
        assert_eq!(example.text, passage.text);
        assert_eq!(example.source, "AN 3.66");
        assert_eq!(example.sutta, "an3.66");
        assert_eq!(example.translations, passage.translations);
    }

    #[test]
    fn the_blank_is_on_the_first_whole_word_of_the_pressed_form() {
        let term = added_term(
            &entry("ca 2.1"),
            &passage("sāvajjo ca hoti sānuvajjo ca"),
            "ca",
        );
        assert_eq!(
            blank_of(&term),
            Some(("sāvajjo ", "ca", " hoti sānuvajjo ca"))
        );
    }

    #[test]
    fn a_word_twice_in_the_passage_is_blanked_once_on_the_first_whole_one() {
        let term = added_term(&entry("ca 2.1"), &passage("ca hoti ca"), "ca");
        assert_eq!(blank_of(&term), Some(("", "ca", " hoti ca")));
        // The first copy sits inside a longer word: the second is the one.
        let term = added_term(&entry("ca 2.1"), &passage("bhikkhūca hoti ca"), "ca");
        assert_eq!(blank_of(&term), Some(("bhikkhūca hoti ", "ca", "")));
        // Both blanks would make the answer ambiguous: only one is made.
        let blank = term.example.unwrap().blank.unwrap();
        assert_eq!(blank.before.matches("ca").count(), 1);
        assert_eq!(blank.after.matches("ca").count(), 0);
    }

    #[test]
    fn a_form_inside_a_longer_word_is_not_blanked() {
        let term = added_term(&entry("ca 2.1"), &passage("cakkhu ca hoti"), "ca");
        assert_eq!(blank_of(&term), Some(("cakkhu ", "ca", " hoti")));
        let term = added_term(&entry("ca 2.1"), &passage("bhikkhūca hoti"), "ca");
        assert_eq!(blank_of(&term), None);
    }

    #[test]
    fn the_match_ignores_case_and_the_niggahita_dot_and_keeps_the_text_as_written() {
        let term = added_term(&entry("sati 1.1"), &passage("Sati upaṭṭhitā"), "sati");
        assert_eq!(blank_of(&term), Some(("", "Sati", " upaṭṭhitā")));
        let term = added_term(&entry("evaṃ"), &passage("so evaṁ me"), "evaṃ");
        assert_eq!(blank_of(&term), Some(("so ", "evaṁ", " me")));
    }

    #[test]
    fn a_word_with_an_inner_apostrophe_is_one_word() {
        let term = added_term(&entry("na"), &passage("kathañ'c'ānanda na hoti"), "na");
        assert_eq!(blank_of(&term), Some(("kathañ'c'ānanda ", "na", " hoti")));
        let term = added_term(
            &entry("kathañ"),
            &passage("kathañ'c'ānanda kathañ"),
            "kathañ'c'ānanda",
        );
        assert_eq!(blank_of(&term), Some(("", "kathañ'c'ānanda", " kathañ")));
    }

    #[test]
    fn a_form_next_to_an_apostrophe_and_a_letter_is_part_of_a_longer_word() {
        let term = added_term(
            &entry("x"),
            &passage("ānanda'ti ti'ānanda ānanda"),
            "ānanda",
        );
        assert_eq!(
            blank_of(&term),
            Some(("ānanda'ti ti'ānanda ", "ānanda", ""))
        );
        // A quote that does not join two letters ends the word.
        let term = added_term(&entry("x"), &passage("‘ānanda’ ti"), "ānanda");
        assert_eq!(blank_of(&term), Some(("‘", "ānanda", "’ ti")));
    }

    #[test]
    fn a_form_not_found_leaves_the_example_without_a_blank() {
        let term = added_term(&entry("sānuvajja"), &passage("so hoti"), "sānuvajjo");
        assert_eq!(term.example.as_ref().unwrap().text, "so hoti");
        assert_eq!(blank_of(&term), None);
        assert_eq!(term.example.unwrap().blank, None);
    }

    #[test]
    fn an_empty_pressed_form_blanks_nothing() {
        let term = added_term(&entry("x"), &passage("so hoti"), "");
        assert_eq!(blank_of(&term), None);
    }

    #[test]
    fn the_facets_are_meaning_and_recall_and_context_when_the_blank_exists() {
        let with = added_term(&entry("ca 2.1"), &passage("so ca hoti"), "ca");
        let facets: Vec<Facet> = facets_of(&with).iter().collect();
        assert_eq!(facets, vec![Facet::Meaning, Facet::Recall, Facet::Context]);
        let without = added_term(&entry("ca 2.1"), &passage("so hoti"), "ca");
        let facets: Vec<Facet> = facets_of(&without).iter().collect();
        assert_eq!(facets, vec![Facet::Meaning, Facet::Recall]);
    }
}
