//! Parsing the DPD inflection tables.
//!
//! A headword's `inflections_html` holds one `<table class='inflection'>`
//! whose cells look like `<td title='fem acc pl'>sat<b>iyo</b><br>sat<b>ī</b></td>`:
//! the `title` is the grammatical label, the cell text between the `<br>`s
//! is one value each (stem plus bold ending, markup removed). A value
//! wrapped in `<span class='gray'>` is one the DPD greys (rare).
//!
//! The scan is hand-written (no HTML crate): the layout is machine
//! generated and fixed, and anything that departs from it is a *warning*,
//! never a guess. A titled cell with no value is not a departure: the DPD
//! leaves such cells in its tables, so it is data, counted apart.

use learn_core::deck::Form;

/// What a headword's table gave: its forms and what looked wrong.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Parsed {
    /// The labelled forms, in order of first appearance of the label.
    pub forms: Vec<Form>,
    /// What departed from the expected layout (unknown title, missing
    /// table, unterminated cell); `build-deck --strict` fails on any.
    pub warnings: Vec<String>,
    /// Cells with a title and no value: skipped and counted, never a
    /// warning (ruling 2810).
    pub empty_cells: usize,
}

/// Parses one headword's `inflections_html`.
///
/// Rules: an empty string is an indeclinable (no forms, no warning); a
/// non-empty string without the inflection table, a cell with a title but no
/// token outside the DPD's list are warnings. A cell with a title but no value
/// is skipped and counted in `empty_cells`, not a warning. Padding cells (empty title) and the `in comps` cell (the compound stem,
/// not a form of the word) are skipped silently. Grey is read **per value**:
/// the same label in several cells is grouped (values appended); a label is
/// `rare` when it has no plain value, and the grey values of a label that
/// has a plain one are dropped.
pub fn parse_inflections(html: &str) -> Parsed {
    let mut parsed = Parsed::default();
    if html.trim().is_empty() {
        return parsed;
    }
    if !html.contains("<table class='inflection'") {
        parsed
            .warnings
            .push("no inflection table in a non-empty inflections_html".to_string());
        return parsed;
    }
    let mut groups: Vec<Group> = Vec::new();
    let mut rest = html;
    while let Some(start) = rest.find("<td") {
        rest = &rest[start + 3..];
        let Some(cell) = take_cell(rest) else {
            parsed.warnings.push("unterminated <td> cell".to_string());
            break;
        };
        rest = cell.after;
        read_cell(&cell, &mut groups, &mut parsed);
    }
    parsed.forms = groups.into_iter().map(Group::into_form).collect();
    parsed
}

/// The DPD's label tokens (29 with `in` and `comps`): a title made of any
/// other token is unknown.
const TOKENS: [&str; 29] = [
    "nom", "acc", "instr", "dat", "abl", "gen", "loc", "voc", "sg", "pl", "dual", "masc", "fem",
    "nt", "x", "1st", "2nd", "3rd", "pr", "imp", "opt", "fut", "aor", "imperf", "cond", "perf",
    "reflx", "in", "comps",
];

/// One `<td …>…</td>`: the text of its `title`, its inner markup, and what
/// follows it.
struct Cell<'a> {
    title: &'a str,
    inner: &'a str,
    after: &'a str,
}

/// The values of one label, plain and grey, as the cells give them.
struct Group {
    label: String,
    plain: Vec<String>,
    grey: Vec<String>,
}

impl Group {
    /// A label with a plain value is regular and keeps only those; one
    /// with none is rare and keeps its grey values.
    fn into_form(self) -> Form {
        let rare = self.plain.is_empty();
        Form {
            label: self.label,
            values: if rare { self.grey } else { self.plain },
            rare,
        }
    }
}

/// Reads a cell from just after `<td`; `None` when it is not closed.
fn take_cell(rest: &str) -> Option<Cell<'_>> {
    let open_end = rest.find('>')?;
    let attributes = &rest[..open_end];
    let body = &rest[open_end + 1..];
    let close = body.find("</td>")?;
    Some(Cell {
        title: title_of(attributes),
        inner: &body[..close],
        after: &body[close + "</td>".len()..],
    })
}

/// The value of the `title='…'` attribute (`""` when absent).
fn title_of(attributes: &str) -> &str {
    let Some(at) = attributes.find("title=") else {
        return "";
    };
    let value = &attributes[at + "title=".len()..];
    let Some(quote) = value.chars().next().filter(|c| matches!(c, '\'' | '"')) else {
        return "";
    };
    let value = &value[1..];
    value.find(quote).map_or("", |end| &value[..end])
}

/// Adds a cell's values to its label's group, records a warning for an
/// unknown title, or counts the cell when it has no value.
fn read_cell(cell: &Cell<'_>, groups: &mut Vec<Group>, parsed: &mut Parsed) {
    let title = cell.title.trim();
    // Padding, and the compound stem: not forms of the word.
    if title.is_empty() || title == "in comps" {
        return;
    }
    if let Some(token) = title.split_whitespace().find(|t| !TOKENS.contains(t)) {
        parsed
            .warnings
            .push(format!("unknown title '{title}' (token '{token}')"));
    }
    let mut plain = Vec::new();
    let mut grey = Vec::new();
    for part in cell.inner.split("<br>") {
        let value = strip_tags(part);
        if value.is_empty() {
            continue;
        }
        if part.trim_start().starts_with("<span class='gray'>") {
            grey.push(value);
        } else {
            plain.push(value);
        }
    }
    if plain.is_empty() && grey.is_empty() {
        parsed.empty_cells += 1;
        return;
    }
    let at = groups
        .iter()
        .position(|g| g.label == title)
        .unwrap_or_else(|| {
            groups.push(Group {
                label: title.to_string(),
                plain: Vec::new(),
                grey: Vec::new(),
            });
            groups.len() - 1
        });
    groups[at].plain.extend(plain);
    groups[at].grey.extend(grey);
}

/// The text of some markup: every `<…>` removed, the rest trimmed.
fn strip_tags(markup: &str) -> String {
    let mut text = String::new();
    let mut in_tag = false;
    for c in markup.chars() {
        match c {
            '<' => in_tag = true,
            '>' => in_tag = false,
            _ if !in_tag => text.push(c),
            _ => {}
        }
    }
    text.trim().to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn form(label: &str, values: &[&str], rare: bool) -> Form {
        Form {
            label: label.to_string(),
            values: values.iter().map(|v| (*v).to_string()).collect(),
            rare,
        }
    }

    fn table(cells: &str) -> String {
        format!(
            "<p class='heading'><b>sati ¹·¹</b></p><table class='inflection'><tr>{cells}</tr></table>"
        )
    }

    #[test]
    fn the_real_noun_cells() {
        let html = table(
            "<td title='fem acc pl'>sat<b>iyo</b><br>sat<b>ī</b></td>\
             <td title='fem instr pl'><span class='gray'>sat<b>ibhi</b></span><br><span class='gray'>sat<b>ībhi</b></span><br>sat<b>īhi</b></td>\
             <td title='in comps'>sat<b>i</b></td><td title=''></td>",
        );
        let parsed = parse_inflections(&html);
        assert_eq!(parsed.warnings, Vec::<String>::new());
        assert_eq!(
            parsed.forms,
            vec![
                form("fem acc pl", &["satiyo", "satī"], false),
                form("fem instr pl", &["satīhi"], false),
            ]
        );
    }

    #[test]
    fn the_real_verb_cells() {
        let html = table(
            "<td title='reflx pr 3rd pl'>gacch<b>ante</b><br>gacch<b>are</b></td>\
             <td title='reflx imp 3rd pl'>gacch<b>antaṃ</b><br><span class='gray'>gacch<b>aruṃ</b></span></td>",
        );
        let parsed = parse_inflections(&html);
        assert_eq!(parsed.warnings, Vec::<String>::new());
        assert_eq!(
            parsed.forms,
            vec![
                form("reflx pr 3rd pl", &["gacchante", "gacchare"], false),
                form("reflx imp 3rd pl", &["gacchantaṃ"], false),
            ]
        );
    }

    #[test]
    fn a_wholly_grey_cell_is_rare_and_keeps_its_values() {
        let html = table(
            "<td title='fem loc pl'><span class='gray'>sat<b>isu</b></span><br><span class='gray'>sat<b>īsu</b></span></td>",
        );
        let parsed = parse_inflections(&html);
        assert_eq!(parsed.warnings, Vec::<String>::new());
        assert_eq!(
            parsed.forms,
            vec![form("fem loc pl", &["satisu", "satīsu"], true)]
        );
    }

    #[test]
    fn a_repeated_label_is_grouped() {
        let html = table(
            "<td title='masc nom sg'>a<b>x</b></td><td title='masc nom pl'>a<b>y</b></td>\
             <td title='masc nom sg'>a<b>z</b></td>",
        );
        let parsed = parse_inflections(&html);
        assert_eq!(parsed.warnings, Vec::<String>::new());
        assert_eq!(
            parsed.forms,
            vec![
                form("masc nom sg", &["ax", "az"], false),
                form("masc nom pl", &["ay"], false),
            ]
        );
    }

    #[test]
    fn a_repeated_label_grey_then_plain_is_not_rare() {
        let html = table(
            "<td title='nt voc sg'><span class='gray'>a<b>x</b></span></td><td title='nt voc sg'>a<b>y</b></td>",
        );
        assert_eq!(
            parse_inflections(&html).forms,
            vec![form("nt voc sg", &["ay"], false)]
        );
    }

    #[test]
    fn padding_and_in_comps_cells_are_skipped_silently() {
        let html = table("<td title=''></td><td title='in comps'>sat<b>i</b></td>");
        assert_eq!(parse_inflections(&html), Parsed::default());
    }

    #[test]
    fn an_empty_cell_with_a_title_is_counted_not_a_warning() {
        let html = table(
            "<td title='fem nom sg'></td><td title='fem nom pl'>sat<b>iyo</b></td>\
             <td title='fem acc sg'></td>",
        );
        let parsed = parse_inflections(&html);
        assert_eq!(parsed.forms, vec![form("fem nom pl", &["satiyo"], false)]);
        assert_eq!(parsed.warnings, Vec::<String>::new());
        assert_eq!(parsed.empty_cells, 2);
    }

    #[test]
    fn padding_cells_are_not_counted_as_empty_cells() {
        let html = table("<td title=''></td><td title='in comps'></td>");
        assert_eq!(parse_inflections(&html).empty_cells, 0);
    }

    #[test]
    fn an_unknown_title_is_a_warning() {
        let html = table("<td title='fem foo sg'>sat<b>i</b></td>");
        let parsed = parse_inflections(&html);
        assert_eq!(parsed.warnings.len(), 1);
        assert!(parsed.warnings[0].contains("fem foo sg"), "{parsed:?}");
    }

    #[test]
    fn every_known_token_is_accepted() {
        let tokens = "nom acc instr dat abl gen loc voc sg pl dual masc fem nt x \
                      1st 2nd 3rd pr imp opt fut aor imperf cond perf reflx";
        for token in tokens.split(' ') {
            let html = table(&format!("<td title='{token}'>a<b>b</b></td>"));
            assert_eq!(
                parse_inflections(&html).warnings,
                Vec::<String>::new(),
                "{token}"
            );
        }
    }

    #[test]
    fn a_missing_table_is_a_warning() {
        let parsed = parse_inflections("<p class='heading'>nothing</p>");
        assert_eq!(parsed.forms, vec![]);
        assert_eq!(parsed.warnings.len(), 1);
    }

    #[test]
    fn an_empty_string_is_an_indeclinable() {
        assert_eq!(parse_inflections(""), Parsed::default());
        assert_eq!(parse_inflections("  \n"), Parsed::default());
    }

    #[test]
    fn a_table_without_cells_gives_nothing_and_no_warning() {
        let parsed = parse_inflections("<table class='inflection'></table>");
        assert_eq!(parsed, Parsed::default());
    }

    #[test]
    fn header_cells_are_not_forms() {
        let html = "<table class='inflection'><tr><th></th><th>fem sg</th></tr>\
                    <tr><th>nom</th><td title='fem nom sg'>sat<b>i</b></td></tr></table>";
        assert_eq!(
            parse_inflections(html).forms,
            vec![form("fem nom sg", &["sati"], false)]
        );
    }

    #[test]
    fn a_cell_with_no_closing_tag_is_a_warning_not_a_panic() {
        let parsed = parse_inflections("<table class='inflection'><td title='fem nom sg'>sat<b>i");
        assert_eq!(parsed.warnings.len(), 1, "{parsed:?}");
    }
}
