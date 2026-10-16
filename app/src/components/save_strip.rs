//! The strip 'Your progress is not saved': shown at the top of the home
//! from the store's first `Failed` status until a save succeeds (`Pending`
//! keeps it), with Export of the progress that would be lost: the current
//! state for a transient failure, the unsaved state for a newer file that
//! won. Nothing is rendered while the progress is saved.

use learn_core::progress::State;
use leptos::prelude::*;
use leptos_i18n::td_string;
use palicanon_ui_lib::{Button, Variant};

use crate::backend::{Backend, SaveError, SaveStatus};
use crate::catalogue::strings;
use crate::components::data_block::ProgressData;
use crate::store::{Load, export_name};
use crate::today::{browser_today, session_today};

/// The state the strip offers to export for `error`: the unsaved one
/// of a lost conflict, else what the store holds now (`current`).
pub(crate) fn exportable(error: &SaveError, current: Load) -> Option<State> {
    match error {
        SaveError::NewerVersion { unsaved } => Some(unsaved.clone()),
        SaveError::Transient => match current {
            Load::Ok(state) => Some(state),
            Load::Empty
            | Load::Invalid { .. }
            | Load::NewerVersion { .. }
            | Load::Unreadable(_) => None,
        },
    }
}

/// Export of the progress `error` leaves unsaved, through the host's file
/// transfer, named for the day (`pc_learn-<date>.json`); nothing when
/// there is nothing to export.
pub(crate) fn export_for(backend: Backend, error: &SaveError) {
    if let Some(state) = exportable(error, backend.load()) {
        let name = export_name(session_today(browser_today(), &state));
        backend.export(&name, &state.to_json());
    }
}

/// The failure the strip shows after `status`: a new `Failed` replaces the
/// old one, `Saved` clears it, and `Pending` keeps it (a write in flight
/// after a failure is not yet a save that succeeded).
pub(crate) fn kept_failure(previous: Option<SaveError>, status: &SaveStatus) -> Option<SaveError> {
    match status {
        SaveStatus::Failed(error) => Some(error.clone()),
        SaveStatus::Saved => None,
        SaveStatus::Pending => previous,
    }
}

/// The failure to show, following `status`: it stays until a save succeeds.
pub(crate) fn failure(status: Signal<SaveStatus>) -> Memo<Option<SaveError>> {
    Memo::new(move |previous: Option<&Option<SaveError>>| {
        kept_failure(previous.cloned().flatten(), &status.get())
    })
}

/// The strip while a failure is kept, nothing after a save succeeded. The
/// failure is kept by the provider's data, not here: a strip built again
/// (the page left and entered) still shows it.
#[component]
pub(crate) fn SaveStrip() -> impl IntoView {
    let backend = Backend::current();
    let shown = ProgressData::current().failure;
    move || {
        shown
            .get()
            .map(|error| view! { <Strip backend=backend error=error /> })
    }
}

/// The strip's content: the two sentences and, when there is something to
/// export, the Export button.
#[component]
fn Strip(backend: Backend, error: SaveError) -> impl IntoView {
    let t = strings();
    // The state is taken when the button is clicked: a transient failure
    // goes on taking saves, and the export is of the latest one.
    let offer = exportable(&error, backend.load()).is_some();
    let on_click = move |_| export_for(backend, &error);
    let export = offer.then(|| {
        view! {
            <Button variant=Variant::Accent on_click=on_click>
                {move || td_string!(t.locale(), export)}
            </Button>
        }
    });
    view! {
        <div
            role="alert"
            data-save-strip=""
            class="flex flex-wrap items-center justify-between gap-3 rounded-lg border border-gold/40 bg-gold/20 px-4 py-3 text-sm text-heading"
        >
            <p>
                <strong>{move || td_string!(t.locale(), save_failed_title)}</strong>
                " "
                {move || td_string!(t.locale(), save_failed_body)}
            </p>
            {export}
        </div>
    }
}

/// These tests render markup on the server: they need `ssr`.
#[cfg(all(test, feature = "ssr"))]
mod tests {
    use learn_core::progress::State;
    use palicanon_ui_lib::Language;

    use super::*;
    use crate::test_support::{FakeStore, Outcome, backend_of, progress_fixture, provide_fakes};

    /// The strip as the server renders it, `store` behind the provider.
    fn strip(language: Language, store: &FakeStore) -> String {
        Owner::new().with(|| {
            palicanon_ui_lib::provide_language().set(language);
            provide_context(backend_of(
                store,
                &crate::test_support::FakeFiles::default(),
            ));
            provide_context(ProgressData::new());
            view! { <SaveStrip /> }.to_html()
        })
    }

    /// A state with one more answered term than `progress_fixture`: another
    /// state, to tell the unsaved one from the first.
    fn later() -> State {
        let mut state = progress_fixture();
        state.progress[0].lapses += 1;
        state
    }

    #[test]
    fn a_transient_failure_exports_the_current_state() {
        let state = progress_fixture();
        assert_eq!(
            exportable(&SaveError::Transient, Load::Ok(state.clone())),
            Some(state)
        );
        for nothing in [
            Load::Empty,
            Load::Invalid { raw: "x".into() },
            Load::NewerVersion { raw: "x".into() },
            Load::Unreadable("denied".into()),
        ] {
            assert_eq!(exportable(&SaveError::Transient, nothing), None);
        }
    }

    #[test]
    fn a_lost_conflict_exports_the_unsaved_state_whatever_the_store_holds() {
        let unsaved = later();
        let error = SaveError::NewerVersion {
            unsaved: unsaved.clone(),
        };
        let newer = Load::NewerVersion { raw: "x".into() };
        assert_eq!(exportable(&error, newer), Some(unsaved));
    }

    #[test]
    fn while_saved_or_pending_there_is_no_strip() {
        Owner::new().with(|| {
            let store = FakeStore::new(Load::Ok(progress_fixture()));
            assert_eq!(
                strip(Language::En, &store)
                    .matches("data-save-strip")
                    .count(),
                0
            );
            store.set_status(SaveStatus::Pending);
            assert_eq!(
                strip(Language::En, &store)
                    .matches("data-save-strip")
                    .count(),
                0
            );
        });
    }

    #[test]
    fn a_failed_save_shows_the_strip_in_every_language_with_export() {
        for (language, title, export) in [
            (Language::En, "Your progress is not saved.", "Export"),
            (
                Language::Fr,
                "Votre progression n’est pas enregistrée.",
                "Exporter",
            ),
            (Language::Vi, "Tiến độ của bạn chưa được lưu.", "Xuất"),
        ] {
            Owner::new().with(|| {
                let store = FakeStore::new(Load::Ok(progress_fixture()));
                store.script(&[Outcome::Transient]);
                crate::backend::ProgressStore::save(&store, &progress_fixture());
                let html = strip(language, &store).replace("<!>", "");
                assert!(html.contains(r#"role="alert""#), "{html}");
                assert!(html.contains(&format!(">{title}</strong>")), "{html}");
                assert!(html.contains(&format!(">{export}<")), "{html}");
            });
        }
    }

    /// The design's sequence: a save fails Transient (the strip, the state
    /// kept), the retry meets a newer file (the strip with the unsaved
    /// state offered to Export, the home read-only), never two screens
    /// contradicting.
    #[test]
    fn a_transient_failure_then_a_newer_file_ends_read_only_with_the_latest_state_offered() {
        use crate::learn::home_state::{BrokenKind, HomeState, home_state};
        Owner::new().with(|| {
            let (first, second) = (progress_fixture(), later());
            let store = FakeStore::new(Load::Ok(State::empty()));
            let files = crate::test_support::FakeFiles::default();
            let backend = backend_of(&store, &files);
            let seen = std::rc::Rc::new(std::cell::Cell::new(0));
            let counter = std::rc::Rc::clone(&seen);
            backend.on_change(move || counter.set(counter.get() + 1));
            store.script(&[Outcome::Written, Outcome::Transient, Outcome::Newer]);

            backend.save(&State::empty());
            assert_eq!(store.status_now(), SaveStatus::Saved);
            assert_eq!(store.written(), [State::empty()]);
            backend.save(&first);
            assert_eq!(store.status_now(), SaveStatus::Failed(SaveError::Transient));
            assert_eq!(
                backend.load(),
                Load::Ok(first.clone()),
                "the memory is kept"
            );
            assert_eq!(strip_of(&store).matches("data-save-strip").count(), 1);
            assert_eq!(seen.get(), 0, "a failure is no change outside a save");

            backend.save(&second);
            let unsaved = SaveError::NewerVersion {
                unsaved: second.clone(),
            };
            assert_eq!(store.status_now(), SaveStatus::Failed(unsaved.clone()));
            assert_eq!(seen.get(), 1, "on_change ran in the same synchronous run");
            // The home reads the newer file: read-only, not an editable home.
            let today = crate::test_support::today();
            assert_eq!(
                home_state(&backend.load(), crate::deck::deck(), today),
                HomeState::Broken(BrokenKind::NewerVersion)
            );
            // The strip offers the LATEST state, not the first.
            export_for(backend, &unsaved);
            let exported = files.exported();
            assert_eq!(exported.len(), 1, "{exported:?}");
            assert_eq!(exported[0].1, second.to_json());
            assert_ne!(exported[0].1, first.to_json());
            assert!(strip_of(&store).contains("Export"));
        });
    }

    /// Once Failed the strip stays until a save succeeds: `Pending` keeps
    /// it, `Saved` takes it away, a new failure replaces the old one.
    #[test]
    fn a_failure_is_kept_until_a_save_succeeds() {
        let newer = SaveError::NewerVersion { unsaved: later() };
        let failed = SaveStatus::Failed(SaveError::Transient);
        assert_eq!(kept_failure(None, &SaveStatus::Saved), None);
        assert_eq!(kept_failure(None, &SaveStatus::Pending), None);
        assert_eq!(kept_failure(None, &failed), Some(SaveError::Transient));
        assert_eq!(
            kept_failure(Some(SaveError::Transient), &SaveStatus::Pending),
            Some(SaveError::Transient),
            "Pending after Failed keeps the strip"
        );
        assert_eq!(
            kept_failure(Some(SaveError::Transient), &SaveStatus::Saved),
            None
        );
        assert_eq!(
            kept_failure(
                Some(SaveError::Transient),
                &SaveStatus::Failed(newer.clone())
            ),
            Some(newer)
        );
    }

    /// The mounted strip's state follows the store's signal: it appears on
    /// Failed, stays through Pending and Failed again, goes away on Saved.
    #[test]
    fn the_mounted_strip_goes_away_when_a_save_succeeds() {
        Owner::new().with(|| {
            let status = RwSignal::new(SaveStatus::Saved);
            let shown = failure(status.into());
            assert_eq!(shown.get_untracked(), None);
            status.set(SaveStatus::Failed(SaveError::Transient));
            assert_eq!(shown.get_untracked(), Some(SaveError::Transient));
            status.set(SaveStatus::Pending);
            assert_eq!(shown.get_untracked(), Some(SaveError::Transient));
            status.set(SaveStatus::Failed(SaveError::Transient));
            assert_eq!(shown.get_untracked(), Some(SaveError::Transient));
            status.set(SaveStatus::Saved);
            assert_eq!(shown.get_untracked(), None);
        });
    }

    /// Export reads what to offer at the click: a transient failure goes on
    /// taking saves, and the file is the latest state, not the one the strip
    /// was built on.
    #[test]
    fn export_is_read_at_the_click() {
        Owner::new().with(|| {
            let (first, second) = (progress_fixture(), later());
            let store = FakeStore::new(Load::Ok(first.clone()));
            let files = crate::test_support::FakeFiles::default();
            let backend = backend_of(&store, &files);
            store.script(&[Outcome::Transient, Outcome::Transient]);
            backend.save(&first);
            let error = SaveError::Transient;
            // The strip is mounted on `first`; a later save fails too.
            backend.save(&second);
            export_for(backend, &error);
            assert_eq!(files.exported()[0].1, second.to_json());
        });
    }

    /// [`strip`] in English.
    fn strip_of(store: &FakeStore) -> String {
        strip(Language::En, store)
    }

    #[test]
    fn exporting_with_nothing_to_export_calls_nothing() {
        Owner::new().with(|| {
            let (_store, files) = provide_fakes(Load::Empty);
            export_for(Backend::current(), &SaveError::Transient);
            assert_eq!(files.exported().len(), 0);
        });
    }
}
