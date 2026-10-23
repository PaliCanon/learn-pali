//! Finding a DPD example in a SuttaCentral text, and its translation.
//!
//! The DPD writes `alaṃ, vakkali` where SuttaCentral writes `“Alaṁ,
//! vakkali,`: both are compared [`normalise`]d (NFC, lower case, `ṃ` as
//! `ṁ`, letters only: spaces, punctuation, quotes and the DPD's sandhi
//! apostrophes dropped). [`match_segments`] gives the contiguous run of
//! segment keys the example covers; [`translations_for`] joins the same
//! keys of a translation, which shares SuttaCentral's segment keys when it
//! is aligned.

use std::collections::HashMap;

use unicode_normalization::UnicodeNormalization;

use crate::fetch::Segment;

/// The text as compared: NFC, lower case, `ṃ` as `ṁ`, letters only.
pub fn normalise(text: &str) -> String {
    text.nfc()
        .flat_map(char::to_lowercase)
        .map(|c| if c == 'ṃ' { 'ṁ' } else { c })
        .filter(|c| c.is_alphabetic())
        .collect()
}

/// The shortest clause that may anchor a fuzzy match, in bytes.
const MIN_ANCHOR: usize = 12;

/// A text's segments run together, normalised, with each segment's byte
/// span in the run.
struct Run {
    text: String,
    spans: Vec<(usize, usize)>,
}

impl Run {
    fn new(segments: &[Segment]) -> Run {
        let mut run = Run {
            text: String::new(),
            spans: Vec::with_capacity(segments.len()),
        };
        for segment in segments {
            let start = run.text.len();
            run.text.push_str(&normalise(&segment.text));
            run.spans.push((start, run.text.len()));
        }
        run
    }

    /// The keys of the segments overlapping `from..to`.
    fn keys_over(&self, segments: &[Segment], (from, to): (usize, usize)) -> Vec<String> {
        segments
            .iter()
            .zip(&self.spans)
            .filter(|(_, (start, end))| start < end && *start < to && *end > from)
            .map(|(segment, _)| segment.key.clone())
            .collect()
    }
}

/// The example's clauses (cut at punctuation and line ends), normalised,
/// each with its byte offset in the normalised example.
fn clauses(example: &str) -> Vec<(usize, String)> {
    let cut = |c: char| ",;.?!:\n“”«»—–()\"".contains(c);
    let mut offset = 0;
    let mut out = Vec::new();
    for clause in example.split(cut).map(normalise) {
        if !clause.is_empty() {
            let len = clause.len();
            out.push((offset, clause));
            offset += len;
        }
    }
    out
}

/// Where the example lies in `run` when it is not there whole: see
/// [`match_segments`].
fn fuzzy_region(run: &str, example: &str, needle: usize) -> Option<(usize, usize)> {
    let clauses = clauses(example);
    let (anchor_offset, anchor_at) = clauses
        .iter()
        .filter(|(_, clause)| clause.len() >= MIN_ANCHOR)
        .filter_map(|(offset, clause)| {
            run.find(clause.as_str())
                .map(|at| (clause.len(), *offset, at))
        })
        .max_by_key(|(len, _, _)| *len)
        .map(|(_, offset, at)| (offset, at))?;
    let slack = needle / 2;
    let (mut matched, mut region) = (0, None::<(usize, usize)>);
    for (offset, clause) in &clauses {
        let expected = (anchor_at + offset).saturating_sub(anchor_offset);
        let nearest = run
            .match_indices(clause.as_str())
            .map(|(at, _)| at)
            .min_by_key(|at| at.abs_diff(expected))
            .filter(|at| at.abs_diff(expected) <= slack);
        if let Some(at) = nearest {
            matched += clause.len();
            let end = at + clause.len();
            region = Some(region.map_or((at, end), |(from, to)| (from.min(at), to.max(end))));
        }
    }
    region.filter(|_| matched * 2 >= needle)
}

/// The keys of the contiguous run of `pli` segments the example covers,
/// in order; empty when the example is not found.
///
/// The whole example is looked for first. When the two editions differ in
/// a word (`vijjati` / `vijjatī`) or the text abridges a list, the example
/// is found by its clauses (cut at punctuation and line ends): the longest
/// clause found anchors it, every other clause is taken where it falls
/// nearest its place, and the run spans the clauses found, provided they
/// make at least half the example.
pub fn match_segments(pli: &[Segment], example: &str) -> Vec<String> {
    let needle = normalise(example);
    if needle.is_empty() {
        return Vec::new();
    }
    let run = Run::new(pli);
    run.text
        .find(&needle)
        .map(|at| (at, at + needle.len()))
        .or_else(|| fuzzy_region(&run.text, example, needle.len()))
        .map(|region| run.keys_over(pli, region))
        .unwrap_or_default()
}

/// The translation of `keys`: the texts of those keys in `segments`,
/// joined with a space, a quote left open or closed by the cut removed.
/// `None` when the translation is not segment-aligned (a key without `:`,
/// such as a paragraph `p24`) or holds none of the keys.
pub fn translations_for(keys: &[String], segments: &[Segment]) -> Option<String> {
    if segments.iter().any(|segment| !segment.key.contains(':')) {
        return None;
    }
    let by_key: HashMap<&str, &str> = segments
        .iter()
        .map(|segment| (segment.key.as_str(), segment.text.trim()))
        .collect();
    let parts: Vec<&str> = keys
        .iter()
        .filter_map(|key| by_key.get(key.as_str()).copied())
        .filter(|text| !text.is_empty())
        .collect();
    (!parts.is_empty()).then(|| tidy_quotes(&strip_markup(&parts.join(" "))))
}

/// The text without SuttaCentral's markup (`<j>` verse joins, `<em>`).
fn strip_markup(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut in_tag = false;
    for c in text.chars() {
        match c {
            '<' => in_tag = true,
            '>' if in_tag => in_tag = false,
            _ if !in_tag => out.push(c),
            _ => {}
        }
    }
    out
}

/// Removes the quote marks a cut leaves unpaired at either end: an
/// opening `“`, `‘` or `«` with no closing one, a closing `”`, `’` or `»`
/// with no opening one (a `’` only after punctuation, where it is not an
/// apostrophe; a `’` between two letters never counts as a quote), and the
/// space beside it.
pub fn tidy_quotes(text: &str) -> String {
    let mut text = text.trim().to_string();
    while let Some(shorter) = strip_unpaired(&text) {
        text = shorter;
    }
    text
}

/// How many times `quote` stands in `text` as a quote: a `’` between two
/// letters is an apostrophe (`there’s`, `l’enseignement`), not counted.
fn quote_count(text: &str, quote: char) -> usize {
    let chars: Vec<char> = text.chars().collect();
    let letter = |at: Option<&char>| at.is_some_and(|c| c.is_alphabetic());
    (0..chars.len())
        .filter(|&at| chars[at] == quote)
        .filter(|&at| {
            quote != '’' || !(at > 0 && letter(chars.get(at - 1)) && letter(chars.get(at + 1)))
        })
        .count()
}

/// The quote pairs [`tidy_quotes`] knows.
const QUOTES: [(char, char); 3] = [('“', '”'), ('‘', '’'), ('«', '»')];

/// `text` with one unpaired quote taken off an end, or `None` when there
/// is none.
fn strip_unpaired(text: &str) -> Option<String> {
    let count = |c: char| quote_count(text, c);
    for (open, close) in QUOTES {
        if let Some(rest) = text.strip_prefix(open)
            && count(open) > count(close)
        {
            return Some(rest.trim_start().to_string());
        }
        if let Some(rest) = text.strip_suffix(close)
            && count(close) > count(open)
            && (close != '’' || rest.ends_with(|c: char| ".,;:!?".contains(c)))
        {
            return Some(rest.trim_end().to_string());
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fetch::SuttaText;

    fn text(name: &str) -> SuttaText {
        let path = format!(
            "{}/fixtures/api/v1_sutta_{name}.json",
            env!("CARGO_MANIFEST_DIR")
        );
        serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap()
    }

    const VAKKALI: &str = "alaṃ, vakkali, kiṃ te iminā pūtikāyena diṭṭhena? yo kho, vakkali, \
        dhammaṃ passati so maṃ passati, yo maṃ passati so dhammaṃ passati.";
    const DHP127: &str = "na antalikkhe na samuddamajjhe,\nna pabbatānaṃ vivaraṃ pavissa,\n\
        na vijjati so jagatippadeso,\nyatthaṭṭhito mucceyya pāpakammā.";

    fn keys(list: &[&str]) -> Vec<String> {
        list.iter().map(|k| (*k).to_string()).collect()
    }

    fn segment(key: &str, text: &str) -> Segment {
        Segment {
            key: key.to_string(),
            text: text.to_string(),
        }
    }

    #[test]
    fn normalising_keeps_letters_lower_case_with_one_niggahita() {
        assert_eq!(normalise("“Alaṁ, vakkali, kiṃ te’"), "alaṁvakkalikiṁte");
        assert_eq!(normalise("na'y'idaṃ"), "nayidaṁ");
        assert_eq!(normalise("ti.\n— !?"), "ti");
        // a decomposed `ā` (a + combining macron) equals the composed one
        assert_eq!(normalise("a\u{304}"), normalise("ā"));
    }

    #[test]
    fn the_vakkali_example_covers_three_segments() {
        let pli = text("sn22.87_pli").segments;
        assert_eq!(
            match_segments(&pli, VAKKALI),
            keys(&["sn22.87:3.1", "sn22.87:3.2", "sn22.87:3.3"])
        );
    }

    #[test]
    fn an_example_not_in_the_text_matches_nothing() {
        let pli = text("sn22.87_pli").segments;
        let none = match_segments(&pli, "sabbe saṅkhārā aniccā'ti, yadā paññāya passati");
        assert_eq!(none.len(), 0);
        assert_eq!(match_segments(&pli, "").len(), 0);
        assert_eq!(match_segments(&[], VAKKALI).len(), 0);
    }

    #[test]
    fn a_word_spelt_otherwise_still_finds_the_verse_by_its_lines() {
        // the DPD writes `vijjati`, SuttaCentral `vijjatī`
        let pli = text("dhp116-128_pli").segments;
        assert_eq!(
            match_segments(&pli, DHP127),
            keys(&["dhp127:1", "dhp127:2", "dhp127:3", "dhp127:4"])
        );
    }

    #[test]
    fn a_lone_short_clause_found_elsewhere_is_not_a_match() {
        // `vakkali` alone is in the text, but it is not half the example
        let pli = text("sn22.87_pli").segments;
        let found = match_segments(&pli, "vakkali, imaṃ kho pana aññaṃ vacanaṃ na passāmi");
        assert_eq!(found.len(), 0, "{found:?}");
    }

    #[test]
    fn the_clauses_found_are_taken_near_the_anchor_not_anywhere() {
        let pli = vec![
            segment("x:1", "Bhikkhave, idaṁ paṭhamaṁ."),
            segment("x:2", "Aññaṁ kiñci."),
            segment("x:3", "Iti kho bhikkhave dukkhaṁ ariyasaccaṁ pariññeyyaṁ;"),
            segment("x:4", "dukkhasamudayo pahātabbo."),
        ];
        let example =
            "iti kho, bhikkhave, dukkhaṃ ariyasaccaṃ pariññeyyan'ti, dukkhasamudayo pahātabbo";
        assert_eq!(match_segments(&pli, example), keys(&["x:3", "x:4"]));
    }

    #[test]
    fn the_english_and_french_of_the_vakkali_lines() {
        let lines = keys(&["sn22.87:3.1", "sn22.87:3.2", "sn22.87:3.3"]);
        assert_eq!(
            translations_for(&lines, &text("sn22.87_en").segments).as_deref(),
            Some(
                "Enough, Vakkali! Why would you want to see this rotting body? \
                 One who sees the teaching sees me. One who sees me sees the teaching."
            )
        );
        let fr = translations_for(&lines, &text("sn22.87_fr").segments).unwrap();
        assert!(fr.starts_with("Assez, Vakkali"), "{fr}");
        assert!(
            fr.ends_with("Celui qui me voit, voit l’enseignement."),
            "{fr}"
        );
    }

    #[test]
    fn a_translation_by_paragraph_is_not_aligned() {
        let lines = keys(&["sn22.87:3.1"]);
        assert_eq!(translations_for(&lines, &text("sn22.87_vi").segments), None);
        // the French Dhammapada is keyed `sc127`
        let verse = keys(&["dhp127:1"]);
        assert_eq!(
            translations_for(&verse, &text("dhp116-128_fr").segments),
            None
        );
    }

    #[test]
    fn keys_the_translation_lacks_are_skipped_and_none_gives_nothing() {
        let en = text("dhp116-128_en").segments;
        let verse = keys(&["dhp127:0", "dhp127:1", "dhp127:2", "dhp127:3", "dhp127:4"]);
        assert_eq!(
            translations_for(&verse, &en).as_deref(),
            Some(
                "Not in midair, nor mid-ocean, nor hiding in a mountain cleft; \
                 you’ll find no place on the planet to escape your wicked deeds."
            )
        );
        assert_eq!(translations_for(&keys(&["dhp999:1"]), &en), None);
        assert_eq!(translations_for(&[], &en), None);
    }

    #[test]
    fn suttacentral_markup_in_a_translation_is_dropped() {
        let segments = vec![
            segment(
                "snp4.2:8.1",
                "Having understood perception <j>and crossed the flood,",
            ),
            segment("snp4.2:8.2", "the <em>sage</em> is not stuck."),
        ];
        let lines = keys(&["snp4.2:8.1", "snp4.2:8.2"]);
        assert_eq!(
            translations_for(&lines, &segments).as_deref(),
            Some("Having understood perception and crossed the flood, the sage is not stuck.")
        );
    }

    #[test]
    fn an_apostrophe_does_not_pair_an_opening_quote() {
        assert_eq!(
            tidy_quotes("‘I hope there’s no reason to blame myself?"),
            "I hope there’s no reason to blame myself?"
        );
        assert_eq!(tidy_quotes("‘It’s so,’ he said."), "‘It’s so,’ he said.");
    }

    #[test]
    fn quotes_left_unpaired_by_the_cut_are_removed() {
        assert_eq!(tidy_quotes("“Enough, Vakkali!"), "Enough, Vakkali!");
        assert_eq!(tidy_quotes("« Assez !"), "Assez !");
        assert_eq!(tidy_quotes("«\u{a0}Assez !"), "Assez !");
        assert_eq!(tidy_quotes("he said so.”"), "he said so.");
        assert_eq!(tidy_quotes("the end.’”"), "the end.");
        assert_eq!(tidy_quotes("‘Go on"), "Go on");
        assert_eq!(tidy_quotes("Assez ! »"), "Assez !");
        // paired quotes and apostrophes stay
        assert_eq!(tidy_quotes("“Yes,” he said."), "“Yes,” he said.");
        assert_eq!(
            tidy_quotes("you’ll see l’enseignement"),
            "you’ll see l’enseignement"
        );
        assert_eq!(tidy_quotes("the monks’"), "the monks’");
    }
}
