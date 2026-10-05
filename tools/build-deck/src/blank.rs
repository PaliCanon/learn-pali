//! Blanking the example.
//!
//! The DPD marks the headword's occurrence in `example_1` with `<b>…</b>`.
//! The **first** marked span is the blank (the others are shown as plain
//! words); when an example has no usable mark the lemma (its homonym number
//! removed), then each form value, is searched as a whole word. Matching is
//! NFC and case-folded, diacritics are kept (`sati` is not `satī`), no
//! sandhi is undone. Nothing found: no blank and a note, never a failure.

use std::ops::Range;

use learn_core::deck::{Blank, Form};
use unicode_normalization::UnicodeNormalization;

/// An example prepared for the deck.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Blanked {
    /// The sentence as plain text (no markup), NFC.
    pub text: String,
    /// The sentence split around the blank, when one was found.
    pub blank: Option<Blank>,
    /// Why there is no blank (an info line for the report), when none.
    pub note: Option<String>,
}

/// Strips the markup of `text_html` and blanks the word to recall.
///
/// See the module documentation for the rules.
pub fn blank_example(text_html: &str, lemma: &str, forms: &[Form]) -> Blanked {
    let normalised: String = text_html.nfc().collect();
    let (text, mark) = plain_text(&normalised);
    let (span, note) = match mark {
        Some(span) => (Some(span), None),
        None => match find_word(&text, lemma, forms) {
            Some(span) => (Some(span), None),
            None => (
                None,
                Some(
                    "no blank: no mark, and neither the lemma nor a form is a whole word"
                        .to_string(),
                ),
            ),
        },
    };
    let blank = span.map(|span| Blank {
        before: text[..span.start].to_string(),
        answer: text[span.clone()].to_string(),
        after: text[span.end..].to_string(),
    });
    Blanked { text, blank, note }
}

/// The text of some markup, and the byte range (in that text) of its first
/// `<b>…</b>` span when it is closed and not blank. Other tags are dropped,
/// further `<b>` marks read as plain words.
fn plain_text(markup: &str) -> (String, Option<Range<usize>>) {
    let mut text = String::new();
    let (mut start, mut end) = (None, None);
    let mut rest = markup;
    while let Some(open) = rest.find('<') {
        text.push_str(&rest[..open]);
        let Some(close) = rest[open..].find('>') else {
            rest = &rest[open..];
            break;
        };
        match rest[open + 1..open + close].trim() {
            "b" if start.is_none() => start = Some(text.len()),
            "/b" if start.is_some() && end.is_none() => end = Some(text.len()),
            _ => {}
        }
        rest = &rest[open + close + 1..];
    }
    text.push_str(rest);
    let span = start.zip(end).map(|(s, e)| s..e);
    let span = span.filter(|span| !text[span.clone()].trim().is_empty());
    (text, span)
}

/// The first whole word of `text` equal (NFC, case-folded) to the lemma
/// without its homonym number, else to one of the form values.
fn find_word(text: &str, lemma: &str, forms: &[Form]) -> Option<Range<usize>> {
    let words = words(text);
    let lemma = lemma.split_whitespace().next().unwrap_or("");
    let values = forms.iter().flat_map(|form| form.values.iter());
    std::iter::once(lemma)
        .chain(values.map(String::as_str))
        .filter(|candidate| !candidate.is_empty())
        .find_map(|candidate| {
            let wanted = fold(candidate);
            words
                .iter()
                .find(|word| fold(&text[(*word).clone()]) == wanted)
                .cloned()
        })
}

/// NFC and lower case.
fn fold(word: &str) -> String {
    word.nfc().collect::<String>().to_lowercase()
}

/// A letter, a digit or a combining mark.
fn is_word(c: char) -> bool {
    c.is_alphanumeric() || ('\u{0300}'..='\u{036F}').contains(&c)
}

/// The byte ranges of the words of `text`; an apostrophe between two word
/// characters stays inside the word (`kathañ'c'ānanda`).
fn words(text: &str) -> Vec<Range<usize>> {
    let chars: Vec<(usize, char)> = text.char_indices().collect();
    let at = |i: usize| chars.get(i).map_or(text.len(), |(offset, _)| *offset);
    let word_at = |i: usize| chars.get(i).is_some_and(|(_, c)| is_word(*c));
    let mut found = Vec::new();
    let mut i = 0;
    while i < chars.len() {
        if !word_at(i) {
            i += 1;
            continue;
        }
        let first = i;
        let apostrophe = |i: usize| chars.get(i).is_some_and(|(_, c)| matches!(c, '\'' | '’'));
        while word_at(i) || (apostrophe(i) && word_at(i + 1)) {
            i += 1;
        }
        found.push(at(first)..at(i));
    }
    found
}

#[cfg(test)]
mod tests {
    use super::*;

    fn form(label: &str, value: &str) -> Form {
        Form {
            label: label.to_string(),
            values: vec![value.to_string()],
            rare: false,
        }
    }

    #[allow(clippy::unnecessary_wraps)]
    fn blank(before: &str, answer: &str, after: &str) -> Option<Blank> {
        Some(Blank {
            before: before.to_string(),
            answer: answer.to_string(),
            after: after.to_string(),
        })
    }

    #[test]
    fn the_marked_word_is_the_blank() {
        let got = blank_example(
            "atthi, bhikkhave, dhammā <b>satiyā</b> sacchikaraṇīyā",
            "sati 1.1",
            &[],
        );
        assert_eq!(got.text, "atthi, bhikkhave, dhammā satiyā sacchikaraṇīyā");
        assert_eq!(
            got.blank,
            blank("atthi, bhikkhave, dhammā ", "satiyā", " sacchikaraṇīyā")
        );
        assert_eq!(got.note, None);
    }

    #[test]
    fn only_the_first_mark_is_blanked() {
        let got = blank_example("sāvajjo <b>ca</b> hoti sānuvajjo <b>ca</b>", "ca 2.1", &[]);
        assert_eq!(got.text, "sāvajjo ca hoti sānuvajjo ca");
        assert_eq!(got.blank, blank("sāvajjo ", "ca", " hoti sānuvajjo ca"));
    }

    #[test]
    fn a_mark_at_the_start_and_at_the_end() {
        let got = blank_example("<b>sati</b> upaṭṭhitā", "sati 1.1", &[]);
        assert_eq!(got.blank, blank("", "sati", " upaṭṭhitā"));
        let got = blank_example("upaṭṭhitā <b>sati</b>", "sati 1.1", &[]);
        assert_eq!(got.blank, blank("upaṭṭhitā ", "sati", ""));
    }

    #[test]
    fn no_mark_falls_back_to_the_lemma() {
        let got = blank_example("sati upaṭṭhitā hoti", "sati 1.1", &[]);
        assert_eq!(got.blank, blank("", "sati", " upaṭṭhitā hoti"));
        assert_eq!(got.note, None);
    }

    #[test]
    fn the_fallback_is_case_folded_and_keeps_the_text_as_written() {
        let got = blank_example("Sati upaṭṭhitā", "sati 1.1", &[]);
        assert_eq!(got.blank, blank("", "Sati", " upaṭṭhitā"));
    }

    #[test]
    fn no_mark_and_no_lemma_falls_back_to_an_inflected_form() {
        let forms = [form("fem inst sg", "satiyā")];
        let got = blank_example("so satiyā samannāgato", "sati 1.1", &forms);
        assert_eq!(got.blank, blank("so ", "satiyā", " samannāgato"));
    }

    #[test]
    fn the_lemma_is_preferred_to_a_form() {
        let forms = [form("fem inst sg", "satiyā")];
        let got = blank_example("satiyā sati", "sati 1.1", &forms);
        assert_eq!(got.blank, blank("satiyā ", "sati", ""));
    }

    #[test]
    fn matching_is_whole_word_and_keeps_diacritics() {
        for text in ["satiyā hoti", "satī hoti", "asati hoti", "sati1 hoti"] {
            let got = blank_example(text, "sati 1.1", &[]);
            assert_eq!(got.blank, None, "{text}");
            assert!(
                got.note.as_deref().unwrap().starts_with("no blank"),
                "{text}"
            );
        }
    }

    #[test]
    fn nothing_found_gives_a_note_and_the_plain_text() {
        let got = blank_example("evaṃ me <i>sutaṃ</i>", "kho", &[]);
        assert_eq!(got.text, "evaṃ me sutaṃ");
        assert_eq!(got.blank, None);
        assert!(got.note.as_deref().unwrap().starts_with("no blank: "));
    }

    #[test]
    fn apostrophes_inside_words_do_not_break_the_scan() {
        let got = blank_example("kathañ'c'ānanda ca hoti", "ca 2.1", &[]);
        assert_eq!(got.blank, blank("kathañ'c'ānanda ", "ca", " hoti"));
        let marked = blank_example("kathañ'<b>c</b>'ānanda hoti", "ca 2.1", &[]);
        assert_eq!(marked.text, "kathañ'c'ānanda hoti");
        assert_eq!(marked.blank, blank("kathañ'", "c", "'ānanda hoti"));
        let none = blank_example("kathañ'c'ānanda hoti", "ca 2.1", &[]);
        assert_eq!(none.blank, None);
    }

    #[test]
    fn the_text_and_the_match_are_nfc() {
        // `ā` as `a` + combining macron in the source, precomposed in the lemma.
        let got = blank_example("sa\u{0304}ti hoti", "sāti", &[]);
        assert_eq!(got.text, "sāti hoti");
        assert_eq!(got.blank, blank("", "sāti", " hoti"));
    }

    #[test]
    fn an_empty_or_unclosed_mark_is_no_mark() {
        let got = blank_example("sati <b></b> hoti", "sati 1.1", &[]);
        assert_eq!(got.blank, blank("", "sati", "  hoti"));
        let got = blank_example("sati <b>hoti", "sati 1.1", &[]);
        assert_eq!(got.text, "sati hoti");
        assert_eq!(got.blank, blank("", "sati", " hoti"));
    }

    #[test]
    fn an_empty_example_has_no_blank() {
        let got = blank_example("", "sati 1.1", &[]);
        assert_eq!(got.text, "");
        assert_eq!(got.blank, None);
        assert!(got.note.is_some());
    }
}
