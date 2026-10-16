//! The sync's UI (task 050): [`SyncPanel`] in Settings' 'Your progress'
//! (in `ProgressActions`, inside `LearnSettings`), and [`SyncChip`] on the
//! home. Both render nothing when the host gave no transport
//! ([`SyncCycle::current`] is none: nothing changes from a device without
//! sync), and nothing before the cycle is mounted on the client: the
//! site's server, built without `web-sync`, has no transport and paints
//! the same.
//!
//! Signed out, the panel offers 'Sync with Google Drive' and says what is
//! stored where. Signed in (or remembered, the site after a reload), it
//! shows the account, where the sync stands ('Last synced at `<time>`', 'Not
//! synced.' with the reason), 'Sync now' ('Retry' after a failure) and
//! 'Sign out'; a Drive copy that could not be read is offered for download.
//! Remembered with no token at hand, it also says that Sign out cannot
//! revoke the access from here, with a link to the Google account.
//! The chip shows on the home while the account is remembered with no
//! token at hand: its tap asks Google for one, in the gesture, then syncs.

use leptos::prelude::*;
use leptos_i18n::td_string;
use palicanon_ui_lib::{Button, Variant};

use crate::backend::{Backend, SyncAccount, SyncError};
use crate::catalogue::{Locale, strings};
use crate::sync::{Refusal, SyncCycle, SyncStatus};

/// Where the learner removes the site's access to their Drive in their
/// Google account (connected apps).
pub(crate) const GOOGLE_CONNECTIONS: &str = "https://myaccount.google.com/connections";

/// The name of a Drive copy that could not be read, when downloaded.
pub(crate) const DRIVE_COPY_NAME: &str = "pc_learn-drive.json";

/// Hands the Drive copy `raw` to the host's file transfer, as is.
pub(crate) fn download_drive_copy(backend: Backend, raw: &str) {
    backend.export(DRIVE_COPY_NAME, raw);
}

/// The sync's part of 'Your progress'; nothing without a transport.
#[component]
pub(crate) fn SyncPanel() -> impl IntoView {
    let sync = SyncCycle::current();
    move || {
        let sync = sync.filter(|sync| sync.mounted())?;
        let account = sync.account();
        Some(view! {
            <div data-sync="" class="flex flex-col gap-3 border-t border-line pt-4">
                {move || match account.get() {
                    SyncAccount::SignedOut => view! { <SignedOut sync=sync /> }.into_any(),
                    SyncAccount::Remembered { email } => {
                        view! {
                            <SignedIn sync=sync email=email />
                            <RevokeNote />
                        }
                            .into_any()
                    }
                    SyncAccount::SignedIn { email } => {
                        view! { <SignedIn sync=sync email=email /> }.into_any()
                    }
                }}
            </div>
        })
    }
}

/// Signed out: 'Sync with Google Drive', and what is stored where.
#[component]
fn SignedOut(
    /// The page's sync.
    sync: SyncCycle,
) -> impl IntoView {
    let t = strings();
    view! {
        <div class="flex flex-wrap gap-3">
            <Button variant=Variant::Accent on_click=move |_| sync.sign_in()>
                {move || td_string!(t.locale(), sync_sign_in)}
            </Button>
        </div>
        <p class="text-sm text-muted">{move || td_string!(t.locale(), sync_explain)}</p>
    }
}

/// Signed in (or remembered): the account, where the sync stands, 'Sync
/// now' ('Retry' after a failure; off while a cycle runs) and 'Sign out'.
#[component]
fn SignedIn(
    /// The page's sync.
    sync: SyncCycle,
    /// The account's email, if Google gave it.
    email: Option<String>,
) -> impl IntoView {
    let t = strings();
    let status = sync.status();
    let busy = Signal::derive(move || status.with(|status| *status == SyncStatus::Syncing));
    let failed =
        Signal::derive(move || status.with(|status| matches!(status, SyncStatus::Failed(_))));
    let now = move || {
        if failed.get() {
            td_string!(t.locale(), sync_retry)
        } else {
            td_string!(t.locale(), sync_now)
        }
    };
    view! {
        <p class="text-sm text-heading">{move || account_line(t.locale(), email.as_deref())}</p>
        <SyncState sync=sync />
        <div class="flex flex-wrap gap-3">
            <Button variant=Variant::Accent disabled=busy on_click=move |_| sync.sync_now()>
                {now}
            </Button>
            <Button variant=Variant::Ghost on_click=move |_| sync.sign_out()>
                {move || td_string!(t.locale(), sync_sign_out)}
            </Button>
        </div>
    }
}

/// Remembered with no token at hand (the site after a reload): Sign out
/// cannot revoke the access at Google from here, so the line says where
/// the learner removes it, their Google account's connected apps.
#[component]
fn RevokeNote() -> impl IntoView {
    let t = strings();
    view! {
        <p class="text-sm text-muted">
            {move || td_string!(t.locale(), sync_revoke_note)} " "
            <a
                href=GOOGLE_CONNECTIONS
                target="_blank"
                rel="noopener noreferrer"
                class="underline underline-offset-2 hover:text-heading"
            >
                {move || td_string!(t.locale(), sync_revoke_link)}
            </a> "."
        </p>
    }
}

/// 'Google Drive: <email>', or 'Signed in to Google Drive' with no email.
fn account_line(locale: Locale, email: Option<&str>) -> String {
    match email {
        Some(email) => td_string!(locale, sync_account, email = email).to_string(),
        None => td_string!(locale, sync_account_unknown).to_string(),
    }
}

/// Where the sync stands, in words: nothing before the first cycle.
#[component]
fn SyncState(
    /// The page's sync.
    sync: SyncCycle,
) -> impl IntoView {
    let t = strings();
    let status = sync.status();
    let backend = Backend::current();
    move || {
        let locale = t.locale();
        match status.get() {
            SyncStatus::Idle => ().into_any(),
            SyncStatus::Syncing => muted(td_string!(locale, sync_syncing).to_string()),
            SyncStatus::Synced { at } => {
                muted(td_string!(locale, sync_last, time = at).to_string())
            }
            SyncStatus::Failed(error) => not_synced(locale, &error_reason(locale, &error)),
            SyncStatus::Refused(refusal) => not_synced(locale, refusal_reason(locale, refusal)),
            SyncStatus::DriveNewer => alert(td_string!(locale, sync_drive_newer).to_string()),
            SyncStatus::DriveUnreadable { raw } => {
                view! { <DriveUnreadable backend=backend raw=raw /> }.into_any()
            }
        }
    }
}

/// A muted line, in a polite live region.
fn muted(text: String) -> AnyView {
    view! {
        <p role="status" class="text-sm text-muted">
            {text}
        </p>
    }
    .into_any()
}

/// A line in an alert.
fn alert(text: String) -> AnyView {
    view! {
        <p role="alert" class="text-sm text-heading">
            {text}
        </p>
    }
    .into_any()
}

/// 'Not synced.' and the reason.
fn not_synced(locale: Locale, reason: &str) -> AnyView {
    alert(format!("{} {reason}", td_string!(locale, sync_failed)))
}

/// Why a call of the transport failed, in words.
fn error_reason(locale: Locale, error: &SyncError) -> String {
    match error {
        SyncError::SignedOut => td_string!(locale, sync_error_signed_out).to_string(),
        SyncError::Cancelled => td_string!(locale, sync_error_cancelled).to_string(),
        SyncError::Popup => td_string!(locale, sync_error_popup).to_string(),
        SyncError::Network => td_string!(locale, sync_error_network).to_string(),
        SyncError::Drive(detail) => {
            td_string!(locale, sync_error_drive, detail = detail).to_string()
        }
    }
}

/// Why this device's progress is not synced, in words.
fn refusal_reason(locale: Locale, refusal: Refusal) -> &'static str {
    match refusal {
        Refusal::Unreadable => td_string!(locale, sync_refused_unreadable),
        Refusal::Newer => td_string!(locale, sync_refused_newer),
        Refusal::Invalid => td_string!(locale, sync_refused_invalid),
        Refusal::NotSaved => td_string!(locale, sync_refused_not_saved),
    }
}

/// The Drive copy could not be read: nothing was changed, and the copy is
/// offered for download.
#[component]
fn DriveUnreadable(
    /// The host's file transfer.
    backend: Backend,
    /// The copy, as is.
    raw: String,
) -> impl IntoView {
    let t = strings();
    let raw = StoredValue::new(raw);
    let download = move |_| raw.with_value(|raw| download_drive_copy(backend, raw));
    view! {
        <p role="alert" class="text-sm text-heading">
            {move || td_string!(t.locale(), sync_drive_unreadable)}
        </p>
        <div class="flex flex-wrap gap-3">
            <Button variant=Variant::Ghost on_click=download>
                {move || td_string!(t.locale(), sync_drive_download)}
            </Button>
        </div>
    }
}

/// The home's 'Sync' chip: shown while the account is remembered with no
/// token at hand; its tap asks Google for one in the gesture, then syncs.
/// Nothing otherwise.
#[component]
pub(crate) fn SyncChip() -> impl IntoView {
    let t = strings();
    let sync = SyncCycle::current();
    move || {
        let sync = sync.filter(|sync| sync.mounted())?;
        matches!(sync.account().get(), SyncAccount::Remembered { .. }).then(|| {
            view! {
                <div class="flex justify-center">
                    <Button variant=Variant::Chip on_click=move |_| sync.sync_now()>
                        {move || td_string!(t.locale(), sync_chip)}
                    </Button>
                </div>
            }
        })
    }
}

/// These tests render markup on the server: they need `ssr`.
#[cfg(all(test, feature = "ssr"))]
mod tests {
    use std::rc::Rc;

    use palicanon_ui_lib::Language;

    use super::*;
    use crate::backend::{SharedSync, SyncAccount, SyncError};
    use crate::store::Load;
    use crate::sync::{Refusal, SyncStatus};
    use crate::test_support::{FakeTransport, button_labelled, progress_fixture, provide_fakes};

    fn email() -> SyncAccount {
        SyncAccount::SignedIn {
            email: Some("learner@example.org".into()),
        }
    }

    /// `view` rendered in `language` under fakes, with a sync over a fake
    /// transport for `account` whose status is `status` (none: no sync).
    fn render<V: IntoView + 'static>(
        sync: Option<(SyncAccount, SyncStatus)>,
        language: Language,
        view: impl FnOnce() -> V,
    ) -> String {
        Owner::new()
            .with(|| {
                palicanon_ui_lib::provide_language().set(language);
                provide_fakes(Load::Ok(progress_fixture()));
                if let Some((account, status)) = sync {
                    let transport: SharedSync = Rc::new(FakeTransport::new(account));
                    let cycle = SyncCycle::new(
                        transport,
                        Backend::current(),
                        || {},
                        String::new,
                        |_, _| {},
                    );
                    cycle.set_status(status);
                    cycle.set_mounted();
                    provide_context(cycle);
                }
                view().to_html()
            })
            .replace("<!>", "")
    }

    fn panel(account: SyncAccount, status: SyncStatus, language: Language) -> String {
        render(
            Some((account, status)),
            language,
            || view! { <SyncPanel /> },
        )
    }

    fn chip(account: SyncAccount) -> String {
        render(Some((account, SyncStatus::Idle)), Language::En, || {
            view! { <SyncChip /> }
        })
    }

    /// Before the provider is mounted on the client (the server's paint,
    /// the first hydration pass) the panel and the chip render nothing, so
    /// the server, which has no transport, paints the same.
    #[test]
    fn nothing_is_rendered_before_the_cycle_is_mounted() {
        let html = Owner::new()
            .with(|| {
                palicanon_ui_lib::provide_language().set(Language::En);
                provide_fakes(Load::Ok(progress_fixture()));
                let account = SyncAccount::Remembered { email: None };
                let transport: SharedSync = Rc::new(FakeTransport::new(account));
                let cycle =
                    SyncCycle::new(transport, Backend::current(), || {}, String::new, |_, _| {});
                provide_context(cycle);
                view! {
                    <SyncPanel />
                    <SyncChip />
                }
                .to_html()
            })
            .replace("<!>", "");
        assert_eq!(html, "", "{html}");
    }

    #[test]
    fn without_a_transport_nothing_is_rendered() {
        let html = render(None, Language::En, || view! { <SyncPanel /> });
        assert_eq!(html, "", "{html}");
        let html = render(None, Language::En, || view! { <SyncChip /> });
        assert_eq!(html, "", "{html}");
    }

    #[test]
    fn signed_out_offers_the_sign_in_and_says_what_is_stored_where() {
        for (language, sign_in, explain) in [
            (
                Language::En,
                "Sync with Google Drive",
                "Nothing is kept on our servers.",
            ),
            (
                Language::Fr,
                "Synchroniser avec Google Drive",
                "Rien n’est gardé sur nos serveurs.",
            ),
            (
                Language::Vi,
                "Đồng bộ với Google Drive",
                "Máy chủ của chúng tôi không lưu gì.",
            ),
        ] {
            let html = panel(SyncAccount::SignedOut, SyncStatus::Idle, language);
            assert!(html.contains(&format!(">{sign_in}<")), "{html}");
            assert!(html.contains(explain), "{html}");
            assert_eq!(html.matches("<button").count(), 1, "{html}");
        }
    }

    #[test]
    fn signed_in_shows_the_account_the_last_sync_and_two_actions() {
        for (language, account, last, now, out) in [
            (
                Language::En,
                "Google Drive: learner@example.org",
                "Last synced at 10:42",
                "Sync now",
                "Sign out",
            ),
            (
                Language::Fr,
                "Google Drive : learner@example.org",
                "Dernière synchronisation à 10:42",
                "Synchroniser",
                "Se déconnecter",
            ),
            (
                Language::Vi,
                "Tài khoản Google Drive: learner@example.org",
                "Đồng bộ lần cuối lúc 10:42",
                "Đồng bộ ngay",
                "Đăng xuất",
            ),
        ] {
            let status = SyncStatus::Synced { at: "10:42".into() };
            let html = panel(email(), status, language);
            for text in [account, last] {
                assert!(html.contains(text), "{text}: {html}");
            }
            for label in [now, out] {
                assert!(
                    !button_labelled(&html, label).contains("disabled"),
                    "{label}: {html}"
                );
            }
        }
    }

    #[test]
    fn an_account_with_no_email_is_named_plainly() {
        let html = panel(
            SyncAccount::SignedIn { email: None },
            SyncStatus::Idle,
            Language::En,
        );
        assert!(html.contains("Signed in to Google Drive"), "{html}");
        assert!(!html.contains("Last synced"), "{html}");
    }

    #[test]
    fn a_remembered_account_offers_sync_now_and_sign_out() {
        let account = SyncAccount::Remembered {
            email: Some("learner@example.org".into()),
        };
        let html = panel(account, SyncStatus::Idle, Language::En);
        assert!(html.contains("Google Drive: learner@example.org"), "{html}");
        assert!(
            html.contains(">Sync now<") && html.contains(">Sign out<"),
            "{html}"
        );
    }

    /// After a reload no token is at hand, so Sign out cannot revoke the
    /// access at Google: the line says so and links to the Google account.
    #[test]
    fn a_remembered_account_says_where_the_access_is_removed() {
        for (language, note, link) in [
            (
                Language::En,
                "After a reload, Sign out cannot revoke Google Drive access from here.",
                "Remove it in your Google account",
            ),
            (
                Language::Fr,
                "Après un rechargement, Se déconnecter ne peut pas révoquer l’accès à Google Drive depuis ici.",
                "Retirez-le dans votre compte Google",
            ),
            (
                Language::Vi,
                "Sau khi tải lại trang, Đăng xuất không thể thu hồi quyền truy cập Google Drive từ đây.",
                "Hãy gỡ quyền này trong tài khoản Google của bạn",
            ),
        ] {
            let account = SyncAccount::Remembered { email: None };
            let html = panel(account, SyncStatus::Idle, language);
            assert!(html.contains(note), "{html}");
            let anchor = crate::test_support::tag_around(&html, &format!(">{link}<"));
            assert!(anchor.starts_with("<a "), "{anchor}");
            assert!(
                anchor.contains(&format!(r#"href="{GOOGLE_CONNECTIONS}""#)),
                "{anchor}"
            );
            assert!(anchor.contains(r#"target="_blank""#), "{anchor}");
            assert!(anchor.contains(r#"rel="noopener noreferrer""#), "{anchor}");
        }
        assert_eq!(
            GOOGLE_CONNECTIONS,
            "https://myaccount.google.com/connections"
        );
        for account in [SyncAccount::SignedOut, email()] {
            let html = panel(account.clone(), SyncStatus::Idle, Language::En);
            assert!(
                !html.contains("myaccount.google.com"),
                "{account:?}: {html}"
            );
        }
    }

    #[test]
    fn while_syncing_sync_now_is_off() {
        let html = panel(email(), SyncStatus::Syncing, Language::En);
        assert!(html.contains("Syncing…"), "{html}");
        assert!(
            button_labelled(&html, "Sync now").contains("disabled"),
            "{html}"
        );
    }

    #[test]
    fn a_failure_says_not_synced_with_its_reason_and_retry() {
        for (error, reason) in [
            (SyncError::SignedOut, "Google asked to sign in again."),
            (SyncError::Cancelled, "The Google sign-in was closed."),
            (
                SyncError::Popup,
                "The Google window could not open. Allow pop-ups, then try again.",
            ),
            (SyncError::Network, "Google Drive could not be reached."),
            (
                SyncError::Drive("403 rate".into()),
                "Google Drive refused: 403 rate",
            ),
        ] {
            let html = panel(email(), SyncStatus::Failed(error.clone()), Language::En);
            assert!(html.contains("Not synced."), "{error:?}: {html}");
            assert!(html.contains(reason), "{error:?}: {html}");
            assert!(html.contains(">Retry<"), "{error:?}: {html}");
            assert!(!html.contains(">Sync now<"), "{error:?}: {html}");
        }
    }

    #[test]
    fn a_refusal_names_why_this_device_is_not_synced() {
        for (refusal, reason) in [
            (
                Refusal::Unreadable,
                "This device’s progress could not be read.",
            ),
            (
                Refusal::Newer,
                "This device’s progress was saved by a newer version.",
            ),
            (Refusal::Invalid, "This device’s progress is damaged."),
            (
                Refusal::NotSaved,
                "This device could not save the progress.",
            ),
        ] {
            let html = panel(email(), SyncStatus::Refused(refusal), Language::En);
            assert!(html.contains("Not synced."), "{refusal:?}: {html}");
            assert!(html.contains(reason), "{refusal:?}: {html}");
        }
    }

    #[test]
    fn a_newer_drive_copy_asks_to_update_the_app() {
        let html = panel(email(), SyncStatus::DriveNewer, Language::En);
        assert!(
            html.contains(
                "The copy in Google Drive was saved by a newer version. Update the app to sync."
            ),
            "{html}"
        );
        assert!(!html.contains("Download the Drive copy"), "{html}");
    }

    #[test]
    fn an_unreadable_drive_copy_is_offered_for_download() {
        let status = SyncStatus::DriveUnreadable {
            raw: "{oops".into(),
        };
        let html = panel(email(), status, Language::En);
        assert!(
            html.contains("The copy in Google Drive could not be read. Nothing was changed."),
            "{html}"
        );
        assert!(html.contains(">Download the Drive copy<"), "{html}");
    }

    #[test]
    fn the_drive_copy_is_downloaded_as_is() {
        Owner::new().with(|| {
            let (_, files) = provide_fakes(Load::Empty);
            download_drive_copy(Backend::current(), "{oops");
            assert_eq!(
                files.exported(),
                vec![(DRIVE_COPY_NAME.to_owned(), "{oops".to_owned())]
            );
        });
    }

    #[test]
    fn the_chip_shows_only_for_a_remembered_account() {
        let html = chip(SyncAccount::Remembered { email: None });
        assert!(html.contains(">Sync<"), "{html}");
        assert!(html.contains("<button"), "{html}");
        for account in [SyncAccount::SignedOut, email()] {
            assert_eq!(chip(account.clone()), "", "{account:?}");
        }
    }

    /// No sync string holds an em dash, in any language (memory: no em
    /// dash in copy).
    #[test]
    fn no_sync_string_holds_an_em_dash() {
        for (code, text) in [
            ("en", include_str!("../../locales/en.json")),
            ("fr", include_str!("../../locales/fr.json")),
            ("vi", include_str!("../../locales/vi.json")),
        ] {
            let values: serde_json::Map<String, serde_json::Value> =
                serde_json::from_str(text).expect("a catalogue");
            let sync: Vec<_> = values
                .iter()
                .filter(|(key, _)| key.starts_with("sync_"))
                .collect();
            assert_eq!(sync.len(), 25, "{code}");
            for (key, value) in sync {
                assert!(!value.as_str().unwrap().contains('—'), "{code} {key}");
            }
        }
    }
}
