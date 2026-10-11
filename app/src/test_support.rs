//! What the screens' tests share: a fixed `today`, a progress fixture on
//! the embedded deck, and small readers of rendered HTML.

use learn_core::date::Date;
use learn_core::deck::TermId;
use learn_core::facet::{Facet, FacetSet};
use learn_core::progress::{Progress, State};
use learn_core::session::Session;
use learn_core::stage::Stage;

use crate::deck::deck;

/// The tests' today, 5 October 2026.
pub(crate) fn today() -> Date {
    Date::parse("2026-10-05").expect("a date")
}

/// Three new terms (kho, ca 2.1, vā 1); kho moved to Week with two lapses
/// and two facets passed (due 12 October 2026); one orphan, `ghost 1`, in
/// Quarter.
pub(crate) fn progress_fixture() -> State {
    let mut session = Session::start(deck(), State::empty(), today(), 0).expect("a session");
    assert_eq!(session.request_new(3), 3);
    let mut state = session.into_state();
    let week = &mut state.progress[0];
    week.stage = Stage::Week;
    week.due = Date::from_days(today().days() + 7);
    week.lapses = 2;
    week.passed = [Facet::Meaning, Facet::Recall].into_iter().collect();
    state.progress.push(Progress {
        term: TermId("ghost 1".into()),
        stage: Stage::Quarter,
        passed: FacetSet::empty(),
        due: today(),
        lapses: 0,
    });
    state
}

/// The byte where `needle` starts; a panic showing the HTML otherwise.
#[cfg(feature = "ssr")]
pub(crate) fn position(html: &str, needle: &str) -> usize {
    html.find(needle)
        .unwrap_or_else(|| panic!("{needle}: {html}"))
}

/// The opening tag that contains `needle`.
#[cfg(feature = "ssr")]
pub(crate) fn tag_around<'a>(html: &'a str, needle: &str) -> &'a str {
    let at = position(html, needle);
    let start = html[..at].rfind('<').expect(html);
    &html[start..at + html[at..].find('>').expect(html)]
}

/// The `<button>` that ends with the label `label`, its class removed (the
/// variant's classes hold `disabled:` states of their own).
#[cfg(feature = "ssr")]
pub(crate) fn button_labelled(html: &str, label: &str) -> String {
    let end = position(html, &format!(">{label}<"));
    let tag = &html[html[..end].rfind("<button").expect(html)..end];
    let class = tag.find(r#"class=""#).expect(tag);
    let close = class + 7 + tag[class + 7..].find('"').expect(tag);
    format!("{}{}", &tag[..class], &tag[close + 1..])
}
