//! The data block: where the progress lives and how to move it. Export
//! downloads the state as `pc_learn-<date>.json`; Import validates a file,
//! then a Dialog confirms before it replaces the whole state (a refusal
//! says why and changes nothing). Both go through [`crate::store`].

use learn_core::date::Date;
use learn_core::progress::State;
use leptos::html;
use leptos::prelude::*;
use leptos_i18n::td_string;
use palicanon_ui_lib::{Button, Dialog, Disclosed, Disclosure, Variant};

use crate::catalogue::strings;
use crate::store::{self, ImportError, export_name, validate_import};

/// Where the progress lives, and how to move it: export, import (with its
/// confirmation and its refusal): two Accent buttons at the right, nothing
/// else on view.
#[component]
pub(crate) fn DataBlock(
    /// What Export downloads; `None`: nothing to export yet.
    export_state: Option<State>,
    /// The session's today, for the file name.
    today: Date,
    /// Import is available.
    can_import: bool,
    /// Called with an imported state once confirmed.
    on_replace: Callback<State>,
) -> impl IntoView {
    let dialog = Disclosed::new("pc-import", RwSignal::new(false));
    let pending = RwSignal::new(None::<State>);
    let refusal = RwSignal::new(None::<ImportError>);
    view! {
        <section class="flex flex-col gap-3">
            <div class="flex flex-wrap justify-end gap-3">
                <ExportButton state=export_state today=today />
                <ImportPicker can_import=can_import dialog=dialog pending=pending refusal=refusal />
            </div>
            <ImportStatus error=refusal.into() />
            <ImportDialog dialog=dialog pending=pending on_replace=on_replace />
        </section>
    }
}

/// Downloads the state as `pc_learn-<date>.json`; disabled with nothing to
/// export.
#[component]
fn ExportButton(
    /// What to export.
    state: Option<State>,
    /// The session's today, for the file name.
    today: Date,
) -> impl IntoView {
    let t = strings();
    let disabled = state.is_none();
    let export = move |_| {
        if let Some(state) = &state {
            store::download(&export_name(today), &state.to_json());
        }
    };
    view! {
        <Button variant=Variant::Accent disabled=disabled on_click=export>
            {move || td_string!(t.locale(), export)}
        </Button>
    }
}

/// The Import button and the hidden file input it opens. A chosen file is
/// validated: a state waits in `pending` for the confirmation (the Dialog
/// opens), a refusal goes to `refusal`.
#[component]
fn ImportPicker(
    /// Import is available.
    can_import: bool,
    /// The confirmation Dialog.
    dialog: Disclosed,
    /// The validated state waiting for 'Replace'.
    pending: RwSignal<Option<State>>,
    /// Why the last file was refused.
    refusal: RwSignal<Option<ImportError>>,
) -> impl IntoView {
    let t = strings();
    let input = NodeRef::<html::Input>::new();
    let on_change = move |_| {
        refusal.set(None);
        store::read_file(input, move |text| match validate_import(&text) {
            Ok(state) => {
                pending.set(Some(state));
                dialog.set_open(true);
            }
            Err(error) => refusal.set(Some(error)),
        });
        reset_picker(input);
    };
    view! {
        <Button variant=Variant::Accent disabled=!can_import on_click=move |_| open_picker(input)>
            {move || td_string!(t.locale(), import)}
        </Button>
        <input
            node_ref=input
            type="file"
            accept=".json,application/json"
            class="hidden"
            tabindex="-1"
            aria-hidden="true"
            on:change=on_change
        />
    }
}

/// Opens the browser's file chooser (hydrate only).
fn open_picker(input: NodeRef<html::Input>) {
    #[cfg(feature = "hydrate")]
    if let Some(input) = input.get_untracked() {
        input.click();
    }
    #[cfg(not(feature = "hydrate"))]
    let _ = input;
}

/// Forgets the chosen file, so that choosing the same one again fires
/// `change` (hydrate only).
fn reset_picker(input: NodeRef<html::Input>) {
    #[cfg(feature = "hydrate")]
    if let Some(input) = input.get_untracked() {
        input.set_value("");
    }
    #[cfg(not(feature = "hydrate"))]
    let _ = input;
}

/// Why an import was refused, in a live region (empty while nothing was
/// refused).
#[component]
fn ImportStatus(
    /// The last refusal.
    error: Signal<Option<ImportError>>,
) -> impl IntoView {
    let t = strings();
    view! {
        <p role="alert" class="text-sm text-heading">
            {move || {
                error
                    .get()
                    .map(|error| match error {
                        ImportError::NotAState => td_string!(t.locale(), import_refused_json),
                        ImportError::Newer => td_string!(t.locale(), import_refused_newer),
                        ImportError::DuplicateTerm => {
                            td_string!(t.locale(), import_refused_duplicate)
                        }
                    })
            }}
        </p>
    }
}

/// 'Import replaces the progress in this browser': Replace hands the
/// validated state to `on_replace`, Cancel forgets it.
#[component]
fn ImportDialog(
    /// What opens and closes it.
    dialog: Disclosed,
    /// The validated state waiting for 'Replace'.
    pending: RwSignal<Option<State>>,
    /// Called with the state on 'Replace'.
    on_replace: Callback<State>,
) -> impl IntoView {
    let t = strings();
    let title = Signal::derive(move || td_string!(t.locale(), import_title).to_string());
    // Closed and emptied before `on_replace` runs: it renders the page again.
    let replace = move |_| {
        let state = pending.get_untracked();
        pending.set(None);
        dialog.set_open(false);
        if let Some(state) = state {
            on_replace.run(state);
        }
    };
    let cancel = move |_| {
        pending.set(None);
        dialog.set_open(false);
    };
    let footer = move || {
        view! {
            <Button variant=Variant::Ghost on_click=cancel>
                {move || td_string!(t.locale(), cancel)}
            </Button>
            <Button variant=Variant::Primary on_click=replace>
                {move || td_string!(t.locale(), replace)}
            </Button>
        }
    };
    view! {
        <Dialog controls=dialog title=title footer=footer>
            <p>{move || td_string!(t.locale(), import_body)}</p>
        </Dialog>
    }
}

/// These tests render markup on the server: they need `ssr`.
#[cfg(all(test, feature = "ssr"))]
mod tests {
    use palicanon_ui_lib::Language;

    use super::*;
    use crate::test_support::{button_labelled, position, progress_fixture, tag_around, today};

    /// The data block in `language`, exporting `state`, import on or off.
    fn block(state: Option<State>, can_import: bool, language: Language) -> String {
        Owner::new()
            .with(|| {
                palicanon_ui_lib::provide_language().set(language);
                let on_replace = Callback::new(|_: State| {});
                view! { <DataBlock export_state=state today=today() can_import=can_import on_replace=on_replace /> }
                .to_html()
            })
            .replace("<!>", "")
    }

    #[test]
    fn the_data_block_offers_export_and_import() {
        let html = block(Some(progress_fixture()), true, Language::En);
        for text in [">Export<", ">Import<"] {
            assert!(html.contains(text), "{text}: {html}");
        }
        let input = tag_around(&html, r#"type="file""#);
        assert!(input.contains(".json"), "{input}");
        assert!(!html.contains("This file"), "no refusal yet: {html}");
        assert!(
            !button_labelled(&html, "Export").contains("disabled"),
            "{html}"
        );
        assert!(
            !button_labelled(&html, "Import").contains("disabled"),
            "{html}"
        );
    }

    #[test]
    fn nothing_to_export_disables_export_and_import_can_wait() {
        let html = block(None, false, Language::En);
        assert!(
            button_labelled(&html, "Export").contains("disabled"),
            "{html}"
        );
        assert!(
            button_labelled(&html, "Import").contains("disabled"),
            "{html}"
        );
        let html = block(None, true, Language::En);
        assert!(
            button_labelled(&html, "Export").contains("disabled"),
            "{html}"
        );
        assert!(
            !button_labelled(&html, "Import").contains("disabled"),
            "{html}"
        );
    }

    /// The block is two buttons: no heading, no sentence, no 'not synced'
    /// line (user, 2026-10-06).
    #[test]
    fn the_data_block_has_no_heading_no_sentence_and_no_sync_line() {
        for (language, gone) in [
            (
                Language::En,
                [
                    "Your progress is on this device",
                    "kept in this browser",
                    "Not synced",
                ],
            ),
            (
                Language::Fr,
                [
                    "Votre progression est sur cet appareil",
                    "dans ce navigateur seulement",
                    "Pas encore synchronisée",
                ],
            ),
            (
                Language::Vi,
                [
                    "nằm trên thiết bị này",
                    "chỉ được lưu trong trình duyệt này",
                    "Chưa đồng bộ",
                ],
            ),
        ] {
            let html = block(Some(progress_fixture()), true, language);
            for text in gone {
                assert!(!html.contains(text), "{language:?} {text}: {html}");
            }
            // The buttons' row is the block's first child: no heading above it.
            assert!(
                html.starts_with(
                    r#"<section class="flex flex-col gap-3"><div class="flex flex-wrap justify-end gap-3">"#
                ),
                "{language:?}: {html}"
            );
        }
    }

    /// Export and Import sit at the right of their row, in Accent (gold
    /// border and tint); the import dialog's own buttons keep their look.
    #[test]
    fn export_and_import_are_accent_buttons_at_the_right() {
        let html = block(Some(progress_fixture()), true, Language::En);
        let row = tag_around(&html, "justify-end");
        for class in ["flex", "flex-wrap", "justify-end", "gap-3"] {
            assert!(row.split(['"', ' ']).any(|c| c == class), "{class}: {row}");
        }
        assert!(
            position(&html, "justify-end") < position(&html, ">Export<"),
            "{html}"
        );
        for label in ["Export", "Import"] {
            let button = tag_around(&html, &format!(">{label}<"));
            let classes: Vec<&str> = button.split(['"', ' ']).collect();
            assert!(classes.contains(&"border-gold/40"), "{label}: {button}");
            assert!(!classes.contains(&"border-line"), "{label}: {button}");
        }
        for label in ["Replace", "Cancel"] {
            let button = tag_around(&html, &format!(">{label}<"));
            assert!(!button.contains("border-gold/40"), "{label}: {button}");
        }
    }

    #[test]
    fn the_import_dialog_confirms_before_replacing() {
        let html = block(Some(progress_fixture()), true, Language::En);
        let dialog = tag_around(&html, r#"role="dialog""#);
        assert!(dialog.contains("aria-modal"), "{dialog}");
        let title = &html[position(&html, "Import replaces the progress in this browser")..];
        assert!(title.contains("replaced, as a whole"), "{html}");
        assert!(html.contains(">Replace<"), "{html}");
        assert!(html.contains(">Cancel<"), "{html}");
        let dialog_tag = tag_around(&html, r#"class="fixed inset-0 z-50""#);
        assert!(dialog_tag.contains("hidden"), "closed: {dialog_tag}");
    }

    #[test]
    fn it_reads_in_french() {
        let html = block(Some(progress_fixture()), true, Language::Fr);
        assert!(!html.contains("Not synced"), "{html}");
        assert!(!html.contains(">Export<"), "{html}");
    }

    #[test]
    fn a_refusal_names_its_reason_in_an_alert() {
        let reasons = [
            (ImportError::NotAState, "This file is not a progress file"),
            (ImportError::Newer, "This file was saved by a newer version"),
            (ImportError::DuplicateTerm, "This file lists a term twice"),
        ];
        for (error, text) in reasons {
            let html = Owner::new().with(|| {
                palicanon_ui_lib::provide_language().set(Language::En);
                let error = Signal::from(Some(error));
                view! { <ImportStatus error=error /> }.to_html()
            });
            assert!(html.contains(r#"role="alert""#), "{html}");
            assert!(html.contains(text), "{html}");
        }
    }

    #[test]
    fn no_refusal_leaves_the_alert_empty() {
        let html = Owner::new().with(|| {
            let error = Signal::from(None::<ImportError>);
            view! { <ImportStatus error=error /> }.to_html()
        });
        assert!(html.contains(r#"role="alert""#), "{html}");
        assert!(!html.contains("This file"), "{html}");
    }
}
