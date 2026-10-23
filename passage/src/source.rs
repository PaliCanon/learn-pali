//! A DPD example's `source` (`SN22.87`, `DHP127`, `TH259`) as the
//! SuttaCentral uid the PaliCanon API serves it under (`sn22.87`,
//! `dhp116-128`, `thag17.2`).
//!
//! The DPD numbers some books its own way: the Dhammapada and the second
//! book of the Aṅguttara by verse or sutta where SuttaCentral groups them in
//! ranges (`dhp116-128`, `an2.130-140`); the Sutta Nipāta, the Theragāthā
//! and the Therīgāthā by a running count of their poems where SuttaCentral
//! numbers them within chapters. The tables below carry those groupings;
//! each uid they produce was checked against the API (2026-10-11).

/// The Dhammapada's chapters, as SuttaCentral's verse ranges.
const DHP: [(u32, u32); 26] = [
    (1, 20),
    (21, 32),
    (33, 43),
    (44, 59),
    (60, 75),
    (76, 89),
    (90, 99),
    (100, 115),
    (116, 128),
    (129, 145),
    (146, 156),
    (157, 166),
    (167, 178),
    (179, 196),
    (197, 208),
    (209, 220),
    (221, 234),
    (235, 255),
    (256, 272),
    (273, 289),
    (290, 305),
    (306, 319),
    (320, 333),
    (334, 359),
    (360, 382),
    (383, 423),
];

/// The Book of the Twos (AN 2), as SuttaCentral's sutta ranges.
const AN2: [(u32, u32); 19] = [
    (1, 10),
    (11, 20),
    (21, 31),
    (32, 41),
    (42, 51),
    (52, 63),
    (64, 76),
    (77, 86),
    (87, 97),
    (98, 117),
    (118, 129),
    (130, 140),
    (141, 150),
    (151, 162),
    (163, 179),
    (180, 229),
    (230, 279),
    (280, 309),
    (310, 479),
];

/// Poems per chapter of the Sutta Nipāta (73).
const SNP: [u32; 5] = [12, 14, 12, 16, 19];
/// Poems per chapter of the Theragāthā (264).
const THAG: [u32; 21] = [
    120, 49, 16, 12, 12, 14, 5, 3, 1, 7, 1, 2, 1, 2, 2, 10, 3, 1, 1, 1, 1,
];
/// Poems per chapter of the Therīgāthā (73).
const THIG: [u32; 16] = [18, 10, 8, 1, 12, 8, 3, 1, 1, 1, 1, 1, 5, 1, 1, 1];

/// The uid of a DPD source, or `None` when the book is not mapped or the
/// number is out of its range.
pub fn source_uid(source: &str) -> Option<String> {
    let source = source.trim();
    let (book, number) = source.split_at(source.find(|c: char| c.is_ascii_digit())?);
    let mut parts = number.split('.');
    let first: u32 = parts.next()?.parse().ok()?;
    let second: Option<u32> = match parts.next() {
        Some(part) => Some(part.parse().ok()?),
        None => None,
    };
    match book {
        "DN" | "MN" => Some(format!("{}{first}", book.to_lowercase())),
        "AN" if first == 2 => in_range("an2.", &AN2, second?),
        "SN" | "AN" => Some(format!("{}{first}.{}", book.to_lowercase(), second?)),
        "DHP" => in_range("dhp", &DHP, first),
        "SNP" => in_chapters("snp", &SNP, first),
        "TH" => in_chapters("thag", &THAG, first),
        "THI" => in_chapters("thig", &THIG, first),
        "UD" => (1..=80)
            .contains(&first)
            .then(|| format!("ud{}.{}", (first - 1) / 10 + 1, (first - 1) % 10 + 1)),
        _ => None,
    }
}

/// The range of `table` holding `n`, as `{prefix}{first}-{last}`.
fn in_range(prefix: &str, table: &[(u32, u32)], n: u32) -> Option<String> {
    table
        .iter()
        .find(|(first, last)| (*first..=*last).contains(&n))
        .map(|(first, last)| format!("{prefix}{first}-{last}"))
}

/// The `n`th poem (from 1) of a book whose chapters hold `sizes` poems, as
/// `{book}{chapter}.{poem}`.
fn in_chapters(book: &str, sizes: &[u32], n: u32) -> Option<String> {
    let mut left = n.checked_sub(1)?;
    for (chapter, size) in sizes.iter().enumerate() {
        if left < *size {
            return Some(format!("{book}{}.{}", chapter + 1, left + 1));
        }
        left -= size;
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    fn uid(source: &str) -> Option<String> {
        source_uid(source)
    }

    #[test]
    fn suttas_numbered_alike_keep_their_number() {
        assert_eq!(uid("SN22.87").as_deref(), Some("sn22.87"));
        assert_eq!(uid("MN10").as_deref(), Some("mn10"));
        assert_eq!(uid("AN4.1").as_deref(), Some("an4.1"));
        assert_eq!(uid("AN10.48").as_deref(), Some("an10.48"));
        assert_eq!(uid("DN22").as_deref(), Some("dn22"));
    }

    #[test]
    fn a_section_inside_a_long_sutta_is_dropped() {
        assert_eq!(uid("DN16.36").as_deref(), Some("dn16"));
        assert_eq!(uid("DN22.8").as_deref(), Some("dn22"));
        assert_eq!(uid("MN2.2").as_deref(), Some("mn2"));
    }

    #[test]
    fn a_dhammapada_verse_gives_its_chapter_range() {
        for (verse, want) in [
            ("DHP1", "dhp1-20"),
            ("DHP20", "dhp1-20"),
            ("DHP21", "dhp21-32"),
            ("DHP127", "dhp116-128"),
            ("DHP128", "dhp116-128"),
            ("DHP129", "dhp129-145"),
            ("DHP382", "dhp360-382"),
            ("DHP383", "dhp383-423"),
            ("DHP423", "dhp383-423"),
        ] {
            assert_eq!(uid(verse).as_deref(), Some(want), "{verse}");
        }
        assert_eq!(uid("DHP0"), None);
        assert_eq!(uid("DHP424"), None);
    }

    #[test]
    fn the_book_of_twos_gives_its_range() {
        assert_eq!(uid("AN2.1").as_deref(), Some("an2.1-10"));
        assert_eq!(uid("AN2.135").as_deref(), Some("an2.130-140"));
        assert_eq!(uid("AN2.479").as_deref(), Some("an2.310-479"));
        assert_eq!(uid("AN2.480"), None);
    }

    #[test]
    fn a_running_poem_count_gives_chapter_and_poem() {
        for (source, want) in [
            ("SNP1", "snp1.1"),
            ("SNP4", "snp1.4"),
            ("SNP12", "snp1.12"),
            ("SNP13", "snp2.1"),
            ("SNP40", "snp4.2"),
            ("SNP56", "snp5.2"),
            ("SNP68", "snp5.14"),
            ("SNP73", "snp5.19"),
            ("TH1", "thag1.1"),
            ("TH120", "thag1.120"),
            ("TH121", "thag2.1"),
            ("TH155", "thag2.35"),
            ("TH259", "thag17.2"),
            ("TH264", "thag21.1"),
            ("THI1", "thig1.1"),
            ("THI71", "thig14.1"),
            ("THI73", "thig16.1"),
            ("UD1", "ud1.1"),
            ("UD45", "ud5.5"),
            ("UD80", "ud8.10"),
        ] {
            assert_eq!(uid(source).as_deref(), Some(want), "{source}");
        }
        for past in ["SNP74", "TH265", "THI74", "UD81", "TH0"] {
            assert_eq!(uid(past), None, "{past}");
        }
    }

    #[test]
    fn an_unknown_book_or_a_malformed_source_maps_to_nothing() {
        for source in ["KHP5", "VIN1", "", "SN", "SN22", "AN", "22.87", "MNx"] {
            assert_eq!(uid(source), None, "{source:?}");
        }
    }
}
