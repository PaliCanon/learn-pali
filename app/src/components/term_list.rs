//! The term list and what filters it, as the home shows them: the strip of
//! the six stages, three by three, whose cells filter the list in place
//! (each a `<button aria-pressed>`; the pressed one pressed again shows
//! every term; the filter is the page's state, no URL, lost on reload),
//! the known terms in a region capped in height (each a card that opens
//! its detail inside itself, a disclosure, no route), and the terms the
//! deck does not know, listed apart and never counted.

use std::collections::BTreeMap;
use std::sync::{LazyLock, Mutex, PoisonError};

use learn_core::date::Date;
use learn_core::deck::Term;
use learn_core::facet::{Facet, FacetSet, facets_of};
use learn_core::progress::State;
use learn_core::stage::Stage;
use leptos::prelude::*;
use leptos_i18n::td_string;
use palicanon_ui_lib::{
    Bucket, BucketStrip, Columns, Disclosed, Disclosure, DisclosureButton, PaliTerm, Size, Variant,
    language_signal,
};

use crate::catalogue::{Strings, strings};
use crate::deck::deck;
use crate::learn::stages::stage_label;
use crate::learn::words::date_words;

/// The six stages as a strip of counts, three by three at the default
/// size, that filters the list in place: a cell sets `stage` to its stage,
/// the pressed one clears it (the pressed cell is the only sign of the
/// filter).
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
                <BucketStrip
                    cells=cells
                    columns=Columns::Three
                    on_select=on_select
                    selected=selected
                />
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
    /// What sits just below the list (the home's '+'); none by default.
    #[prop(default = None)]
    add: Option<ViewFn>,
) -> impl IntoView {
    move || view! { <TermRows rows=known_rows(&progress, stage.get()) add=add.clone() /> }
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
    /// The facets the term can be asked.
    facets: FacetSet,
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
                facets: facets_of(term),
            })
        })
        .collect();
    rows.sort_by_key(|row| (row.due, row.term.rank));
    rows
}

/// The first day a known term of `state` is asked again: the earliest due
/// of the terms the deck has; none when no term is started.
pub(crate) fn next_review(state: &State) -> Option<Date> {
    known_rows(state, None).first().map(|row| row.due)
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

/// A closed card: the tile's resting surface, a hairline border.
const CARD_CLOSED: &str = "rounded-xl border border-line bg-chip hover:border-gold/60";

/// An open card: the same surface, the gold border (one border colour or
/// the other, never both on the element).
const CARD_OPEN: &str = "rounded-xl border border-gold bg-chip";

/// A facet's pill: gold with a check when passed, muted otherwise.
fn pill_classes(passed: bool) -> &'static str {
    if passed {
        "rounded-full border border-gold px-2 py-0.5 text-xs text-heading"
    } else {
        "rounded-full border border-line px-2 py-0.5 text-xs text-muted"
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
/// rows at every width that scrolls (keyboard too, `tabindex="0"`), `add`
/// just below the region ([`AddRow`], centred, outside it: it covers no card);
/// or 'No terms here yet', `add` under it.
#[component]
pub(crate) fn TermRows(
    /// The rows, already filtered and sorted.
    rows: Vec<Row>,
    /// What sits just below the list (the home's '+'); none by default.
    #[prop(default = None)]
    add: Option<ViewFn>,
) -> impl IntoView {
    let t = strings();
    if rows.is_empty() {
        return view! {
            <p class="text-muted">{move || td_string!(t.locale(), progress_empty)}</p>
            {add.map(|add| view! { <AddRow add=add /> })}
        }
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
        {add.map(|add| view! { <AddRow add=add /> })}
    }
    .into_any()
}

/// The row that holds `add` (the home's '+'), centred, a small gap above
/// it (`mt-3`): just below the list's scroll region, or where the list would
/// be.
#[component]
pub(crate) fn AddRow(
    /// What it holds.
    add: ViewFn,
) -> impl IntoView {
    view! {
        <div data-add-row="" class="mt-3 flex justify-center">
            {add.run()}
        </div>
    }
}

/// One term in the list: a [`TermCard`] with its own disclosure, closed.
#[component]
fn TermRow(
    /// The term and where it stands.
    row: Row,
) -> impl IntoView {
    let detail = Disclosed::new(detail_id(row.term.rank), RwSignal::new(false));
    view! {
        <li>
            <TermCard row=row open=detail />
        </li>
    }
}

/// One term as a card that holds its detail: the toggle ([`TermToggle`])
/// and, while open, the detail under a divider ([`TermDetail`]); the
/// border is gold while open.
#[component]
fn TermCard(
    /// The term and where it stands.
    row: Row,
    /// What shows and hides the detail.
    open: Disclosed,
) -> impl IntoView {
    let surface = move || {
        if open.is_open() {
            CARD_OPEN
        } else {
            CARD_CLOSED
        }
    };
    view! {
        <div data-term-card="" class=surface>
            <TermToggle row=row.clone() open=open />
            <TermDetail row=row open=open />
        </div>
    }
}

/// The card's toggle: ui-lib's `PaliTerm` in a full-width disclosure with
/// no surface of its own (the card has it): the lemma in bold with its
/// stage glued in parentheses and the due date on line 1, the sense and
/// the lapses on line 2. The stage, the date and the lapses follow the
/// language.
#[component]
fn TermToggle(
    /// The term and where it stands.
    row: Row,
    /// What it opens.
    open: Disclosed,
) -> impl IntoView {
    let t = strings();
    let language = language_signal();
    let (stage, due, lapses) = (row.stage, row.due, row.lapses);
    view! {
        <DisclosureButton controls=open variant=Variant::Nav size=Size::Lg layout="text-left">
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
    }
}

/// The detail a card opens, under a hairline divider: a pill per facet of
/// the term ([`FacetPills`]), then stage, next review and lapses.
#[component]
fn TermDetail(
    /// The term and where it stands.
    row: Row,
    /// What shows and hides it.
    open: Disclosed,
) -> impl IntoView {
    view! {
        <div id=open.id() hidden=move || !open.is_open() class="pb-3 text-sm">
            <div class="mx-4 border-t border-line"></div>
            <div class="flex flex-col gap-3 px-4 pt-3">
                <FacetPills facets=row.facets passed=row.passed />
                <DetailPairs row=row />
            </div>
        </div>
    }
}

/// One small pill per facet the term has, in the facets' order: passed ones
/// gold with a check, the others muted.
#[component]
fn FacetPills(
    /// The facets the term can be asked.
    facets: FacetSet,
    /// Those passed.
    passed: FacetSet,
) -> impl IntoView {
    let t = strings();
    view! {
        <ul data-facets="" class="flex flex-wrap gap-2">
            {Facet::ALL
                .into_iter()
                .filter(|facet| facets.contains(*facet))
                .map(|facet| {
                    let done = passed.contains(facet);
                    view! {
                        <li class=pill_classes(
                            done,
                        )>{done.then_some("✓ ")} {move || facet_label(t, facet)}</li>
                    }
                })
                .collect_view()}
        </ul>
    }
}

/// Stage, next review and lapses, label and value in two columns.
#[component]
fn DetailPairs(
    /// The term and where it stands.
    row: Row,
) -> impl IntoView {
    let t = strings();
    let language = language_signal();
    let (stage, due, lapses) = (row.stage, row.due, row.lapses);
    view! {
        <dl class="grid grid-cols-[auto_1fr] gap-x-4 gap-y-1">
            <dt class="text-muted">{move || td_string!(t.locale(), detail_stage)}</dt>
            <dd>{move || stage_label(t, stage)}</dd>
            <dt class="text-muted">{move || td_string!(t.locale(), detail_due)}</dt>
            <dd>{move || date_words(language.get(), due)}</dd>
            <dt class="text-muted">{move || td_string!(t.locale(), detail_lapses)}</dt>
            <dd>{lapses}</dd>
        </dl>
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

/// These tests render markup on the server: they need `ssr`.
#[cfg(all(test, feature = "ssr"))]
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
    fn the_strip_has_six_cells_three_by_three_that_filter_in_place() {
        let html = strip(None);
        let list = tag_around(&html, "<ul");
        let classes: Vec<&str> = list.split(['"', ' ']).collect();
        assert!(classes.contains(&"grid-cols-3"), "{list}");
        assert!(
            !classes.contains(&"sm:flex"),
            "three columns at every width: {list}"
        );
        assert!(!html.contains("sm:flex-1"), "{html}");
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
                r#">{label}</span><span class="order-first text-2xl font-semibold text-heading">{count}</span>"#
            );
            assert!(
                html.contains(&counted),
                "the default size: {counted}: {html}"
            );
        }
        assert!(!html.contains("text-base"), "not compact: {html}");
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

    /// The whole `<div>` element whose open tag holds byte `inside`.
    fn div_at(html: &str, inside: usize) -> &str {
        let start = html[..inside].rfind("<div").expect(html);
        let mut depth = 0;
        let mut i = start;
        loop {
            let open = html[i..].find("<div").map(|o| i + o);
            let close = html[i..].find("</div>").map(|c| i + c).expect(html);
            match open {
                Some(open) if open < close => {
                    depth += 1;
                    i = open + 4;
                }
                _ => {
                    depth -= 1;
                    i = close + 6;
                    if depth == 0 {
                        return &html[start..i];
                    }
                }
            }
        }
    }

    /// The card of the row whose lemma is `lemma`, whole.
    fn card<'a>(html: &'a str, lemma: &str) -> &'a str {
        let at = position(html, &format!(">{lemma}</span>"));
        let card = html[..at].rfind("data-term-card").expect(html);
        div_at(html, card)
    }

    /// The class tokens of the open tag holding `needle`.
    fn classes_of<'a>(html: &'a str, needle: &str) -> Vec<&'a str> {
        tag_around(html, needle).split(['"', ' ']).collect()
    }

    /// One card, its disclosure `open` or not.
    fn one_card(open: bool) -> String {
        let row = known_rows(&progress_fixture(), Some(Stage::Week)).remove(0);
        render(Language::En, move || {
            let detail = Disclosed::new(detail_id(row.term.rank), RwSignal::new(open));
            view! { <TermCard row=row open=detail /> }
        })
    }

    #[test]
    fn a_row_is_one_card_holding_the_toggle_and_its_detail() {
        let html = list(None, Language::En);
        let card = card(&html, "kho");
        let surface = classes_of(card, "data-term-card");
        for class in ["rounded-xl", "border", "border-line", "bg-chip"] {
            assert!(surface.contains(&class), "{class}: {surface:?}");
        }
        assert!(!surface.contains(&"border-gold"), "closed: {surface:?}");
        assert!(card.contains("<button"), "the toggle is inside: {card}");
        assert!(card.contains("<dl"), "the detail is inside: {card}");
        assert!(card.contains(r#"id="pc-detail-1""#), "{card}");
        let toggle = classes_of(card, r#"aria-controls="pc-detail-1""#);
        for class in ["w-full", "text-left"] {
            assert!(toggle.contains(&class), "{class}: {toggle:?}");
        }
        for surface in ["bg-chip", "border-line", "rounded-full", "underline"] {
            assert!(
                !toggle.contains(&surface),
                "transparent: {surface} in {toggle:?}"
            );
        }
    }

    #[test]
    fn an_open_card_has_a_gold_border_and_shows_its_detail_under_a_divider() {
        let html = one_card(true);
        let surface = classes_of(&html, "data-term-card");
        assert!(surface.contains(&"border-gold"), "{surface:?}");
        assert!(
            !surface.contains(&"border-line"),
            "one border colour: {surface:?}"
        );
        let detail = tag_around(&html, r#"id="pc-detail-1""#);
        assert!(!detail.split(['"', ' ']).any(|t| t == "hidden"), "{detail}");
        let divider = classes_of(&html, "border-t");
        for class in ["mx-4", "border-t", "border-line"] {
            assert!(divider.contains(&class), "{class}: {divider:?}");
        }
        assert!(
            position(&html, "</button>") < position(&html, "border-t")
                && position(&html, "border-t") < position(&html, "<dl"),
            "the toggle, the divider, the detail: {html}"
        );
        let closed = one_card(false);
        assert!(
            tag_around(&closed, r#"id="pc-detail-1""#).contains("hidden"),
            "{closed}"
        );
    }

    #[test]
    fn the_detail_has_a_pill_per_facet_of_the_term_the_passed_ones_gold_with_a_check() {
        let html = one_card(true);
        let kho = deck()
            .term(&learn_core::deck::TermId("kho".into()))
            .expect("kho");
        let facets = learn_core::facet::facets_of(kho);
        let pills = &html[position(&html, "data-facets")..];
        let pills = &pills[..position(pills, "</ul>")];
        assert_eq!(pills.matches("<li").count(), facets.len(), "{pills}");
        for facet in Facet::ALL {
            let name = match facet {
                Facet::Meaning => "Meaning",
                Facet::Recall => "Recall",
                Facet::Forms => "Forms",
                Facet::Context => "Context",
            };
            if !facets.contains(facet) {
                assert!(!pills.contains(name), "{name}: {pills}");
                continue;
            }
            let pill = tag_around(pills, &format!("{name}<"));
            let classes: Vec<&str> = pill.split(['"', ' ']).collect();
            for class in ["rounded-full", "px-2", "py-0.5", "text-xs", "border"] {
                assert!(classes.contains(&class), "{class}: {pill}");
            }
            let text = &pills[pills.find(pill).expect(pills)..];
            let text = &text[..position(text, "</li>")];
            if matches!(facet, Facet::Meaning | Facet::Recall) {
                assert!(classes.contains(&"border-gold"), "passed: {pill}");
                assert!(text.contains('✓'), "passed: {text}");
            } else {
                assert!(classes.contains(&"border-line"), "not passed: {pill}");
                assert!(!text.contains('✓'), "not passed: {text}");
            }
        }
    }

    #[test]
    fn the_detail_reads_stage_next_review_and_lapses() {
        let html = one_card(true);
        let dl = &html[position(&html, "<dl")..];
        let dl = &dl[..position(dl, "</dl>")];
        let order = [
            "Stage",
            "Weekly",
            "Next review",
            "12 October 2026",
            "Lapses",
            ">2<",
        ]
        .map(|text| position(dl, text));
        assert!(order.is_sorted(), "{order:?}: {dl}");
        assert!(dl.contains("grid-cols-[auto_1fr]"), "two columns: {dl}");
        for gone in ["Facets passed", ">Due<"] {
            assert!(!html.contains(gone), "{gone}: {html}");
        }
    }

    #[test]
    fn the_next_review_is_the_earliest_due_of_the_known_terms() {
        let state = progress_fixture();
        assert_eq!(next_review(&state), Some(crate::test_support::today()));
        let mut later = progress_fixture();
        later
            .progress
            .retain(|p| p.term.0 == "kho" || p.term.0 == "ghost 1");
        assert_eq!(
            next_review(&later),
            Some(Date::from_days(crate::test_support::today().days() + 7)),
            "an orphan's date does not count"
        );
        assert_eq!(next_review(&State::empty()), None);
    }

    #[test]
    fn a_row_is_ui_libs_pali_term_in_its_card() {
        let html = list(None, Language::En);
        let row = row_button(&html, "kho");
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
        let detail = div_at(&html, position(&html, r#"id="pc-detail-1""#));
        for text in [
            "✓ Meaning",
            "✓ Recall",
            "Lapses",
            "Next review",
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
