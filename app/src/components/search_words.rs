//! Add a word by search (062): the search icon on the home's CTA row, the
//! 'Add a word' screen it opens in place (a mode of the home, like a
//! session: the user, after the first check, wanted an interface like the
//! sutta search, not a popup, and adding must keep the screen), and that
//! screen's panel. The learner types a Pāli word (with or without diacritics);
//! after 300 ms of quiet and at least two letters the server is asked
//! ([`crate::dictionary::search_words`]) and the matching dictionary
//! entries are listed, each with an Add button. Add re-reads where the word
//! stands (started already, or an unusable store) before any call; a deck
//! term is started as the deck's own term; any other word first gets its
//! line from the suttas ([`crate::dictionary::passage_for`]): found, the
//! word is added with it; a miss adds the word without that facet and says
//! so; a failed call adds nothing and says so, with Add still there.
//!
//! The write goes through [`add_through`] with the backend captured by the
//! row, never through a signal of the popup: closing the popup while a call
//! is in flight still adds the word once. A newer query drops an older
//! answer (a sequence number); nothing is searched while an IME composes.
//! Phone: the same page, full width, the page scrolls.

use std::time::Duration;

use learn_core::added::{AddError, blank_on, lemma_of};
use learn_core::deck::{Example, Meaning, Pos, Term, TermId};
use leptos::prelude::*;
use leptos_i18n::td_string;
use palicanon_ui_lib::{Button, Icon, IconName, Pali, PaliSize, Size, Variant, language_signal};

use crate::backend::Backend;
use crate::backend::{SharedDictionary, Words};
use crate::catalogue::{Locale, strings};
use crate::components::add_word::{Standing, reason, standing};
use crate::components::data_block::ProgressData;
use crate::dictionary::{Entry, PassageAnswer, SearchAnswer};
use crate::learn::adding::{Refused, add_through};
use crate::learn::words::pos_words;
use crate::pages::home::HomeAction;

/// The quiet after the last keystroke before the server is asked.
pub(crate) const DEBOUNCE: Duration = Duration::from_millis(300);
/// The shortest query searched.
pub(crate) const MIN_CHARS: usize = 2;
const FIELD: &str = "w-full rounded-full border border-line bg-surface py-2 pr-4 pl-10 text-[17px] \
                     text-ink placeholder:text-muted focus-visible:outline-2 \
                     focus-visible:outline-offset-2 focus-visible:outline-gold";

/// What the popup shows under the field.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Phase {
    /// Nothing searched yet (or fewer than two letters): a hint.
    Idle,
    /// A query is in flight.
    Searching,
    /// The entries found, at most ten.
    List(Vec<Entry>),
    /// Nothing matched.
    Empty,
    /// The dictionary could not be reached.
    Error,
    /// This host has no dictionary (068: the app without its assets).
    Unavailable,
}

/// One row's Add button, through its states.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Slot {
    /// Add can be pressed.
    Ready,
    /// The passage is being fetched.
    Adding,
    /// The word is in the practice, with its line.
    Added,
    /// The word is in the practice, without a line from the suttas.
    AddedNoLine,
    /// Nothing was added; Add is offered again.
    Failed(Failure),
}

/// Why nothing was added.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Failure {
    /// A call failed or ran out of time.
    Transient,
    /// The stored progress refused the word.
    Refused(Refused),
}

/// The search icon on the home's CTA row: opens the 'Add a word' screen in
/// place ([`HomeAction::AddWord`]), icon-only, named for a screen reader;
/// a 44 px target on a phone.
#[component]
pub(crate) fn SearchButton(
    /// Performs the home's actions.
    act: Callback<HomeAction>,
) -> impl IntoView {
    let t = strings();
    move || {
        view! {
            <Button
                variant=Variant::Ghost
                size=Size::Md
                label=td_string!(t.locale(), search_open).to_string()
                layout="rounded-full"
                on_click=move |_| act.run(HomeAction::AddWord)
            >
                <Icon name=IconName::Search size=20 />
            </Button>
        }
    }
}

/// The 'Add a word' screen (062, the user's check: «une interface un peu
/// plus comme le search de suttas», in place like a session): no visible
/// title (the user: the field says it all), Back home at the right, then
/// the panel; the list is the page's, it scrolls with it; adding keeps the
/// screen.
#[component]
pub(crate) fn AddWordScreen(
    /// Back to the home.
    on_home: Callback<()>,
) -> impl IntoView {
    let t = strings();
    view! {
        <section data-add-word-screen="" class="flex flex-col gap-4">
            <h1 class="sr-only">{move || td_string!(t.locale(), search_title)}</h1>
            <div class="flex justify-end">
                <Button variant=Variant::Ghost size=Size::Sm on_click=move |_| on_home.run(())>
                    {move || td_string!(t.locale(), back_home)}
                </Button>
            </div>
            <SearchPanel />
        </section>
    }
}

/// The field, then the hint, the list, the empty line or the error with
/// Retry. The list is the page's: it scrolls with the page, so nothing
/// sits over the Add buttons.
#[component]
pub(crate) fn SearchPanel() -> impl IntoView {
    let t = strings();
    let search = Search::new(Words::current());
    let phase = search.phase;
    // Ready to type on arrival: the screen mounts after the page loaded, so
    // `autofocus` alone does nothing; the field is focused once it exists.
    let field: NodeRef<leptos::html::Input> = NodeRef::new();
    Effect::new(move |_| {
        if let Some(input) = field.get() {
            let _ = input.focus();
        }
    });
    view! {
        <div data-search-panel="" class="flex flex-col gap-3">
            <label class="relative block">
                <span class="pointer-events-none absolute inset-y-0 left-3 flex items-center text-muted">
                    <Icon name=IconName::Search size=18 />
                </span>
                <input
                    node_ref=field
                    type="search"
                    id="pc-search-field"
                    class=FIELD
                    autocomplete="off"
                    spellcheck="false"
                    autofocus
                    lang="pi"
                    aria-label=move || td_string!(t.locale(), search_title)
                    placeholder=move || td_string!(t.locale(), search_placeholder)
                    on:input=move |ev| search.typed(event_target_value(&ev))
                    on:compositionstart=move |_| search.composing(true)
                    on:compositionend=move |ev| {
                        search.composing(false);
                        search.typed(event_target_value(&ev));
                    }
                />
            </label>
            <div data-search-results="">{move || phase_view(&phase.get(), search, t.locale())}</div>
        </div>
    }
}

/// The search's driver: the debounce, the sequence number, the IME flag.
#[derive(Clone, Copy)]
struct Search {
    phase: RwSignal<Phase>,
    seq: StoredValue<u64>,
    timer: StoredValue<Option<TimeoutHandle>>,
    composing: StoredValue<bool>,
    last: StoredValue<String>,
    /// The host's dictionary (068), read when the panel mounts.
    words: StoredValue<Option<SharedDictionary>, LocalStorage>,
}

impl Search {
    fn new(words: Option<SharedDictionary>) -> Search {
        Search {
            phase: RwSignal::new(if words.is_some() {
                Phase::Idle
            } else {
                Phase::Unavailable
            }),
            seq: StoredValue::new(0),
            timer: StoredValue::new(None),
            composing: StoredValue::new(false),
            last: StoredValue::new(String::new()),
            words: StoredValue::new_local(words),
        }
    }

    fn composing(self, on: bool) {
        self.composing.set_value(on);
    }

    /// A keystroke: remembered; searched after the quiet, if long enough.
    fn typed(self, q: String) {
        let q = q.trim().to_string();
        self.last.set_value(q.clone());
        if let Some(timer) = self.timer.try_update_value(Option::take).flatten() {
            timer.clear();
        }
        if self.composing.get_value() {
            return;
        }
        if q.chars().count() < MIN_CHARS {
            self.seq.update_value(|n| *n += 1);
            self.phase.set(Phase::Idle);
            return;
        }
        let handle = set_timeout_with_handle(move || self.fire(q), DEBOUNCE).ok();
        self.timer.set_value(handle);
    }

    /// Retry asks again with the last query, at once.
    fn retry(self) {
        let q = self.last.get_value();
        if q.chars().count() >= MIN_CHARS {
            self.fire(q);
        }
    }

    fn fire(self, q: String) {
        let n = self.seq.try_update_value(|n| {
            *n += 1;
            *n
        });
        let Some(n) = n else {
            return;
        };
        let Some(words) = self.words.try_get_value().flatten() else {
            self.phase.set(Phase::Unavailable);
            return;
        };
        self.phase.set(Phase::Searching);
        words.search(
            q,
            Box::new(move |answer| {
                if self.seq.try_get_value() != Some(n) {
                    return;
                }
                self.phase.try_set(phase_of(Some(answer)));
            }),
        );
    }
}

/// The phase a search answer puts the popup in.
pub(crate) fn phase_of(answer: Option<SearchAnswer>) -> Phase {
    match answer {
        Some(SearchAnswer::Entries(entries)) if entries.is_empty() => Phase::Empty,
        Some(SearchAnswer::Entries(entries)) => Phase::List(entries),
        Some(SearchAnswer::Unreachable) | None => Phase::Error,
    }
}

fn status(text: &'static str) -> AnyView {
    view! {
        <p role="status" class="py-2 text-center text-sm text-muted">
            {text}
        </p>
    }
    .into_any()
}

/// A short word at the row's right (`Added`), where the button was.
fn mark(text: &'static str) -> AnyView {
    view! {
        <span role="status" class="shrink-0 text-sm text-muted">
            {text}
        </span>
    }
    .into_any()
}

/// A line under the entry: why no line from the suttas, or why nothing
/// was added.
fn note(text: &'static str) -> AnyView {
    view! {
        <p data-search-note="" class="text-sm text-muted">
            {text}
        </p>
    }
    .into_any()
}

fn phase_view(phase: &Phase, search: Search, locale: Locale) -> AnyView {
    match phase {
        Phase::Idle => status(td_string!(locale, search_hint)),
        Phase::Unavailable => status(td_string!(locale, search_unavailable)),
        Phase::Searching => status(td_string!(locale, search_searching)),
        Phase::Empty => status(td_string!(locale, search_empty)),
        Phase::Error => view! {
            <div class="flex flex-col items-center">
                {status(td_string!(locale, search_error))}
                <Button variant=Variant::Ghost size=Size::Sm on_click=move |_| search.retry()>
                    {td_string!(locale, search_retry)}
                </Button>
            </div>
        }
        .into_any(),
        Phase::List(entries) => {
            let count = entries.len();
            let count_line = if count == 1 {
                td_string!(locale, search_count_1).to_string()
            } else {
                td_string!(locale, search_count_n, count = count).to_string()
            };
            view! {
                <p data-search-count="" class="border-b border-line pb-3 text-sm text-muted">
                    {count_line}
                </p>
                <ul class="divide-y divide-line">
                    {entries
                        .iter()
                        .cloned()
                        .map(|entry| view! { <EntryRow entry=entry /> })
                        .collect_view()}
                </ul>
            }
            .into_any()
        }
    }
}

/// One entry: headword, part of speech, meaning, and its Add. The backend
/// is read here, when a list renders (always under the app's provider),
/// never when the popup mounts, and captured by the row for the write.
#[component]
fn EntryRow(
    /// The dictionary entry.
    entry: Entry,
) -> impl IntoView {
    let language = language_signal();
    let backend = Backend::current();
    let data = use_context::<ProgressData>();
    let id = TermId(entry.lemma.clone());
    let slot = RwSignal::new(match standing(None, backend, &id) {
        Standing::Started => Slot::Added,
        _ => Slot::Ready,
    });
    let number = entry.lemma[entry.headword.len().min(entry.lemma.len())..]
        .trim()
        .to_string();
    let pressed = entry.clone();
    let words = use_context::<Words>();
    let press = Callback::new(move |_| {
        let dictionary = words.and_then(Words::current_of);
        press_add(pressed.clone(), slot, backend, data, dictionary);
    });
    view! {
        <li class="flex flex-col gap-1 py-4">
            <div class="flex min-h-11 items-center gap-3">
                <div class="min-w-0 flex-1">
                    <p>
                        <Pali size=PaliSize::Lemma>{entry.headword.clone()}</Pali>
                        {(!number.is_empty())
                            .then(|| {
                                view! { <span class="ml-1 text-sm text-muted">{number}</span> }
                            })}
                        <span class="ml-2 text-sm text-muted">
                            {move || pos_words(language.get(), &entry.pos)}
                        </span>
                    </p>
                    <p class="truncate text-sm text-body">{entry.meaning.clone()}</p>
                </div>
                {move || slot_right(&slot.get(), press, Locale::from(language.get()))}
            </div>
            {move || slot_note(&slot.get(), Locale::from(language.get()))}
        </li>
    }
}

fn slot_right(slot: &Slot, press: Callback<leptos::ev::MouseEvent>, locale: Locale) -> AnyView {
    let add = move |text: &'static str, enabled: bool| {
        view! {
            <Button variant=Variant::Ghost size=Size::Sm on_click=press disabled=!enabled>
                {text}
            </Button>
        }
        .into_any()
    };
    match slot {
        Slot::Ready | Slot::Failed(_) => add(td_string!(locale, search_add), true),
        Slot::Adding => add(td_string!(locale, search_adding), false),
        Slot::Added | Slot::AddedNoLine => mark(td_string!(locale, search_added)),
    }
}

fn slot_note(slot: &Slot, locale: Locale) -> Option<AnyView> {
    match slot {
        Slot::Ready | Slot::Adding | Slot::Added => None,
        Slot::AddedNoLine => Some(note(td_string!(locale, search_added_no_line))),
        Slot::Failed(Failure::Transient) => Some(note(td_string!(locale, search_transient))),
        Slot::Failed(Failure::Refused(refused)) => Some(note(reason(locale, refused))),
    }
}

fn press_add(
    entry: Entry,
    slot: RwSignal<Slot>,
    backend: Backend,
    data: Option<ProgressData>,
    words: Option<SharedDictionary>,
) {
    if matches!(
        slot.get_untracked(),
        Slot::Adding | Slot::Added | Slot::AddedNoLine
    ) {
        return;
    }
    let id = TermId(entry.lemma.clone());
    match standing(None, backend, &id) {
        Standing::Started => return slot.set(Slot::Added),
        Standing::Unusable(refused) => return slot.set(Slot::Failed(Failure::Refused(refused))),
        Standing::Open => {}
    }
    if let Some(term) = crate::deck::deck().term(&id).cloned() {
        let has_line = term.example.is_some();
        slot.set(settle(add_through(None, backend, term), has_line, data));
        return;
    }
    let Some(words) = words else {
        slot.set(Slot::Failed(Failure::Transient));
        return;
    };
    slot.set(Slot::Adding);
    let asked = entry.clone();
    words.passage(
        asked,
        Box::new(move |answer| {
            let outcome = match answer {
                PassageAnswer::Found(example) => settle(
                    add_through(None, backend, term_for(&entry, Some(example))),
                    true,
                    data,
                ),
                PassageAnswer::Miss(_) => settle(
                    add_through(None, backend, term_for(&entry, None)),
                    false,
                    data,
                ),
                PassageAnswer::Transient => Slot::Failed(Failure::Transient),
            };
            slot.try_set(outcome);
        }),
    );
}

/// The slot after the write: added (with or without its line; a word
/// started meanwhile counts as added), or refused by the store.
pub(crate) fn settle(
    result: Result<(), Refused>,
    has_line: bool,
    data: Option<ProgressData>,
) -> Slot {
    match result {
        Ok(()) => {
            if let Some(data) = data {
                data.moved();
            }
            if has_line {
                Slot::Added
            } else {
                Slot::AddedNoLine
            }
        }
        Err(Refused::Add(AddError::AlreadyStarted(_))) => Slot::Added,
        Err(refused) => Slot::Failed(Failure::Refused(refused)),
    }
}

/// The term an entry becomes: shaped like 054's added words (the DPD's
/// numbered lemma as id, the bare word as lemma, English meaning, no forms,
/// the last rank); the passage keeps the blank the server made from the
/// DPD's mark, else is blanked on the word when the line holds it as is.
pub(crate) fn term_for(entry: &Entry, passage: Option<Example>) -> Term {
    Term {
        id: TermId(entry.lemma.clone()),
        lemma: lemma_of(&entry.lemma).to_string(),
        pos: Pos(entry.pos.clone()),
        meaning: Meaning {
            en: entry.meaning.clone(),
            fr: None,
            vi: None,
            literal: None,
        },
        construction: None,
        root: None,
        example: passage.map(|passage| Example {
            blank: passage
                .blank
                .clone()
                .or_else(|| blank_on(&passage.text, &entry.headword)),
            ..passage
        }),
        forms: Vec::new(),
        rank: u32::MAX,
    }
}

#[cfg(all(test, feature = "ssr"))]
mod tests {
    use std::collections::BTreeMap;

    use learn_core::deck::Translation;
    use palicanon_ui_lib::Language;

    use super::*;
    use crate::dictionary::Miss;
    use crate::test_support::provide_fakes;

    fn vakkali() -> Entry {
        Entry {
            headword: "vakkali".into(),
            lemma: "vakkali".into(),
            pos: "masc".into(),
            meaning: "name of a monk".into(),
            example: Some("alaṃ vakkali".into()),
            source: Some("SN22.87".into()),
            sutta: Some("Vakkalisutta".into()),
        }
    }

    fn dhamma_1() -> Entry {
        Entry {
            headword: "dhamma".into(),
            lemma: "dhamma 1".into(),
            pos: "masc".into(),
            meaning: "nature; teaching".into(),
            example: None,
            source: None,
            sutta: None,
        }
    }

    fn passage() -> Example {
        let mut translations = BTreeMap::new();
        translations.insert(
            "en".to_string(),
            Translation {
                text: "Enough, Vakkali.".into(),
                translator: "sujato".into(),
            },
        );
        Example {
            text: "alaṃ vakkali".into(),
            source: "SN22.87".into(),
            sutta: "Vakkalisutta".into(),
            blank: None,
            translations,
        }
    }

    fn render(view: impl IntoView) -> String {
        view.to_html().replace("<!>", "")
    }

    #[test]
    fn the_term_takes_the_headword_as_id_the_lemma_and_the_blanked_passage() {
        let term = term_for(&vakkali(), Some(passage()));
        assert_eq!(term.id.0, "vakkali");
        assert_eq!(term.lemma, "vakkali");
        assert_eq!(term.pos.0, "masc");
        assert_eq!(term.meaning.en, "name of a monk");
        assert_eq!(term.rank, u32::MAX);
        assert_eq!(term.forms.len(), 0);
        let example = term.example.unwrap();
        assert!(example.blank.is_some(), "the lemma is in the line: blanked");
        assert_eq!(example.translations["en"].translator, "sujato");
        let numbered = term_for(&dhamma_1(), None);
        assert_eq!(numbered.id.0, "dhamma 1");
        assert_eq!(numbered.lemma, "dhamma");
        assert!(numbered.example.is_none(), "a miss: no context facet");
        assert_eq!(numbered.lemma, "dhamma", "shown bare");
    }

    #[test]
    fn a_line_without_the_lemma_as_is_keeps_the_passage_unblanked() {
        let mut inflected = passage();
        inflected.text = "vakkalissa vacanaṃ".into();
        let term = term_for(&vakkali(), Some(inflected));
        let example = term.example.unwrap();
        assert!(example.blank.is_none(), "{example:?}");
        assert_eq!(example.text, "vakkalissa vacanaṃ");
    }

    #[test]
    fn the_phase_follows_the_answer() {
        assert_eq!(
            phase_of(Some(SearchAnswer::Entries(Vec::new()))),
            Phase::Empty
        );
        assert_eq!(
            phase_of(Some(SearchAnswer::Entries(vec![vakkali()]))),
            Phase::List(vec![vakkali()])
        );
        assert_eq!(phase_of(Some(SearchAnswer::Unreachable)), Phase::Error);
        assert_eq!(
            phase_of(None),
            Phase::Error,
            "a transport failure to this server"
        );
        let _ = Miss::NoExample;
    }

    #[test]
    fn the_slot_settles_as_added_with_or_without_its_line_and_refused_otherwise() {
        assert_eq!(settle(Ok(()), true, None), Slot::Added);
        assert_eq!(settle(Ok(()), false, None), Slot::AddedNoLine);
        let there = Err(Refused::Add(AddError::AlreadyStarted(TermId("x".into()))));
        assert_eq!(
            settle(there, false, None),
            Slot::Added,
            "started meanwhile counts as added"
        );
        assert_eq!(
            settle(Err(Refused::Newer), true, None),
            Slot::Failed(Failure::Refused(Refused::Newer))
        );
    }

    #[test]
    fn the_icon_is_a_named_button_that_asks_the_add_word_screen() {
        Owner::new().with(|| {
            provide_fakes(crate::store::Load::Empty);
            let asked = RwSignal::new(None);
            let act = Callback::new(move |action| asked.set(Some(action)));
            let html = render(view! { <SearchButton act=act /> });
            assert!(html.contains(r#"aria-label="Add a word""#), "{html}");
            assert!(html.contains("<button"), "{html}");
            assert!(html.contains("<svg"), "an icon, not a word: {html}");
            assert!(!html.contains(">Add a word<"), "icon-only: {html}");
            act.run(HomeAction::AddWord);
            assert_eq!(asked.get_untracked(), Some(HomeAction::AddWord));
        });
    }

    #[test]
    fn the_screen_has_its_title_the_field_and_a_way_back_home() {
        Owner::new().with(|| {
            provide_fakes(crate::store::Load::Empty);
            let on_home = Callback::new(|()| {});
            let html = render(view! { <AddWordScreen on_home=on_home /> });
            assert!(
                html.contains(r#"<h1 class="sr-only">Add a word</h1>"#),
                "no visible title: {html}"
            );
            assert!(html.contains(r#"type="search""#), "{html}");
            assert!(html.contains(">Back home<"), "{html}");
            assert!(html.contains("text-center"), "the hint centred: {html}");
            assert!(
                !html.contains(r#"role="dialog""#),
                "in place, not a popup: {html}"
            );
            assert!(
                !html.contains("overflow-y-auto"),
                "the page scrolls: {html}"
            );
        });
    }

    #[test]
    fn the_panel_has_the_field_with_the_focus_and_the_hint() {
        Owner::new().with(|| {
            provide_fakes(crate::store::Load::Empty);
            provide_context(Words::new(std::rc::Rc::new(FakeWords)));
            let html = render(view! { <SearchPanel /> });
            assert!(html.contains(r#"type="search""#), "{html}");
            assert!(html.contains("autofocus"), "{html}");
            assert!(html.contains(r#"aria-label="Add a word""#), "{html}");
            assert!(html.contains(r#"lang="pi""#), "{html}");
            assert!(
                html.contains("Type at least two letters"),
                "the hint: {html}"
            );
            assert!(
                !html.contains(r#"role="dialog""#),
                "a page, not a popup: {html}"
            );
            assert!(
                !html.contains("overflow-y-auto"),
                "the page scrolls: {html}"
            );
        });
    }

    fn phase_html(phase: Phase, locale: Locale) -> String {
        Owner::new().with(|| {
            provide_fakes(crate::store::Load::Empty);
            let backend = Backend::current();
            let _ = backend;
            render(phase_view(&phase, Search::new(None), locale))
        })
    }

    #[test]
    fn each_phase_has_its_line_and_the_error_its_retry() {
        let english = Locale::from(Language::En);
        assert!(phase_html(Phase::Searching, english).contains("Searching"));
        assert!(phase_html(Phase::Empty, english).contains("No word found for the letters typed."));
        let error = phase_html(Phase::Error, english);
        assert!(error.contains("not reachable right now"), "{error}");
        assert!(error.contains(">Retry<"), "{error}");
        let fr = phase_html(Phase::Empty, Locale::from(Language::Fr));
        assert!(fr.contains("Aucun mot"), "{fr}");
    }

    #[test]
    fn a_row_shows_the_entry_and_its_add_and_a_started_word_shows_added() {
        let english = Locale::from(Language::En);
        let html = phase_html(Phase::List(vec![vakkali(), dhamma_1()]), english);
        assert!(html.contains("vakkali"), "{html}");
        assert!(html.contains("masculine noun"), "the pos in words: {html}");
        assert!(html.contains("name of a monk"), "{html}");
        assert_eq!(html.matches(">Add<").count(), 2, "{html}");
        assert!(
            html.contains(r#"class="ml-1 text-sm text-muted">1<"#),
            "the homonym number muted: {html}"
        );
        assert!(html.contains("min-h-11"), "a 44px row: {html}");
        assert!(
            !html.contains("data-search-note"),
            "nothing added yet, no note: {html}"
        );
        assert!(html.contains("data-search-count"), "{html}");
        assert!(html.contains(">2 entries</p>"), "the count line: {html}");
        assert!(html.contains("divide-y"), "dividers: {html}");
    }

    #[test]
    fn an_added_word_shows_a_short_mark_and_its_note_under_the_entry() {
        let owner = Owner::new();
        owner.set();
        let english = Locale::from(Language::En);
        let press = Callback::new(|_| {});
        let right = render(slot_right(&Slot::AddedNoLine, press, english));
        assert!(
            right.contains(">Added</span>"),
            "short, at the right: {right}"
        );
        assert!(!right.contains("No line"), "{right}");
        let under = render(slot_note(&Slot::AddedNoLine, english));
        assert!(under.contains("data-search-note"), "{under}");
        assert!(
            under.contains("Added. No line from the suttas for this word."),
            "{under}"
        );
        assert!(
            render(slot_note(&Slot::Added, english)).is_empty(),
            "a line: no note"
        );
        let failed = render(slot_right(
            &Slot::Failed(Failure::Transient),
            press,
            english,
        ));
        assert!(failed.contains(">Add<"), "Add offered again: {failed}");
        let why = render(slot_note(&Slot::Failed(Failure::Transient), english));
        assert!(why.contains("Nothing was added"), "{why}");
    }

    /// A fake host dictionary: answers at once, from fixtures.
    struct FakeWords;

    impl crate::backend::Dictionary for FakeWords {
        fn search(&self, q: String, on_done: Box<dyn FnOnce(SearchAnswer)>) {
            let entries = if q.starts_with("vak") {
                vec![vakkali()]
            } else {
                Vec::new()
            };
            on_done(SearchAnswer::Entries(entries));
        }
        fn passage(&self, _entry: Entry, on_done: Box<dyn FnOnce(PassageAnswer)>) {
            on_done(PassageAnswer::Found(passage()));
        }
        fn lookup(&self, _form: String, on_done: Box<dyn FnOnce(crate::dictionary::LookupAnswer)>) {
            on_done(crate::dictionary::LookupAnswer::Senses(Vec::new()));
        }
    }

    #[test]
    fn a_host_without_a_dictionary_says_so_and_a_host_with_one_lists_its_entries() {
        Owner::new().with(|| {
            provide_fakes(crate::store::Load::Empty);
            let none = Search::new(None);
            assert_eq!(none.phase.get_untracked(), Phase::Unavailable);
            none.fire("vak".to_string());
            assert_eq!(
                none.phase.get_untracked(),
                Phase::Unavailable,
                "no call to make"
            );
            let html = render(phase_view(
                &Phase::Unavailable,
                none,
                Locale::from(Language::En),
            ));
            assert!(html.contains("not available"), "{html}");
            let some = Search::new(Some(std::rc::Rc::new(FakeWords)));
            assert_eq!(some.phase.get_untracked(), Phase::Idle);
            some.fire("vak".to_string());
            assert_eq!(some.phase.get_untracked(), Phase::List(vec![vakkali()]));
            some.fire("zz".to_string());
            assert_eq!(some.phase.get_untracked(), Phase::Empty);
        });
    }

    #[test]
    fn the_panel_without_a_dictionary_says_it_is_not_available_here() {
        Owner::new().with(|| {
            provide_fakes(crate::store::Load::Empty);
            let html = render(view! { <SearchPanel /> });
            assert!(html.contains("not available"), "{html}");
            assert!(!html.contains("Type at least two letters"), "{html}");
        });
    }

    #[test]
    fn every_string_is_in_the_three_locales_without_an_em_dash() {
        for file in [
            include_str!("../../locales/en.json"),
            include_str!("../../locales/fr.json"),
            include_str!("../../locales/vi.json"),
        ] {
            for key in [
                "search_open",
                "search_title",
                "search_placeholder",
                "search_hint",
                "search_searching",
                "search_empty",
                "search_error",
                "search_retry",
                "search_add",
                "search_adding",
                "search_added",
                "search_added_no_line",
                "search_transient",
                "search_count_1",
                "search_count_n",
                "search_unavailable",
            ] {
                let line = file
                    .lines()
                    .find(|l| l.contains(&format!("\"{key}\"")))
                    .unwrap_or_else(|| panic!("{key} missing"));
                assert!(!line.contains('\u{2014}'), "{line}");
            }
        }
    }
}
