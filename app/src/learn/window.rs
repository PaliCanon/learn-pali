//! The Context facet's front: the example's line windowed around its blank
//! ([`context_window`]), so a long line never hides the blank and is never
//! clamped by CSS.

use learn_core::deck::Blank;

/// The longest window [`context_window`] keeps, in characters (not bytes:
/// Pāli diacritics count one each), ellipses not counted (A4).
pub(crate) const MAX_CHARS: usize = 120;

/// One run of the windowed line, in reading order.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Segment {
    /// Text shown as is. A side the window trimmed starts (or ends) with
    /// `…` inside the run.
    Text(String),
    /// The blank (`___`), where the answer was; every occurrence of the
    /// answer in the line is one.
    Blank,
}

/// One unit of the line while it is being cut: a character, or a blank
/// standing for one occurrence of the answer.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Item {
    Char(char),
    Blank,
}

impl Item {
    /// A letter or digit: what a word is made of. Spaces, punctuation, the
    /// sandhi apostrophe and hyphens all end a word.
    fn is_word(self) -> bool {
        matches!(self, Item::Char(c) if c.is_alphanumeric())
    }

    fn is_space(self) -> bool {
        matches!(self, Item::Char(c) if c.is_whitespace())
    }
}

/// The line of `blank` (`before` + the answer + `after`) as segments:
/// every whole-word occurrence of the answer blanked, at most `max_chars`
/// characters kept around the first blank, a `…` on each trimmed side and
/// none on a side kept whole; the blank always in the output.
///
/// The line is measured as it reads with the answer in place: a blank
/// weighs as many characters as the answer, so a line of at most
/// `max_chars` characters is returned whole, and the text runs of a
/// windowed one hold at most `max_chars` characters less the blanks kept,
/// plus the two `…`. A window starts and ends on a word boundary (a cut
/// that would split a word moves inward to the next boundary).
pub(crate) fn context_window(blank: &Blank, max_chars: usize) -> Vec<Segment> {
    let items = blank_items(blank);
    let width = blank.answer.chars().count().max(1);
    let weight = |item: &Item| if *item == Item::Blank { width } else { 1 };
    let first = items
        .iter()
        .position(|item| *item == Item::Blank)
        .unwrap_or(0);
    let total: usize = items.iter().map(weight).sum();
    let (start, end) = if total <= max_chars {
        (0, items.len())
    } else {
        let (start, end) = expand(&items, first, max_chars, weight);
        (
            snap_start(&items, start, first),
            snap_end(&items, end, first),
        )
    };
    segments(&items[start..end], start > 0, end < items.len())
}

/// The line as items: the answer's own place is a blank whatever the
/// text around it, and so is every other whole-word occurrence of the
/// answer (compared case-folded, diacritics kept).
fn blank_items(blank: &Blank) -> Vec<Item> {
    let line: Vec<char> = [
        blank.before.as_str(),
        blank.answer.as_str(),
        blank.after.as_str(),
    ]
    .concat()
    .chars()
    .collect();
    let answer: Vec<char> = blank.answer.chars().collect();
    let own = blank.before.chars().count();
    let own_end = own + answer.len();
    let mut items = Vec::with_capacity(line.len());
    let mut placed = false;
    let mut i = 0;
    while i < line.len() {
        if i == own && !placed {
            items.push(Item::Blank);
            placed = true;
            i = own_end;
        } else if !answer.is_empty()
            && (i >= own_end || i + answer.len() <= own)
            && is_whole_word(&line, i, &answer)
        {
            items.push(Item::Blank);
            i += answer.len();
        } else {
            items.push(Item::Char(line[i]));
            i += 1;
        }
    }
    if !placed {
        items.push(Item::Blank);
    }
    items
}

/// `answer` sits at `at` in `line`, case-folded, between non-word characters
/// (or the line's ends).
fn is_whole_word(line: &[char], at: usize, answer: &[char]) -> bool {
    let Some(candidate) = line.get(at..at + answer.len()) else {
        return false;
    };
    let same = candidate
        .iter()
        .zip(answer)
        .all(|(a, b)| a.to_lowercase().eq(b.to_lowercase()));
    let word = |c: Option<&char>| c.is_some_and(|c| c.is_alphanumeric());
    let before = at.checked_sub(1).and_then(|i| line.get(i));
    same && !word(before) && !word(line.get(at + answer.len()))
}

/// The range around the item `centre` that fits `max_chars`, grown one item
/// at a time on each side in turn, so a side that runs out leaves its
/// share to the other.
fn expand(
    items: &[Item],
    centre: usize,
    max_chars: usize,
    weight: impl Fn(&Item) -> usize,
) -> (usize, usize) {
    let (mut start, mut end) = (centre, centre + 1);
    let mut used = weight(&items[centre]);
    loop {
        let mut grew = false;
        if start > 0 && used + weight(&items[start - 1]) <= max_chars {
            start -= 1;
            used += weight(&items[start]);
            grew = true;
        }
        if end < items.len() && used + weight(&items[end]) <= max_chars {
            used += weight(&items[end]);
            end += 1;
            grew = true;
        }
        if !grew {
            return (start, end);
        }
    }
}

/// Moves a cut at `start` inward off a word it would split, then past the
/// spaces; never reaches the blank at `first`.
fn snap_start(items: &[Item], mut start: usize, first: usize) -> usize {
    if start > 0 && items[start - 1].is_word() {
        while start < first && items[start].is_word() {
            start += 1;
        }
    }
    while start < first && items[start].is_space() {
        start += 1;
    }
    start
}

/// Moves a cut at `end` inward off a word it would split, then past the
/// spaces; always keeps the blank at `first`.
fn snap_end(items: &[Item], mut end: usize, first: usize) -> usize {
    if end < items.len() && items[end].is_word() {
        while end > first + 1 && items[end - 1].is_word() {
            end -= 1;
        }
    }
    while end > first + 1 && items[end - 1].is_space() {
        end -= 1;
    }
    end
}

/// The items as runs of text and blanks; `…` opens the first run when the
/// start was cut and closes the last when the end was.
fn segments(items: &[Item], cut_start: bool, cut_end: bool) -> Vec<Segment> {
    let mut out: Vec<Segment> = Vec::new();
    let mut run = String::new();
    if cut_start {
        run.push('…');
    }
    for item in items {
        match item {
            Item::Char(c) => run.push(*c),
            Item::Blank => {
                if !run.is_empty() {
                    out.push(Segment::Text(std::mem::take(&mut run)));
                }
                out.push(Segment::Blank);
            }
        }
    }
    if cut_end {
        run.push('…');
    }
    if !run.is_empty() {
        out.push(Segment::Text(run));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn blank(before: &str, answer: &str, after: &str) -> Blank {
        Blank {
            before: before.to_string(),
            answer: answer.to_string(),
            after: after.to_string(),
        }
    }

    fn text(run: &str) -> Segment {
        Segment::Text(run.to_string())
    }

    /// The characters of the text runs (the blank not counted).
    fn text_chars(segments: &[Segment]) -> usize {
        segments
            .iter()
            .map(|segment| match segment {
                Segment::Text(run) => run.chars().count(),
                Segment::Blank => 0,
            })
            .sum()
    }

    fn blanks(segments: &[Segment]) -> usize {
        segments
            .iter()
            .filter(|segment| **segment == Segment::Blank)
            .count()
    }

    fn joined(segments: &[Segment]) -> String {
        segments
            .iter()
            .map(|segment| match segment {
                Segment::Text(run) => run.as_str(),
                Segment::Blank => "___",
            })
            .collect()
    }

    #[test]
    fn a_short_line_is_unchanged() {
        let line = blank("evaṃ ", "me", " sutaṃ");
        assert_eq!(
            context_window(&line, MAX_CHARS),
            vec![text("evaṃ "), Segment::Blank, text(" sutaṃ")]
        );
    }

    #[test]
    fn a_line_of_exactly_the_limit_is_unchanged() {
        let before = "a".repeat(40);
        let after = "b".repeat(77);
        let line = blank(&format!("{before} "), "x", &format!(" {after}"));
        // 41 + 1 + 78 = 120 characters of the line.
        let segments = context_window(&line, 120);
        assert_eq!(
            segments,
            vec![
                text(&format!("{before} ")),
                Segment::Blank,
                text(&format!(" {after}"))
            ]
        );
    }

    #[test]
    fn a_long_line_is_windowed_on_both_sides() {
        let before = "alpha beta gamma ".repeat(20);
        let after = " delta epsilon zeta".repeat(20);
        let line = blank(&before, "kho", &after);
        let segments = context_window(&line, 60);
        assert_eq!(blanks(&segments), 1);
        let Segment::Text(first) = &segments[0] else {
            panic!("the window starts with a text run: {segments:?}")
        };
        let Some(Segment::Text(last)) = segments.last() else {
            panic!("the window ends with a text run: {segments:?}")
        };
        assert!(first.starts_with('…'), "left ellipsis: {first:?}");
        assert!(last.ends_with('…'), "right ellipsis: {last:?}");
        let ellipses = 2;
        assert!(
            text_chars(&segments) <= 60 + ellipses,
            "{} chars in {segments:?}",
            text_chars(&segments)
        );
        // Both sides keep some context.
        assert!(first.chars().count() > 5, "{first:?}");
        assert!(last.chars().count() > 5, "{last:?}");
    }

    #[test]
    fn an_answer_longer_than_the_limit_still_shows_its_blank_once() {
        let answer = "x".repeat(30);
        let line = blank(
            &"alpha beta gamma ".repeat(5),
            &answer,
            &" delta epsilon zeta".repeat(5),
        );
        let max = 20;
        let segments = context_window(&line, max);
        assert_eq!(blanks(&segments), 1, "{segments:?}");
        // The blank weighs the whole answer: no room is left for context
        // but the two ellipses.
        let ellipses = 2;
        assert!(
            text_chars(&segments) <= max.saturating_sub(answer.chars().count()) + ellipses,
            "{} chars in {segments:?}",
            text_chars(&segments)
        );
    }

    #[test]
    fn the_window_keeps_the_blank_whatever_the_limit() {
        let line = blank(&"x ".repeat(100), "kho", &" y".repeat(100));
        for max in [0, 1, 3, 4, 10, 50, 120] {
            let segments = context_window(&line, max);
            assert!(blanks(&segments) >= 1, "max {max}: {segments:?}");
        }
    }

    #[test]
    fn a_blank_at_the_start_has_no_left_ellipsis() {
        let after = format!(" {}", "word ".repeat(60));
        let line = blank("", "kho", &after);
        let segments = context_window(&line, 40);
        assert_eq!(segments[0], Segment::Blank);
        let Some(Segment::Text(last)) = segments.last() else {
            panic!("a text run follows the blank: {segments:?}")
        };
        assert!(last.ends_with('…'), "{last:?}");
        assert!(text_chars(&segments) <= 40 + 1);
    }

    #[test]
    fn a_blank_at_the_end_has_no_right_ellipsis() {
        let before = format!("{} ", "word ".repeat(60));
        let line = blank(&before, "kho", "");
        let segments = context_window(&line, 40);
        assert_eq!(segments.last(), Some(&Segment::Blank));
        let Segment::Text(first) = &segments[0] else {
            panic!("a text run precedes the blank: {segments:?}")
        };
        assert!(first.starts_with('…'), "{first:?}");
        assert!(text_chars(&segments) <= 40 + 1);
    }

    #[test]
    fn a_side_kept_whole_has_no_ellipsis() {
        let after = " delta".repeat(60);
        let line = blank("evaṃ ", "me", &after);
        let segments = context_window(&line, 50);
        assert_eq!(segments[0], text("evaṃ "));
        let Some(Segment::Text(last)) = segments.last() else {
            panic!("{segments:?}")
        };
        assert!(last.ends_with('…'));
        assert!(!joined(&segments).starts_with('…'));
    }

    #[test]
    fn the_answer_twice_is_blanked_both_times() {
        let line = blank("sabbe saṅkhārā ", "ca", " dhammā ca nibbānaṃ");
        assert_eq!(
            context_window(&line, MAX_CHARS),
            vec![
                text("sabbe saṅkhārā "),
                Segment::Blank,
                text(" dhammā "),
                Segment::Blank,
                text(" nibbānaṃ"),
            ]
        );
    }

    #[test]
    fn an_occurrence_before_the_blank_is_blanked_too() {
        let line = blank("ca dhammā ", "ca", " nibbānaṃ");
        assert_eq!(
            context_window(&line, MAX_CHARS),
            vec![
                Segment::Blank,
                text(" dhammā "),
                Segment::Blank,
                text(" nibbānaṃ")
            ]
        );
    }

    #[test]
    fn the_answer_inside_a_longer_word_is_not_blanked() {
        let line = blank("kho ", "ca", " cakkhu na vācā");
        assert_eq!(
            context_window(&line, MAX_CHARS),
            vec![text("kho "), Segment::Blank, text(" cakkhu na vācā")]
        );
    }

    #[test]
    fn the_match_ignores_case_and_keeps_diacritics() {
        let line = blank("Evaṃ me ", "evaṃ", " sutaṃ; evam eva, EVAṂ.");
        let segments = context_window(&line, MAX_CHARS);
        // "Evaṃ" and "EVAṂ" are the answer; "evam" has no ṃ: kept.
        assert_eq!(blanks(&segments), 3, "{segments:?}");
        assert!(joined(&segments).contains("evam eva"));
    }

    #[test]
    fn a_word_at_the_end_of_the_line_after_punctuation_is_blanked() {
        let line = blank("saṅkhārā ", "aniccā", " — aniccā'ti, (aniccā).");
        let segments = context_window(&line, MAX_CHARS);
        assert_eq!(blanks(&segments), 3, "{segments:?}");
    }

    #[test]
    fn diacritics_count_as_one_character_each() {
        // 30 letters, each two bytes: 60 bytes, 30 chars.
        let word = "ā".repeat(30);
        let line = blank(&format!("{word} "), "ṃ", &format!(" {word}"));
        let segments = context_window(&line, 70);
        assert_eq!(text_chars(&segments), 62, "whole: 31 + 31 chars");
        assert_eq!(blanks(&segments), 1);
    }

    #[test]
    fn an_empty_answer_still_gives_one_blank_and_loses_no_text() {
        let line = blank("a ", "", " b");
        assert_eq!(
            context_window(&line, MAX_CHARS),
            vec![text("a "), Segment::Blank, text(" b")]
        );
        assert_eq!(
            context_window(&blank("", "", ""), MAX_CHARS),
            vec![Segment::Blank]
        );
    }

    #[test]
    fn a_window_never_splits_a_character() {
        // Cutting at any limit leaves valid text (a panic on a byte slice
        // would fail here) and the blank in view.
        let line = blank(&"āīū ṅñṭ ".repeat(30), "ṃ", &" ḍḷṣ ṛ".repeat(30));
        for max in 1..=80 {
            let segments = context_window(&line, max);
            assert!(blanks(&segments) >= 1, "max {max}");
            assert!(
                text_chars(&segments) <= max + 2,
                "max {max}: {} chars",
                text_chars(&segments)
            );
        }
    }

    #[test]
    fn a_window_does_not_start_in_the_middle_of_a_word() {
        let line = blank(&"abcdefgh ".repeat(30), "kho", &" ijklmnop".repeat(30));
        let segments = context_window(&line, 45);
        let Segment::Text(first) = &segments[0] else {
            panic!("{segments:?}")
        };
        let rest = first.strip_prefix('…').unwrap();
        assert!(rest.starts_with("abcdefgh"), "{first:?}");
        let Some(Segment::Text(last)) = segments.last() else {
            panic!()
        };
        let rest = last.strip_suffix('…').unwrap();
        assert!(rest.ends_with("ijklmnop"), "{last:?}");
    }

    #[test]
    fn the_whole_fixture_deck_keeps_its_blank_in_view() {
        let mut checked = 0;
        for term in &crate::deck::deck().terms {
            let Some(blank) = term.example.as_ref().and_then(|e| e.blank.as_ref()) else {
                continue;
            };
            let segments = context_window(blank, MAX_CHARS);
            assert!(blanks(&segments) >= 1, "{}: {segments:?}", term.id);
            assert!(
                text_chars(&segments) <= MAX_CHARS + 2,
                "{}: {} chars",
                term.id,
                text_chars(&segments)
            );
            // The answer is never left readable at the blank's place.
            checked += 1;
        }
        assert!(checked >= 40, "{checked} examples with a blank");
    }
}
