//! The term list and what filters it, as the home shows them: the strip of
//! the six stages, compact, whose cells filter the list in place (each a
//! `<button aria-pressed>`; the pressed one pressed again shows every term;
//! the filter is the page's state, no URL, lost on reload),
//! the known terms in a region capped in height (a row opens its detail
//! inline, a disclosure, no route), and the terms the deck does not know,
//! listed apart and never counted.

use std::collections::BTreeMap;
use std::sync::{LazyLock, Mutex, PoisonError};

use learn_core::date::Date;
use learn_core::deck::Term;
use learn_core::facet::{Facet, FacetSet};
use learn_core::progress::State;
use learn_core::stage::Stage;
use leptos::prelude::*;
use leptos_i18n::td_string;
use palicanon_ui_lib::{
    Bucket, BucketStrip, Disclosed, Disclosure, DisclosureButton, PaliTerm, Size, Variant,
    language_signal,
};

use crate::catalogue::{Strings, strings};
use crate::deck::deck;
use crate::learn::stages::stage_label;
use crate::learn::words::date_words;

/// The six stages as a compact strip of counts that filters the list in
/// place: a cell sets `stage` to its stage, the pressed one clears it (the
/// pressed cell is the only sign of the filter).
#[component]
pub(crate) fn StageStrip(
    /// The known terms per stage, Day to Year.
    counts: [usize; 6],
    /// The filter in force, the page's state.
    stage: RwSignal<Option<Stage>>,
) -> impl IntoView {
    let t = strings();
    let on_select =
        Callback::new(move |index| stage.update(|current| *current = toggled(*current, index)));
    let selected = Signal::derive(move || stage.get().and_then(stage_index));
    view! {
        {move || {
            let cells: Vec<Bucket> = Stage::ALL
                .into_iter()
                .zip(counts)
                .map(|(stage, count)| Bucket {
                    label: stage_label(t, stage).to_owned(),
                    count,
                    to: None,
                })
                .collect();
            view! {
                <BucketStrip cells=cells size=Size::Sm on_select=on_select selected=selected />
            }
        }}
    }
}

/// The filter after the cell at `index` (its place in [`Stage::ALL`]) is
/// pressed: that stage, or none when it is the one in force; an index past
/// the six changes nothing.
pub(crate) fn toggled(current: Option<Stage>, index: usize) -> Option<Stage> {
    match Stage::ALL.get(index) {
        Some(&pressed) if current == Some(pressed) => None,
        Some(&pressed) => Some(pressed),
        None => current,
    }
}

/// The cell of `stage` in the strip: its place in [`Stage::ALL`].
fn stage_index(stage: Stage) -> Option<usize> {
    Stage::ALL.iter().position(|each| *each == stage)
}

/// The known terms of `progress`, of the stage in force only, as [`TermRows`]:
/// the list alone renders again when the filter changes.
#[component]
pub(crate) fn StageTerms(
    /// The stored progress.
    progress: State,
    /// The filter in force.
    #[prop(into)]
    stage: Signal<Option<Stage>>,
) -> impl IntoView {
    move || view! { <TermRows rows=known_rows(&progress, stage.get()) /> }
}

/// A known term and where it stands, ready to list.
#[derive(Clone)]
pub(crate) struct Row {
    /// The deck's term.
    term: &'static Term,
    /// Its stage.
    stage: Stage,
    /// The first day it is asked again.
    due: Date,
    /// How many times it was failed.
    lapses: u32,
    /// The facets passed that the deck still has.
    passed: FacetSet,
}

/// The known terms of `state` (those the deck has), only those of stage
/// `only` when given, the earliest due first, then by rank.
pub(crate) fn known_rows(state: &State, only: Option<Stage>) -> Vec<Row> {
    let mut rows: Vec<Row> = state
        .progress
        .iter()
        .filter(|progress| only.is_none_or(|only| progress.stage == only))
        .filter_map(|progress| {
            deck().term(&progress.term).map(|term| Row {
                term,
                stage: progress.stage,
                due: progress.due,
                lapses: progress.lapses,
                passed: State::effective_passed(progress, term),
            })
        })
        .collect();
    rows.sort_by_key(|row| (row.due, row.term.rank));
    rows
}

/// The terms of `state` the deck does not know, in file order, with their
/// stage.
pub(crate) fn orphan_rows(state: &State) -> Vec<(String, Stage)> {
    state
        .progress
        .iter()
        .filter(|progress| deck().term(&progress.term).is_none())
        .map(|progress| (progress.term.0.clone(), progress.stage))
        .collect()
}

/// The facet's name in the page's language.
fn facet_label(t: Strings, facet: Facet) -> &'static str {
    match facet {
        Facet::Meaning => td_string!(t.locale(), facet_meaning),
        Facet::Recall => td_string!(t.locale(), facet_recall),
        Facet::Forms => td_string!(t.locale(), facet_forms),
        Facet::Context => td_string!(t.locale(), facet_context),
    }
}

/// The facets passed, by name; a dash for none.
fn facets_text(t: Strings, passed: FacetSet) -> String {
    let names: Vec<&str> = passed.iter().map(|facet| facet_label(t, facet)).collect();
    if names.is_empty() {
        "—".to_owned()
    } else {
        names.join(", ")
    }
}

/// The element id of a term's detail, `pc-detail-<rank>` (a rank is unique
/// in a deck, a term id may hold spaces); interned, since a disclosure's
/// id is `&'static str`, so the leak is one string per rank.
fn detail_id(rank: u32) -> &'static str {
    static IDS: LazyLock<Mutex<BTreeMap<u32, &'static str>>> = LazyLock::new(Mutex::default);
    let mut ids = IDS.lock().unwrap_or_else(PoisonError::into_inner);
    ids.entry(rank)
        .or_insert_with(|| Box::leak(format!("pc-detail-{rank}").into_boxed_str()))
}

/// The term list: a row per known term, in a region capped at about five
/// rows at every width that scrolls (keyboard too, `tabindex="0"`), or 'No
/// terms here yet'.
#[component]
pub(crate) fn TermRows(
    /// The rows, already filtered and sorted.
    rows: Vec<Row>,
) -> impl IntoView {
    let t = strings();
    if rows.is_empty() {
        return view! { <p class="text-muted">{move || td_string!(t.locale(), progress_empty)}</p> }
        .into_any();
    }
    view! {
        <div
            data-term-list=""
            tabindex="0"
            role="region"
            aria-label=move || td_string!(t.locale(), progress_terms)
            class="max-h-[22rem] overflow-y-auto overscroll-contain pr-1"
        >
            <ul class="flex flex-col gap-2">
                {rows.into_iter().map(|row| view! { <TermRow row=row /> }).collect_view()}
            </ul>
        </div>
    }
    .into_any()
}

/// One term as ui-lib's `PaliTerm` in a `Tile` button that opens its
/// detail inline: the lemma in bold with its stage glued in parentheses and
/// the due date on line 1, the sense and the lapses on line 2. The stage,
/// the date and the lapses follow the language.
#[component]
fn TermRow(
    /// The term and where it stands.
    row: Row,
) -> impl IntoView {
    let t = strings();
    let language = language_signal();
    let detail = Disclosed::new(detail_id(row.term.rank), RwSignal::new(false));
    let (stage, due, lapses) = (row.stage, row.due, row.lapses);
    view! {
        <li class="flex flex-col gap-2">
            <DisclosureButton
                controls=detail
                variant=Variant::Tile
                layout="w-full text-left px-4 py-3"
            >
                <PaliTerm
                    lemma=row.term.lemma.clone()
                    gloss=row.term.meaning.en.clone()
                    tag=Signal::derive(move || stage_label(t, stage).to_owned())
                    top_end=move || view! { {move || date_words(language.get(), due)} }
                    bottom_end=move || {
                        view! {
                            {move || format!("{} {lapses}", td_string!(t.locale(), detail_lapses))}
                        }
                    }
                />
            </DisclosureButton>
            <TermDetail row=row open=detail />
        </li>
    }
}

/// The detail a row opens: stage, facets passed, lapses, due.
#[component]
fn TermDetail(
    /// The term and where it stands.
    row: Row,
    /// What shows and hides it.
    open: Disclosed,
) -> impl IntoView {
    let t = strings();
    let language = language_signal();
    let (stage, due, lapses, passed) = (row.stage, row.due, row.lapses, row.passed);
    view! {
        <div id=open.id() hidden=move || !open.is_open() class="px-4 pb-2 text-sm">
            <dl class="grid grid-cols-[auto_1fr] gap-x-4 gap-y-1">
                <dt class="text-muted">{move || td_string!(t.locale(), detail_stage)}</dt>
                <dd>{move || stage_label(t, stage)}</dd>
                <dt class="text-muted">{move || td_string!(t.locale(), detail_facets)}</dt>
                <dd>{move || facets_text(t, passed)}</dd>
                <dt class="text-muted">{move || td_string!(t.locale(), detail_lapses)}</dt>
                <dd>{lapses}</dd>
                <dt class="text-muted">{move || td_string!(t.locale(), detail_due)}</dt>
                <dd>{move || date_words(language.get(), due)}</dd>
            </dl>
        </div>
    }
}

/// The terms the deck does not know, apart from the list (and from the
/// counts): their id and stage. Nothing when there are none.
#[component]
pub(crate) fn Orphans(
    /// Each term id with its stage.
    orphans: Vec<(String, Stage)>,
) -> impl IntoView {
    let t = strings();
    (!orphans.is_empty()).then(|| {
        view! {
            <section class="flex flex-col gap-2">
                <h2 class="text-sm font-medium text-muted">
                    {move || td_string!(t.locale(), orphans_heading)}
                </h2>
                <ul class="flex flex-col gap-1 text-sm">
                    {orphans
                        .into_iter()
                        .map(|(id, stage)| {
                            view! {
                                <li>
                                    <span>{id}</span>
                                    " · "
                                    <span class="text-muted">{move || stage_label(t, stage)}</span>
                                </li>
                            }
                        })
                        .collect_view()}
                </ul>
            </section>
        }
    })
}

#[cfg(test)]
mod tests {
    use leptos_i18n::td_string;
    use palicanon_ui_lib::Language;

    use super::*;
    use crate::catalogue::Locale;
    use crate::test_support::{position, progress_fixture, tag_around};

    /// The text of `$key` in `$language`.
    macro_rules! word {
        ($language:expr, $key:ident) => {
            td_string!(Locale::from($language), $key)
        };
    }

    /// `view` rendered in `language`, the hydration markers removed.
    fn render<V: IntoView + 'static>(language: Language, view: impl FnOnce() -> V) -> String {
        Owner::new()
            .with(|| {
                palicanon_ui_lib::provide_language().set(language);
                view().to_html()
            })
            .replace("<!>", "")
    }

    /// The list of the fixture's known terms, of `stage` only when given.
    fn list(stage: Option<Stage>, language: Language) -> String {
        let rows = known_rows(&progress_fixture(), stage);
        render(language, move || view! { <TermRows rows=rows /> })
    }

    fn orphans(language: Language) -> String {
        let orphans = orphan_rows(&progress_fixture());
        render(language, move || view! { <Orphans orphans=orphans /> })
    }

    /// The strip with `stage` in force.
    fn strip(stage: Option<Stage>) -> String {
        render(Language::En, move || {
            view! { <StageStrip counts=[2, 1, 0, 0, 0, 0] stage=RwSignal::new(stage) /> }
        })
    }

    /// The open tag of the `<button>` whose label is `label`.
    fn cell<'a>(html: &'a str, label: &str) -> &'a str {
        let at = position(html, &format!(">{label}<"));
        let start = html[..at].rfind("<button").expect(html);
        &html[start..start + position(&html[start..], ">")]
    }

    #[test]
    fn the_strip_has_six_compact_cells_that_filter_in_place() {
        let html = strip(None);
        assert!(!html.contains("Time until the next review"), "{html}");
        assert!(!html.contains("<h2"), "no heading, HC: {html}");
        assert!(!html.contains("href="), "no link, no URL: {html}");
        assert_eq!(html.matches("<button").count(), 6, "{html}");
        for (label, count) in [
            ("Daily", 2),
            ("Weekly", 1),
            ("Monthly", 0),
            ("Quarterly", 0),
            ("Half-yearly", 0),
            ("Yearly", 0),
        ] {
            let button = cell(&html, label);
            assert!(button.contains(r#"type="button""#), "{button}");
            assert!(button.contains(r#"aria-pressed="false""#), "{button}");
            let counted = format!(
                r#">{label}</span><span class="order-first text-base font-semibold text-heading">{count}</span>"#
            );
            assert!(html.contains(&counted), "compact: {counted}: {html}");
        }
        assert!(!html.contains("All terms"), "no filter line: {html}");
    }

    #[test]
    fn the_stage_in_force_is_the_pressed_cell_with_no_filter_line() {
        let html = strip(Some(Stage::Week));
        assert!(
            cell(&html, "Weekly").contains(r#"aria-pressed="true""#),
            "{html}"
        );
        assert_eq!(html.matches(r#"aria-pressed="true""#).count(), 1, "{html}");
        assert!(
            cell(&html, "Daily").contains(r#"aria-pressed="false""#),
            "{html}"
        );
        assert!(!html.contains("Stage: Weekly"), "{html}");
        assert!(!html.contains("All terms"), "{html}");
        assert!(!html.contains("href="), "{html}");
    }

    #[test]
    fn pressing_a_cell_selects_its_stage() {
        assert_eq!(toggled(None, 1), Some(Stage::Week));
        assert_eq!(toggled(Some(Stage::Day), 5), Some(Stage::Year));
        assert_eq!(toggled(None, 4), Some(Stage::HalfYear));
    }

    #[test]
    fn pressing_the_pressed_cell_shows_every_term_again() {
        for (index, stage) in Stage::ALL.into_iter().enumerate() {
            assert_eq!(toggled(Some(stage), index), None, "{stage:?}");
        }
    }

    #[test]
    fn a_cell_past_the_six_changes_nothing() {
        assert_eq!(toggled(Some(Stage::Month), 6), Some(Stage::Month));
        assert_eq!(toggled(None, usize::MAX), None);
    }

    #[test]
    fn the_stage_terms_follow_the_filter_signal() {
        let stage = Some(Stage::Week);
        let html = render(Language::En, move || {
            view! { <StageTerms progress=progress_fixture() stage=RwSignal::new(stage) /> }
        });
        assert_eq!(html.matches(r#"aria-expanded=""#).count(), 1, "{html}");
        assert!(html.contains(">kho</span>"), "{html}");
        assert!(!html.contains(">ca</span>"), "{html}");
        let html = render(Language::En, || {
            view! { <StageTerms progress=progress_fixture() stage=RwSignal::new(None) /> }
        });
        assert_eq!(html.matches(r#"aria-expanded=""#).count(), 3, "{html}");
    }

    #[test]
    fn unfiltered_the_list_has_every_known_term_by_due_then_rank() {
        let html = list(None, Language::En);
        assert_eq!(html.matches(r#"aria-expanded=""#).count(), 3, "{html}");
        let (ca, va, kho) = (
            position(&html, ">ca</span>"),
            position(&html, ">vā</span>"),
            position(&html, ">kho</span>"),
        );
        assert!(ca < va && va < kho, "due today first, by rank: {html}");
        assert!(!html.contains("ghost 1"), "no orphan in the list: {html}");
    }

    #[test]
    fn a_stage_filter_keeps_that_stage_alone() {
        let html = list(Some(Stage::Week), Language::En);
        assert_eq!(html.matches(r#"aria-expanded=""#).count(), 1, "{html}");
        assert!(html.contains(">kho</span>"), "{html}");
        assert!(!html.contains(">ca</span>"), "{html}");
    }

    #[test]
    fn a_stage_with_no_term_says_so() {
        let html = list(Some(Stage::Year), Language::En);
        assert_eq!(html.matches(r#"aria-expanded=""#).count(), 0, "{html}");
        assert!(html.contains("No terms here yet"), "{html}");
    }

    /// The `<button>` of the row whose lemma is `lemma`, from its opening
    /// tag to its closing one.
    fn row_button<'a>(html: &'a str, lemma: &str) -> &'a str {
        let at = position(html, &format!(">{lemma}</span>"));
        let start = html[..at].rfind("<button").expect("a button opens the row");
        let row = &html[start..];
        &row[..position(row, "</button>")]
    }

    #[test]
    fn a_row_is_ui_libs_pali_term_inside_a_tile_button() {
        let html = list(None, Language::En);
        let row = row_button(&html, "kho");
        let button = &row[..position(row, ">")];
        for class in [
            "rounded-xl",
            "border-line",
            "bg-chip",
            "hover:border-gold/60",
            "w-full",
            "text-left",
            "px-4",
            "py-3",
        ] {
            assert!(
                button.split(['"', ' ']).any(|c| c == class),
                "{class}: {button}"
            );
        }
        let grid = tag_around(row, "grid-cols-[minmax(0,1fr)_auto]");
        assert!(grid.starts_with("<div"), "PaliTerm's grid: {grid}");
        let strong = position(row, r#"<strong class="font-semibold">"#);
        let lemma = position(row, ">kho</span>");
        let close = position(row, "</strong>");
        assert!(strong < lemma && lemma < close, "the lemma is bold: {row}");
        assert!(row.contains(r#"lang="pi""#), "{row}");
    }

    #[test]
    fn a_row_reads_lemma_stage_and_date_then_sense_and_lapses() {
        let html = list(None, Language::En);
        let row = row_button(&html, "kho");
        let order = [
            ">kho</span>",
            "(Weekly)",
            "12 October 2026",
            "indeed; surely; certainly; truly",
            "Lapses 2",
        ]
        .map(|text| position(row, text));
        assert!(order.is_sorted(), "{order:?}: {row}");
        let row = row_button(&html, "ca");
        for text in ["(Daily)", "5 October 2026", "Lapses 0"] {
            assert!(row.contains(text), "{text}: {row}");
        }
    }

    #[test]
    fn a_row_shows_its_date_at_every_width_and_cuts_only_the_sense() {
        let html = list(None, Language::En);
        let row = row_button(&html, "kho");
        assert!(!row.contains("hidden sm:inline"), "{row}");
        let date = tag_around(row, "12 October 2026");
        assert!(!date.contains("hidden"), "every width: {date}");
        assert!(date.contains("whitespace-nowrap"), "PaliTerm's end: {date}");
        let sense = tag_around(row, "indeed; surely");
        assert!(sense.contains("truncate"), "the sense is cut: {sense}");
        let button = &row[..position(row, ">")];
        assert!(!button.contains("flex-wrap"), "{button}");
    }

    #[test]
    fn a_rows_stage_tag_follows_the_language() {
        let html = list(None, Language::Fr);
        let row = row_button(&html, "ca");
        assert!(row.contains("(Quotidien)"), "{row}");
        assert!(row.contains("Oublis 0"), "{row}");
        let row = row_button(&html, "kho");
        assert!(row.contains("(Hebdomadaire)"), "{row}");
        assert!(row.contains("12 octobre 2026"), "{row}");
    }

    #[test]
    fn a_row_opens_its_detail_inline() {
        let html = list(None, Language::En);
        let button = tag_around(&html, r#"aria-controls="pc-detail-1""#);
        assert!(button.contains(r#"aria-expanded="false""#), "{button}");
        let detail_tag = tag_around(&html, r#"id="pc-detail-1""#);
        assert!(detail_tag.contains("hidden"), "closed: {detail_tag}");
        let detail = &html[position(&html, r#"id="pc-detail-1""#)..];
        let detail = &detail[..position(detail, "</div>")];
        for text in [
            "Facets passed",
            "Meaning, Recall",
            "Lapses",
            "Due",
            "12 October 2026",
            "Stage",
        ] {
            assert!(detail.contains(text), "{text}: {detail}");
        }
    }

    #[test]
    fn each_row_has_its_own_detail_id() {
        let html = list(None, Language::En);
        for id in ["pc-detail-1", "pc-detail-2", "pc-detail-3"] {
            assert_eq!(html.matches(&format!(r#"id="{id}""#)).count(), 1, "{id}");
            assert_eq!(
                html.matches(&format!(r#"aria-controls="{id}""#)).count(),
                1,
                "{id}"
            );
        }
    }

    #[test]
    fn the_term_list_scrolls_inside_a_labelled_region_capped_in_height() {
        for (language, label) in [(Language::En, "Your terms"), (Language::Fr, "Vos termes")] {
            let html = list(None, language);
            let region = tag_around(&html, "data-term-list");
            for class in [
                "overflow-y-auto",
                "overscroll-contain",
                "max-h-[22rem]",
                "pr-1",
            ] {
                assert!(region.contains(class), "{class}: {region}");
            }
            assert!(!region.contains("sm:max-h-"), "every width: {region}");
            assert!(region.contains(r#"role="region""#), "{region}");
            assert!(region.contains(r#"tabindex="0""#), "{region}");
            assert!(
                region.contains(&format!(r#"aria-label="{label}""#)),
                "{region}"
            );
            let inside = &html[position(&html, "data-term-list")..];
            let list = inside.find('>').expect(inside) + 1;
            assert!(
                inside[list..].starts_with(r#"<ul class="flex flex-col gap-2">"#),
                "{inside}"
            );
            let end = inside.find("</ul></div>").expect(inside);
            assert!(inside[..end].contains("kho"), "the rows are inside: {html}");
        }
    }

    #[test]
    fn the_orphans_are_listed_apart_with_their_stage() {
        let html = orphans(Language::En);
        let orphans = &html[position(&html, "Not in this deck")..];
        let orphans = &orphans[..position(orphans, "</section>")];
        assert!(orphans.contains("ghost 1"), "{orphans}");
        assert!(orphans.contains("Quarterly"), "{orphans}");
    }

    #[test]
    fn no_orphan_no_orphan_section() {
        let html = render(Language::En, || view! { <Orphans orphans=Vec::new() /> });
        assert!(!html.contains("Not in this deck"), "{html}");
    }

    #[test]
    fn it_reads_in_french_and_vietnamese() {
        let html = list(None, Language::Fr);
        for text in ["Hebdomadaire", "Quotidien", "12 octobre 2026"] {
            assert!(html.contains(text), "{text}: {html}");
        }
        assert!(
            orphans(Language::Fr).contains(word!(Language::Fr, orphans_heading)),
            "fr orphans"
        );
        let html = list(None, Language::Vi);
        for text in ["Hằng tuần", "ngày 12 tháng 10 năm 2026"] {
            assert!(html.contains(text), "{text}: {html}");
        }
    }
}
