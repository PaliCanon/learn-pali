//! 'Your progress', a section of the shell's Settings: where the progress
//! lives and how to move it. Export downloads the state as
//! `pc_learn-<date>.json`; Import validates a file, then a Dialog confirms
//! before it replaces the whole state (a refusal says why and changes
//! nothing). Both go through the host's [`Backend`]: its store and its file
//! transfer.
//!
//! [`ProgressSection`] is given to `AppShell` as `settings_extra`, after
//! Appearance. [`ImportConfirm`], the confirmation, is rendered outside
//! Settings, after the shell: opening it closes Settings (ui-lib's
//! one-open rule), and a dialog inside a hidden one would not show. The
//! section reads the store itself: after hydration, each time Settings
//! opens, and when the store says the progress changed under it
//! ([`ProgressData::read`], called by the provider's `on_change` handler).
//! After 'Replace', [`StoreWrites`] moves, and the home, which follows it,
//! reads the store again: the store's own `on_change` is for changes it did
//! not make through `save`.

use learn_core::date::Date;
use learn_core::progress::State;
use leptos::prelude::*;
use leptos_i18n::td_string;
use palicanon_ui_lib::{Button, Dialog, DialogSection, Disclosed, Disclosure, ShellState, Variant};

use crate::backend::{Backend, SaveError};
use crate::catalogue::strings;
use crate::components::save_strip::failure;
use crate::store::{ImportError, Load, export_name, validate_import};
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
    /// The store could not read the progress: nothing is exported, nothing
    /// is replaced, Export and Import are off.
    unreadable: bool,
}

/// What the section offers for `load`, `browser` the browser's date: the
/// export of a progress this build reads and that holds a term; Import off
/// for a progress saved by a newer version or one that could not be read
/// (a missing or damaged one may be replaced).
pub(crate) fn stored(load: &Load, browser: Date) -> Stored {
    match load {
        Load::Ok(state) if !state.progress.is_empty() => Stored {
            export: Some((state.clone(), session_today(browser, state))),
            ..Stored::default()
        },
        Load::NewerVersion { .. } => Stored {
            newer: true,
            ..Stored::default()
        },
        Load::Unreadable(_) => Stored {
            unreadable: true,
            ..Stored::default()
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
    /// The host's store and file transfer.
    backend: Backend,
    /// The store, as last read.
    stored: RwSignal<Stored>,
    /// The failure the strip shows: kept from the store's `Failed` until a
    /// save succeeds, and held here (the provider's), so it survives the
    /// page being left and entered again.
    pub(crate) failure: Memo<Option<SaveError>>,
    /// The confirmation Dialog.
    dialog: Disclosed,
    /// The validated state waiting for 'Replace'.
    pending: RwSignal<Option<State>>,
    /// Why the last file was refused.
    refusal: RwSignal<Option<ImportError>>,
    /// The last pick failed (no chooser, an unreadable file): its own
    /// reason, apart from a refusal.
    pick_failed: RwSignal<bool>,
    /// Moved after each 'Replace'.
    writes: StoreWrites,
}

impl ProgressData {
    /// Nothing read yet, nothing pending, the confirmation closed; over the
    /// [`Backend`] of the nearest provider.
    pub(crate) fn new() -> Self {
        let backend = Backend::current();
        ProgressData {
            backend,
            failure: failure(backend.status()),
            stored: RwSignal::new(Stored::default()),
            dialog: Disclosed::new("pc-import", RwSignal::new(false)),
            pending: RwSignal::new(None),
            refusal: RwSignal::new(None),
            pick_failed: RwSignal::new(false),
            writes: StoreWrites(RwSignal::new(0)),
        }
    }

    /// The data the nearest [`LearnProvider`](crate::fragment::LearnProvider)
    /// provided.
    ///
    /// # Panics
    /// When there is none.
    #[track_caller]
    pub(crate) fn current() -> Self {
        use_context::<ProgressData>().expect("Learn needs a LearnProvider above it")
    }

    /// What moves after each 'Replace', for the home.
    pub(crate) fn writes(self) -> StoreWrites {
        self.writes
    }

    /// Says the stored progress changed: the home reads it again.
    pub(crate) fn moved(self) {
        self.writes.0.update(|writes| *writes += 1);
    }

    /// Reads the store and the clock again.
    pub(crate) fn read(self) {
        let next = stored(&self.backend.load(), browser_today());
        if self.stored.with_untracked(|stored| *stored != next) {
            self.stored.set(next);
        }
    }
}

/// 'Your progress', a section of Settings: Export and Import (Accent),
/// side by side, the reason Import is off, the last refusal. Reads the
/// store after hydration, each time Settings opens, and when the store
/// says the progress changed under it. Built again when the language changes (a section's title
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

/// The section's content: the two buttons, the reason they are off (a
/// progress saved by a newer version, or one that could not be read), the
/// last refusal.
#[component]
fn ProgressActions(
    /// The state shared with [`ImportConfirm`].
    data: ProgressData,
) -> impl IntoView {
    let t = strings();
    let newer = Signal::derive(move || data.stored.with(|stored| stored.newer));
    let unreadable = Signal::derive(move || data.stored.with(|stored| stored.unreadable));
    let off = Signal::derive(move || newer.get() || unreadable.get());
    view! {
        <div class="flex flex-col gap-3">
            <div class="flex flex-wrap gap-3">
                <ExportButton data=data />
                <ImportPicker data=data off=off />
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
            {move || {
                unreadable
                    .get()
                    .then(|| {
                        view! {
                            <p class="text-sm text-muted">
                                {move || td_string!(t.locale(), import_off_unreadable)}
                            </p>
                        }
                    })
            }}
            <ImportStatus error=data.refusal.into() />
            <PickStatus failed=data.pick_failed.into() />
        </div>
    }
}

/// Downloads the state as `pc_learn-<date>.json`; disabled with nothing to
/// export.
#[component]
fn ExportButton(
    /// The state shared with [`ImportConfirm`].
    data: ProgressData,
) -> impl IntoView {
    let t = strings();
    let stored = data.stored;
    let disabled = Signal::derive(move || stored.with(|stored| stored.export.is_none()));
    let export = move |_| export_stored(data);
    view! {
        <Button variant=Variant::Accent disabled=disabled on_click=export>
            {move || td_string!(t.locale(), export)}
        </Button>
    }
}

/// Export: hands the stored progress, as `pc_learn-<date>.json`, to the
/// host's file transfer; nothing when there is nothing to export.
fn export_stored(data: ProgressData) {
    data.stored.with_untracked(|stored| {
        if let Some((state, today)) = &stored.export {
            data.backend.export(&export_name(*today), &state.to_json());
        }
    });
}

/// Import: asks the host's file transfer for a file. A chosen file is
/// validated: a state waits in `pending` for the confirmation (the Dialog
/// opens, Settings closes), a refusal goes to `refusal`; a file that could
/// not be read arrives as empty text and is refused the same way.
fn begin_import(data: ProgressData) {
    let ProgressData {
        backend,
        dialog,
        pending,
        refusal,
        pick_failed,
        ..
    } = data;
    pick_failed.set(false);
    backend.pick_import(move |picked| {
        let Ok(text) = picked else {
            // A failure of its own: shown apart, and the refusal of an
            // earlier file is left as it was.
            pick_failed.set(true);
            return;
        };
        // Only a file that was read clears the last refusal: a cancelled
        // chooser calls nothing back and leaves it.
        refusal.set(None);
        match validate_import(&text) {
            Ok(state) => {
                pending.set(Some(state));
                dialog.set_open(true);
            }
            Err(error) => refusal.set(Some(error)),
        }
    });
}

/// The Import button ([`begin_import`]).
#[component]
fn ImportPicker(
    /// The state shared with [`ImportConfirm`].
    data: ProgressData,
    /// Import is off: the stored progress was saved by a newer version, or
    /// could not be read.
    off: Signal<bool>,
) -> impl IntoView {
    let t = strings();
    let on_click = move |_| begin_import(data);
    view! {
        <Button variant=Variant::Accent disabled=off on_click=on_click>
            {move || td_string!(t.locale(), import)}
        </Button>
    }
}

/// 'The file could not be opened', in a live region (empty while the last
/// pick did not fail): a pick that failed, apart from a refusal.
#[component]
fn PickStatus(
    /// The last pick failed.
    failed: Signal<bool>,
) -> impl IntoView {
    let t = strings();
    view! {
        <p role="alert" class="text-sm text-heading">
            {move || failed.get().then(|| td_string!(t.locale(), import_pick_failed))}
        </p>
    }
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
/// progress saved by a newer version or could not be read, never
/// replaced), moves
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
    let replace = move |_| confirm_replace(data);
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

/// 'Replace': the pending state is forgotten and the confirmation closed
/// before the home reads the store again; it is saved over the stored
/// progress unless the store must keep it ([`replace_stored`]), and only
/// then the writes counter moves.
fn confirm_replace(data: ProgressData) {
    let state = data.pending.get_untracked();
    data.pending.set(None);
    data.dialog.set_open(false);
    if let Some(state) = state {
        if replace_stored(data.backend, &state) {
            data.moved();
        }
        data.read();
    }
}

/// Saves `state` over the stored progress, unless the store holds one
/// saved by a newer version (Q3: never overwritten, even when another tab
/// wrote it after Settings read the store) or could not read it; whether it
/// was saved.
fn replace_stored(backend: Backend, state: &State) -> bool {
    if matches!(
        backend.load(),
        Load::NewerVersion { .. } | Load::Unreadable(_)
    ) {
        return false;
    }
    backend.save(state);
    true
}

/// These tests render markup on the server: they need `ssr`.
#[cfg(all(test, feature = "ssr"))]
mod tests {
    use palicanon_ui_lib::Language;

    use super::*;
    use crate::test_support::{
        button_labelled, position, progress_fixture, provide_fakes, tag_around, today,
    };

    /// The stored progress of the fixture, exported on `today`.
    fn saved() -> Stored {
        Stored {
            export: Some((progress_fixture(), today())),
            ..Stored::default()
        }
    }

    /// The Settings section in `language`, the store read as `stored`.
    fn section(stored: Stored, language: Language) -> String {
        Owner::new()
            .with(|| {
                palicanon_ui_lib::provide_language().set(language);
                provide_fakes(Load::Empty);
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
                provide_fakes(Load::Empty);
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
        // The file chooser is the host's: the section holds no input.
        assert!(!html.contains(r#"type="file""#), "{html}");
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
                newer: true,
                ..Stored::default()
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
                ..Stored::default()
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
                newer: true,
                ..Stored::default()
            }
        );
    }

    /// A progress the store could not read offers neither Export nor Import.
    #[test]
    fn an_unreadable_store_turns_export_and_import_off() {
        assert_eq!(
            stored(&Load::Unreadable("permission denied".into()), today()),
            Stored {
                unreadable: true,
                ..Stored::default()
            }
        );
    }

    /// A progress the store could not read: both buttons off, the reason
    /// in a line, in every language.
    #[test]
    fn an_unreadable_progress_turns_both_buttons_off_with_its_reason() {
        for (language, export, import, reason) in [
            (
                Language::En,
                "Export",
                "Import",
                "Export and Import are off. Your progress could not be read.",
            ),
            (
                Language::Fr,
                "Exporter",
                "Importer",
                "Export et import désactivés. Votre progression n’a pas pu être lue.",
            ),
            (
                Language::Vi,
                "Xuất",
                "Nhập",
                "Xuất và nhập đang tắt. Không đọc được tiến độ của bạn.",
            ),
        ] {
            let stored = Stored {
                unreadable: true,
                ..Stored::default()
            };
            let html = section(stored, language);
            for label in [export, import] {
                let button = button_labelled(&html, label);
                assert!(
                    button.contains("disabled"),
                    "{language:?} {label}: {button}"
                );
            }
            assert!(html.contains(reason), "{language:?}: {html}");
            assert!(!html.contains("newer version"), "{language:?}: {html}");
        }
    }

    /// The section reads the store again when told to (the provider's
    /// `on_change` handler): a store that turned unreadable turns both
    /// buttons off, one that went back to a state offers its export.
    #[test]
    fn read_follows_what_the_store_holds_now() {
        Owner::new().with(|| {
            let (store, _files) = provide_fakes(Load::Ok(progress_fixture()));
            let data = ProgressData::new();
            data.read();
            assert!(data.stored.get_untracked().export.is_some());
            store.changed_to(Load::Unreadable("denied".into()));
            data.read();
            assert!(data.stored.get_untracked().unreadable);
            assert!(data.stored.get_untracked().export.is_none());
            store.changed_to(Load::NewerVersion { raw: "x".into() });
            data.read();
            assert!(data.stored.get_untracked().newer);
        });
    }

    /// Export hands the stored progress to the host under its dated name.
    #[test]
    fn export_hands_the_progress_to_the_file_transfer() {
        Owner::new().with(|| {
            let (_store, files) = provide_fakes(Load::Empty);
            let data = ProgressData::new();
            export_stored(data);
            assert_eq!(files.exported().len(), 0, "nothing to export");
            data.stored.set(saved());
            export_stored(data);
            assert_eq!(
                files.exported(),
                [(
                    "pc_learn-2026-10-05.json".to_owned(),
                    progress_fixture().to_json()
                )]
            );
        });
    }

    /// A chosen file waits for the confirmation; a refused one says why and
    /// changes nothing.
    #[test]
    fn import_validates_the_chosen_file_whole() {
        Owner::new().with(|| {
            let (store, files) = provide_fakes(Load::Empty);
            let data = ProgressData::new();
            begin_import(data);
            assert_eq!(files.picks(), 1);
            files.choose("{nope");
            assert_eq!(data.refusal.get_untracked(), Some(ImportError::NotAState));
            assert_eq!(data.pending.get_untracked(), None);
            begin_import(data);
            files.choose(&progress_fixture().to_json());
            assert_eq!(data.refusal.get_untracked(), None, "a new choice clears it");
            assert_eq!(data.pending.get_untracked(), Some(progress_fixture()));
            assert!(
                data.dialog.signal().get_untracked(),
                "the confirmation opens"
            );
            assert_eq!(store.saved().len(), 0, "nothing is saved before Replace");
        });
    }

    /// A cancelled chooser changes nothing, the previous refusal included;
    /// a file that is read clears it.
    #[test]
    fn only_a_file_that_is_read_clears_the_previous_refusal() {
        Owner::new().with(|| {
            let (_store, files) = provide_fakes(Load::Empty);
            let data = ProgressData::new();
            begin_import(data);
            files.choose("{nope");
            assert_eq!(data.refusal.get_untracked(), Some(ImportError::NotAState));
            begin_import(data);
            files.cancel();
            assert_eq!(
                data.refusal.get_untracked(),
                Some(ImportError::NotAState),
                "a cancel keeps it"
            );
            assert_eq!(data.pending.get_untracked(), None);
            begin_import(data);
            files.choose(&format!(r#"{{"version": {}}}"#, State::VERSION + 1));
            assert_eq!(data.refusal.get_untracked(), Some(ImportError::Newer));
            begin_import(data);
            files.choose(&progress_fixture().to_json());
            assert_eq!(data.refusal.get_untracked(), None, "a read clears it");
            assert_eq!(data.pending.get_untracked(), Some(progress_fixture()));
        });
    }

    /// A chooser that cannot be made, or a file that cannot be read, is its
    /// own failure: it shows its own reason (not the refusal of a file that
    /// was read, and not empty text), and keeps a previous refusal as it
    /// was. A later read file takes it away.
    #[test]
    fn a_failed_pick_is_its_own_failure_and_keeps_the_previous_refusal() {
        Owner::new().with(|| {
            let (_store, files) = provide_fakes(Load::Empty);
            let data = ProgressData::new();
            begin_import(data);
            files.choose("{nope");
            assert_eq!(data.refusal.get_untracked(), Some(ImportError::NotAState));
            begin_import(data);
            files.unreadable();
            assert!(data.pick_failed.get_untracked(), "its own reason is shown");
            assert_eq!(
                data.refusal.get_untracked(),
                Some(ImportError::NotAState),
                "the previous refusal is untouched"
            );
            assert_eq!(data.pending.get_untracked(), None);
            begin_import(data);
            assert!(
                !data.pick_failed.get_untracked(),
                "a new attempt starts clean"
            );
            files.choose(&progress_fixture().to_json());
            assert!(!data.pick_failed.get_untracked());
            assert_eq!(data.refusal.get_untracked(), None);
        });
    }

    /// The failure of a pick reads in every language, in the live region.
    #[test]
    fn a_failed_pick_has_its_reason_in_every_language() {
        for (language, text) in [
            (
                Language::En,
                "The file could not be opened. Nothing was changed.",
            ),
            (
                Language::Fr,
                "Le fichier n’a pas pu être ouvert. Rien n’a été modifié.",
            ),
            (Language::Vi, "Không mở được tệp. Không có gì bị thay đổi."),
        ] {
            let html = Owner::new().with(|| {
                palicanon_ui_lib::provide_language().set(language);
                provide_fakes(Load::Empty);
                let data = ProgressData::new();
                data.pick_failed.set(true);
                view! { <PickStatus failed=data.pick_failed.into() /> }.to_html()
            });
            assert!(html.contains(r#"role="alert""#), "{html}");
            assert!(html.contains(text), "{language:?}: {html}");
        }
    }

    /// 'Replace' on a pending file: it saves through the store and moves
    /// the writes counter; over a progress the store must keep (a newer
    /// file, one it could not read) it saves nothing and moves nothing, and
    /// the confirmation still closes and forgets the file.
    #[test]
    fn the_confirmation_refuses_to_replace_what_must_be_kept() {
        Owner::new().with(|| {
            let (store, _files) = provide_fakes(Load::Empty);
            let data = ProgressData::new();
            data.pending.set(Some(progress_fixture()));
            confirm_replace(data);
            assert_eq!(store.saved(), [progress_fixture()]);
            assert_eq!(data.writes.0.get_untracked(), 1);
            assert_eq!(data.pending.get_untracked(), None);
            for kept in [
                Load::Unreadable("denied".into()),
                Load::NewerVersion { raw: "x".into() },
            ] {
                let (store, _files) = provide_fakes(kept);
                let data = ProgressData::new();
                data.pending.set(Some(progress_fixture()));
                data.dialog.set_open(true);
                confirm_replace(data);
                assert_eq!(store.saved().len(), 0);
                assert_eq!(data.writes.0.get_untracked(), 0);
                assert_eq!(data.pending.get_untracked(), None);
                assert!(!data.dialog.signal().get_untracked());
            }
        });
    }

    /// Replace saves through the store, except over a newer version or a
    /// progress that could not be read (Q3; the store refuses too).
    #[test]
    fn replace_saves_but_never_over_what_it_must_keep() {
        Owner::new().with(|| {
            let (store, _files) = provide_fakes(Load::Empty);
            let backend = Backend::current();
            assert!(replace_stored(backend, &progress_fixture()));
            assert_eq!(store.saved(), [progress_fixture()]);
            for kept in [
                Load::NewerVersion { raw: "x".into() },
                Load::Unreadable("denied".into()),
            ] {
                let (store, _files) = provide_fakes(kept);
                assert!(!replace_stored(Backend::current(), &progress_fixture()));
                assert_eq!(store.saved().len(), 0);
            }
        });
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
