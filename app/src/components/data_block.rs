//! 'Your progress', a section of the shell's Settings: where the progress
//! lives and how to move it. Export downloads the state as
//! `pc_learn-<date>.json`; Import validates a file, then a Dialog confirms
//! before it replaces the whole state (a refusal says why and changes
//! nothing). Both go through [`crate::store`].
//!
//! [`ProgressSection`] is given to `AppShell` as `settings_extra`, after
//! Appearance. [`ImportConfirm`], the confirmation, is rendered outside
//! Settings, after the shell: opening it closes Settings (ui-lib's
//! one-open rule), and a dialog inside a hidden one would not show. The
//! section reads the store itself: after hydration, each time Settings
//! opens, and when another tab changes the progress. After 'Replace',
//! [`StoreWrites`] moves, and the home, which follows it, reads the store
//! again: the browser's `storage` event reaches the other tabs only.

use learn_core::date::Date;
use learn_core::progress::State;
use leptos::html;
use leptos::prelude::*;
use leptos_i18n::td_string;
use palicanon_ui_lib::{Button, Dialog, DialogSection, Disclosed, Disclosure, ShellState, Variant};

use crate::catalogue::strings;
use crate::store::{self, ImportError, Load, export_name, on_storage_change, validate_import};
use crate::today::{browser_today, session_today};

/// What the section offers, read from the store.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(crate) struct Stored {
    /// What Export downloads, with the session's today (the file's name);
    /// none: nothing saved, Export is off.
    export: Option<(State, Date)>,
    /// The progress was saved by a newer version: it is never replaced
    /// (Q3), Import is off.
    newer: bool,
}

/// What the section offers for `load`, `browser` the browser's date: the
/// export of a progress this build reads and that holds a term; Import off
/// for a progress saved by a newer version only (a missing or damaged one
/// may be replaced).
pub(crate) fn stored(load: &Load, browser: Date) -> Stored {
    match load {
        Load::Ok(state) if !state.progress.is_empty() => Stored {
            export: Some((state.clone(), session_today(browser, state))),
            newer: false,
        },
        Load::NewerVersion { .. } => Stored {
            export: None,
            newer: true,
        },
        Load::Ok(_) | Load::Empty | Load::Invalid { .. } => Stored::default(),
    }
}

/// How many times this tab replaced the progress from Settings, provided
/// by the app's layout: the home reads the store again when it moves.
#[derive(Debug, Clone, Copy)]
pub(crate) struct StoreWrites(pub(crate) RwSignal<u64>);

/// The state [`ProgressSection`] (in Settings) and [`ImportConfirm`]
/// (outside it) share.
#[derive(Clone, Copy)]
pub(crate) struct ProgressData {
    /// The store, as last read.
    stored: RwSignal<Stored>,
    /// The confirmation Dialog.
    dialog: Disclosed,
    /// The validated state waiting for 'Replace'.
    pending: RwSignal<Option<State>>,
    /// Why the last file was refused.
    refusal: RwSignal<Option<ImportError>>,
    /// Moved after each 'Replace'.
    writes: StoreWrites,
}

impl ProgressData {
    /// Nothing read yet, nothing pending, the confirmation closed.
    pub(crate) fn new() -> Self {
        ProgressData {
            stored: RwSignal::new(Stored::default()),
            dialog: Disclosed::new("pc-import", RwSignal::new(false)),
            pending: RwSignal::new(None),
            refusal: RwSignal::new(None),
            writes: StoreWrites(RwSignal::new(0)),
        }
    }

    /// What moves after each 'Replace', for the home.
    pub(crate) fn writes(self) -> StoreWrites {
        self.writes
    }

    /// Reads the store and the clock again.
    fn read(self) {
        let next = stored(&store::load(), browser_today());
        if self.stored.with_untracked(|stored| *stored != next) {
            self.stored.set(next);
        }
    }
}

/// 'Your progress', a section of Settings: Export and Import (Accent),
/// side by side, the reason Import is off, the last refusal. Reads the
/// store after hydration, each time Settings opens, and when another tab
/// changes it. Built again when the language changes (a section's title
/// is fixed once built).
#[component]
pub(crate) fn ProgressSection(
    /// The state shared with [`ImportConfirm`].
    data: ProgressData,
) -> impl IntoView {
    let settings_open = use_context::<ShellState>().map(|shell| shell.settings_open);
    Effect::new(move |_| {
        if let Some(open) = settings_open {
            open.track();
        }
        data.read();
    });
    on_storage_change(move || data.read());
    let t = strings();
    move || {
        let title = td_string!(t.locale(), settings_progress);
        view! {
            <DialogSection title=title>
                <ProgressActions data=data />
            </DialogSection>
        }
    }
}

/// The section's content: the two buttons, the reason Import is off (a
/// progress saved by a newer version), the last refusal.
#[component]
fn ProgressActions(
    /// The state shared with [`ImportConfirm`].
    data: ProgressData,
) -> impl IntoView {
    let t = strings();
    let newer = Signal::derive(move || data.stored.with(|stored| stored.newer));
    view! {
        <div class="flex flex-col gap-3">
            <div class="flex flex-wrap gap-3">
                <ExportButton stored=data.stored />
                <ImportPicker data=data newer=newer />
            </div>
            {move || {
                newer
                    .get()
                    .then(|| {
                        view! {
                            <p class="text-sm text-muted">
                                {move || td_string!(t.locale(), import_off_newer)}
                            </p>
                        }
                    })
            }}
            <ImportStatus error=data.refusal.into() />
        </div>
    }
}

/// Downloads the state as `pc_learn-<date>.json`; disabled with nothing to
/// export.
#[component]
fn ExportButton(
    /// The store, as last read.
    stored: RwSignal<Stored>,
) -> impl IntoView {
    let t = strings();
    let disabled = Signal::derive(move || stored.with(|stored| stored.export.is_none()));
    let export = move |_| {
        stored.with_untracked(|stored| {
            if let Some((state, today)) = &stored.export {
                store::download(&export_name(*today), &state.to_json());
            }
        });
    };
    view! {
        <Button variant=Variant::Accent disabled=disabled on_click=export>
            {move || td_string!(t.locale(), export)}
        </Button>
    }
}

/// The Import button and the hidden file input it opens. A chosen file is
/// validated: a state waits in `pending` for the confirmation (the Dialog
/// opens, Settings closes), a refusal goes to `refusal`.
#[component]
fn ImportPicker(
    /// The state shared with [`ImportConfirm`].
    data: ProgressData,
    /// The stored progress was saved by a newer version: Import is off.
    newer: Signal<bool>,
) -> impl IntoView {
    let t = strings();
    let input = NodeRef::<html::Input>::new();
    let ProgressData {
        dialog,
        pending,
        refusal,
        ..
    } = data;
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
        <Button variant=Variant::Accent disabled=newer on_click=move |_| open_picker(input)>
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

/// 'Import replaces the progress in this browser', outside Settings:
/// 'Replace' saves the validated state (unless the store now holds a
/// progress saved by a newer version, never replaced), moves
/// [`StoreWrites`] and reads the store again; 'Cancel' forgets it.
#[component]
pub(crate) fn ImportConfirm(
    /// The state shared with [`ProgressSection`].
    data: ProgressData,
) -> impl IntoView {
    let t = strings();
    let ProgressData {
        dialog, pending, ..
    } = data;
    let title = Signal::derive(move || td_string!(t.locale(), import_title).to_string());
    // Closed and emptied before the home reads the store again.
    let replace = move |_| {
        let state = pending.get_untracked();
        pending.set(None);
        dialog.set_open(false);
        if let Some(state) = state {
            if replace_stored(&state) {
                data.writes.0.update(|writes| *writes += 1);
            }
            data.read();
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

/// Saves `state` over the stored progress, unless the store holds one
/// saved by a newer version (Q3: never overwritten, even when another tab
/// wrote it after Settings read the store); whether it was saved.
fn replace_stored(state: &State) -> bool {
    if matches!(store::load(), Load::NewerVersion { .. }) {
        return false;
    }
    store::save(state);
    true
}

/// These tests render markup on the server: they need `ssr`.
#[cfg(all(test, feature = "ssr"))]
mod tests {
    use palicanon_ui_lib::Language;

    use super::*;
    use crate::test_support::{button_labelled, position, progress_fixture, tag_around, today};

    /// The stored progress of the fixture, exported on `today`.
    fn saved() -> Stored {
        Stored {
            export: Some((progress_fixture(), today())),
            newer: false,
        }
    }

    /// The Settings section in `language`, the store read as `stored`.
    fn section(stored: Stored, language: Language) -> String {
        Owner::new()
            .with(|| {
                palicanon_ui_lib::provide_language().set(language);
                let data = ProgressData::new();
                data.stored.set(stored);
                view! { <ProgressSection data=data /> }.to_html()
            })
            .replace("<!>", "")
    }

    /// The import's confirmation, in English.
    fn confirm() -> String {
        Owner::new()
            .with(|| {
                palicanon_ui_lib::provide_language().set(Language::En);
                view! { <ImportConfirm data=ProgressData::new() /> }.to_html()
            })
            .replace("<!>", "")
    }

    /// 'Your progress', a section of Settings, holding Export and Import,
    /// in every language.
    #[test]
    fn the_section_is_your_progress_with_export_and_import() {
        for (language, title, export, import) in [
            (Language::En, "Your progress", "Export", "Import"),
            (Language::Fr, "Votre progression", "Exporter", "Importer"),
            (Language::Vi, "Tiến độ của bạn", "Xuất", "Nhập"),
        ] {
            let html = section(saved(), language);
            assert!(html.starts_with("<section"), "{html}");
            assert!(html.contains(&format!(">{title}</h3>")), "{html}");
            let order = [
                position(&html, &format!(">{title}</h3>")),
                position(&html, &format!(">{export}<")),
                position(&html, &format!(">{import}<")),
            ];
            assert!(order.is_sorted(), "{language:?} {order:?}: {html}");
        }
    }

    #[test]
    fn with_a_progress_saved_export_and_import_are_on() {
        let html = section(saved(), Language::En);
        let input = tag_around(&html, r#"type="file""#);
        assert!(input.contains(".json"), "{input}");
        assert!(!html.contains("This file"), "no refusal yet: {html}");
        assert!(!html.contains("newer version"), "no reason: {html}");
        for label in ["Export", "Import"] {
            let button = button_labelled(&html, label);
            assert!(!button.contains("disabled"), "{label}: {button}");
        }
    }

    /// Nothing saved, or a damaged progress (both read as nothing to
    /// export): Export is off, Import is the way back.
    #[test]
    fn nothing_saved_disables_export_and_leaves_import_on() {
        let html = section(Stored::default(), Language::En);
        let export = button_labelled(&html, "Export");
        assert!(export.contains("disabled"), "{export}");
        let import = button_labelled(&html, "Import");
        assert!(!import.contains("disabled"), "{import}");
        assert!(!html.contains("newer version"), "no reason: {html}");
    }

    /// A progress saved by a newer version is never replaced (Q3): Import
    /// is off, a short line says why, in every language.
    #[test]
    fn a_newer_progress_disables_import_with_its_reason() {
        for (language, import, reason) in [
            (
                Language::En,
                "Import",
                "Import is off: your progress was saved by a newer version.",
            ),
            (
                Language::Fr,
                "Importer",
                "Import désactivé : votre progression vient d’une version plus récente.",
            ),
            (
                Language::Vi,
                "Nhập",
                "Không thể nhập: tiến độ của bạn được lưu bởi một phiên bản mới hơn.",
            ),
        ] {
            let stored = Stored {
                export: None,
                newer: true,
            };
            let html = section(stored, language);
            let button = button_labelled(&html, import);
            assert!(button.contains("disabled"), "{language:?}: {button}");
            assert!(
                position(&html, &format!(">{import}<")) < position(&html, reason),
                "{language:?}: {html}"
            );
        }
    }

    /// Export and Import are Accent (gold border and tint), side by side.
    #[test]
    fn export_and_import_are_accent_buttons() {
        let html = section(saved(), Language::En);
        let row = tag_around(&html, "flex flex-wrap gap-3");
        assert!(row.starts_with("<div"), "{row}");
        for label in ["Export", "Import"] {
            let button = tag_around(&html, &format!(">{label}<"));
            let classes: Vec<&str> = button.split(['"', ' ']).collect();
            assert!(classes.contains(&"border-gold/40"), "{label}: {button}");
            assert!(!classes.contains(&"border-line"), "{label}: {button}");
        }
    }

    /// The confirmation is no part of the section: opening it closes
    /// Settings (one dialog open at a time), and a dialog inside a hidden
    /// one would not show.
    #[test]
    fn the_section_holds_no_dialog() {
        let html = section(saved(), Language::En);
        assert!(!html.contains(r#"role="dialog""#), "{html}");
        assert!(!html.contains(">Replace<"), "{html}");
    }

    #[test]
    fn the_import_confirmation_asks_before_replacing() {
        let html = confirm();
        let dialog = tag_around(&html, r#"role="dialog""#);
        assert!(dialog.contains("aria-modal"), "{dialog}");
        assert!(dialog.contains(r#"id="pc-import""#), "{dialog}");
        let title = &html[position(&html, "Import replaces the progress in this browser")..];
        assert!(title.contains("replaced, as a whole"), "{html}");
        for label in ["Replace", "Cancel"] {
            let button = tag_around(&html, &format!(">{label}<"));
            assert!(!button.contains("border-gold/40"), "{label}: {button}");
        }
        let root = tag_around(&html, r#"class="fixed inset-0 z-50""#);
        assert!(root.contains("hidden"), "closed: {root}");
    }

    #[test]
    fn the_section_has_no_sync_line_in_any_language() {
        for (language, gone) in [
            (
                Language::En,
                ["Your progress is on this device", "Not synced"],
            ),
            (
                Language::Fr,
                [
                    "Votre progression est sur cet appareil",
                    "Pas encore synchronisée",
                ],
            ),
            (Language::Vi, ["nằm trên thiết bị này", "Chưa đồng bộ"]),
        ] {
            let html = section(saved(), language);
            for text in gone {
                assert!(!html.contains(text), "{language:?} {text}: {html}");
            }
        }
    }

    /// What the store holds, as the section offers it.
    #[test]
    fn the_store_decides_export_and_import() {
        let state = progress_fixture();
        assert_eq!(
            stored(&Load::Ok(state.clone()), today()),
            Stored {
                export: Some((state, today())),
                newer: false,
            }
        );
        let mut later = progress_fixture();
        later.last_today = Some(today() + 2);
        assert_eq!(
            stored(&Load::Ok(later.clone()), today()).export,
            Some((later, today() + 2)),
            "the file is named after the session's today"
        );
        for nothing in [
            Load::Ok(State::empty()),
            Load::Empty,
            Load::Invalid { raw: "x".into() },
        ] {
            assert_eq!(stored(&nothing, today()), Stored::default(), "{nothing:?}");
        }
        assert_eq!(
            stored(&Load::NewerVersion { raw: "x".into() }, today()),
            Stored {
                export: None,
                newer: true,
            }
        );
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
