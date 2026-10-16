//! The site's sync transport (task 050): the learner's Google Drive, from
//! the browser, with no server of ours in between ([`web_sync`]).
//!
//! - **Sign-in.** Google Identity Services' token client (the script
//!   [`GIS_SCRIPT`]), injected on the first Sign in or Sync tap only: a
//!   learner who never syncs loads no Google script. The web client
//!   [`CLIENT_ID`] (a build-time constant, `LEARN_PALI_GOOGLE_CLIENT_ID`
//!   overrides it) asks [`SCOPE`]: `drive.appdata` (the hidden app folder)
//!   and `email` (to show the account). `requestAccessToken` runs in the
//!   tap's gesture, with `prompt: ''` and the remembered email as
//!   `login_hint`. On the very first tap the script loads first; a browser
//!   that blocks the window then answers [`SyncError::Popup`] and the
//!   second tap, the script loaded, opens it at once.
//! - **The token** lives in this tab's memory only, with its expiry
//!   (`expires_in`), never in storage and never sent to our server.
//!   `localStorage[pc_learn_sync]` keeps only that sync is on and the
//!   email (the hint): after a reload the account is
//!   [`SyncAccount::Remembered`] and the home shows its Sync chip. It is
//!   read after hydration, so the first paint matches the server's.
//! - **The file** is `pc_learn.json` in the Drive's `appDataFolder`
//!   ([`FILE_NAME`]): listed (the oldest one if two devices raced to create
//!   it), read (`alt=media`), created (one multipart upload: never an empty
//!   file) or updated (a media upload), all with `fetch` and a Bearer
//!   header. The file id it resolved is logged to the console, for the
//!   user's check that the site and the app see the same file.
//! - **A refused token** (401) is dropped: the account turns remembered and
//!   the call answers [`SyncError::SignedOut`].
//! - **Sign out** revokes the token at Google
//!   (`google.accounts.oauth2.revoke`) when one is at hand, then forgets
//!   it, the file id and the remembered email. With no token at hand (after
//!   a reload) there is nothing to revoke from here: the grant stays in the
//!   learner's Google account until they remove it there.
//!
//! The site's Content-Security-Policy, if one is set, must allow
//! `script-src https://accounts.google.com/gsi/client`, `connect-src
//! https://www.googleapis.com https://oauth2.googleapis.com` and
//! `frame-src https://accounts.google.com`.
//!
//! The decisions on Google's answers are pure functions, tested natively;
//! the browser glue is compiled with `web-sync` (which `hydrate` enables)
//! and tested in a headless Chrome against a fake Google
//! (`tests/web_sync.rs`). On the server, [`web_sync`] is a transport that
//! is always signed out (never called there).

// The decisions below serve the browser's glue (`web-sync`) and the tests;
// the server builds them and calls only `web_sync`, its signed-out stub.
#![cfg_attr(
    not(any(feature = "web-sync", test)),
    allow(
        dead_code,
        reason = "used by the browser's sync (web-sync) and the tests"
    )
)]

use crate::backend::{SharedSync, SyncAccount, SyncError};

/// The web OAuth client of project `palicanon-app`; the build's
/// `LEARN_PALI_GOOGLE_CLIENT_ID` replaces it.
pub const CLIENT_ID: &str = match option_env!("LEARN_PALI_GOOGLE_CLIENT_ID") {
    Some(id) => id,
    None => "873594463453-ag83cvs5t5uqa6uvir9n4p9igbmdans7.apps.googleusercontent.com",
};

/// What the token is asked for: the Drive's hidden app folder, and the
/// email to show the account.
pub const SCOPE: &str = "https://www.googleapis.com/auth/drive.appdata email";

/// Google Identity Services' script.
pub const GIS_SCRIPT: &str = "https://accounts.google.com/gsi/client";

/// The progress file's name in the app folder.
pub const FILE_NAME: &str = "pc_learn.json";

/// The `localStorage` key that keeps the sync on, its value the email.
pub(crate) const REMEMBER_KEY: &str = "pc_learn_sync";

/// The multipart boundary of a creation; never in a progress's JSON.
const BOUNDARY: &str = "pc_learn_sync_050_boundary";

/// The account `localStorage[pc_learn_sync]` remembers: none, or the
/// email (empty: unknown).
pub(crate) fn remembered(value: Option<String>) -> SyncAccount {
    match value {
        None => SyncAccount::SignedOut,
        Some(email) => SyncAccount::Remembered {
            email: Some(email).filter(|email| !email.is_empty()),
        },
    }
}

/// The email of an account, if it has one.
pub(crate) fn email_of(account: &SyncAccount) -> Option<String> {
    match account {
        SyncAccount::SignedOut => None,
        SyncAccount::Remembered { email } | SyncAccount::SignedIn { email } => email.clone(),
    }
}

/// The URL listing the progress file in the app folder, the oldest first.
pub(crate) fn list_url() -> String {
    format!(
        "https://www.googleapis.com/drive/v3/files?spaces=appDataFolder\
         &q=name%3D%27{FILE_NAME}%27%20and%20trashed%3Dfalse\
         &orderBy=createdTime&fields=files(id)"
    )
}

/// The URL reading the file `id`'s content.
pub(crate) fn read_url(id: &str) -> String {
    format!("https://www.googleapis.com/drive/v3/files/{id}?alt=media")
}

/// The URL replacing the file `id`'s content.
pub(crate) fn update_url(id: &str) -> String {
    format!("https://www.googleapis.com/upload/drive/v3/files/{id}?uploadType=media")
}

/// The URL creating the file, metadata and content in one request.
pub(crate) const CREATE_URL: &str =
    "https://www.googleapis.com/upload/drive/v3/files?uploadType=multipart&fields=id";

/// The account's email, from Google's userinfo.
pub(crate) const USERINFO_URL: &str = "https://www.googleapis.com/oauth2/v3/userinfo";

/// The creation's `Content-Type` and body: the file's metadata (its name,
/// in the app folder) and `text`, in one multipart upload.
pub(crate) fn create_body(text: &str) -> (String, String) {
    let metadata = serde_json::json!({ "name": FILE_NAME, "parents": ["appDataFolder"] });
    let body = format!(
        "--{BOUNDARY}\r\nContent-Type: application/json; charset=UTF-8\r\n\r\n{metadata}\r\n\
         --{BOUNDARY}\r\nContent-Type: application/json\r\n\r\n{text}\r\n--{BOUNDARY}--"
    );
    (format!("multipart/related; boundary={BOUNDARY}"), body)
}

/// The answer of a call: `Ok` for a 2xx, [`SyncError::SignedOut`] for a
/// 401 (the token is refused), else Google's message.
///
/// # Errors
/// As above.
pub(crate) fn checked(status: u16, body: &str) -> Result<(), SyncError> {
    match status {
        200..=299 => Ok(()),
        401 => Err(SyncError::SignedOut),
        _ => {
            let message = serde_json::from_str::<serde_json::Value>(body)
                .ok()
                .and_then(|value| value["error"]["message"].as_str().map(str::to_owned))
                .unwrap_or_default();
            Err(SyncError::Drive(
                format!("{status} {message}").trim().to_owned(),
            ))
        }
    }
}

/// The first file id of a list answer; none when the folder has no file.
///
/// # Errors
/// [`SyncError::Drive`] when the answer is not a list.
pub(crate) fn first_file_id(body: &str) -> Result<Option<String>, SyncError> {
    let value: serde_json::Value = serde_json::from_str(body)
        .map_err(|_| SyncError::Drive("unexpected list answer".to_owned()))?;
    let files = value["files"]
        .as_array()
        .ok_or_else(|| SyncError::Drive("unexpected list answer".to_owned()))?;
    Ok(files
        .first()
        .and_then(|file| file["id"].as_str())
        .map(str::to_owned))
}

/// The id of a created file.
///
/// # Errors
/// [`SyncError::Drive`] when the answer holds none.
pub(crate) fn created_id(body: &str) -> Result<String, SyncError> {
    serde_json::from_str::<serde_json::Value>(body)
        .ok()
        .and_then(|value| value["id"].as_str().map(str::to_owned))
        .ok_or_else(|| SyncError::Drive("unexpected creation answer".to_owned()))
}

/// The email of a userinfo answer.
pub(crate) fn userinfo_email(body: &str) -> Option<String> {
    serde_json::from_str::<serde_json::Value>(body)
        .ok()
        .and_then(|value| value["email"].as_str().map(str::to_owned))
}

/// An access token and the time it expires (milliseconds since the epoch).
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct Token {
    /// The token, sent as `Authorization: Bearer`.
    pub(crate) access: String,
    /// When it stops being used: a minute before Google's expiry.
    pub(crate) expires_at: f64,
}

impl Token {
    /// Whether it is still used at `now`.
    pub(crate) fn valid(&self, now: f64) -> bool {
        now < self.expires_at
    }
}

/// The token of a token client's answer (its `access_token`,
/// `expires_in` in seconds, `error`), received at `now`.
///
/// # Errors
/// [`SyncError::Cancelled`] when Google answered an error or no token.
pub(crate) fn token_of(
    access: Option<String>,
    expires_in: Option<f64>,
    error: Option<String>,
    now: f64,
) -> Result<Token, SyncError> {
    match (access, error) {
        (Some(access), None) if !access.is_empty() => Ok(Token {
            access,
            expires_at: now + (expires_in.unwrap_or(0.0) - 60.0).max(0.0) * 1000.0,
        }),
        _ => Err(SyncError::Cancelled),
    }
}

/// The failure of a token client's `error_callback`, by its `type`.
pub(crate) fn sign_in_error(kind: Option<&str>) -> SyncError {
    match kind {
        Some("popup_failed_to_open") => SyncError::Popup,
        _ => SyncError::Cancelled,
    }
}

/// The site's sync transport: Google Drive from the browser (see the
/// module documentation); always signed out on the server.
#[must_use]
pub fn web_sync() -> SharedSync {
    browser::transport()
}

/// The server's transport: signed out, never called.
#[cfg(not(feature = "web-sync"))]
mod browser {
    use std::rc::Rc;

    use leptos::prelude::*;

    use crate::backend::{SharedSync, SyncAccount, SyncError, SyncTransport};

    /// Signed out; every call does nothing.
    struct Server;

    impl SyncTransport for Server {
        fn sign_in(&self, _on_done: Box<dyn FnOnce(Result<(), SyncError>)>) {}

        fn sign_out(&self) {}

        fn account(&self) -> Signal<SyncAccount> {
            Signal::stored(SyncAccount::SignedOut)
        }

        fn pull(&self, _on_done: Box<dyn FnOnce(Result<Option<String>, SyncError>)>) {}

        fn push(&self, _text: &str, _on_done: Box<dyn FnOnce(Result<(), SyncError>)>) {}
    }

    /// The server's transport.
    pub(super) fn transport() -> SharedSync {
        Rc::new(Server)
    }
}

#[cfg(feature = "web-sync")]
mod browser;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_client_and_the_scope_are_the_designed_ones() {
        assert_eq!(
            CLIENT_ID,
            "873594463453-ag83cvs5t5uqa6uvir9n4p9igbmdans7.apps.googleusercontent.com"
        );
        assert_eq!(SCOPE, "https://www.googleapis.com/auth/drive.appdata email");
        assert_eq!(GIS_SCRIPT, "https://accounts.google.com/gsi/client");
    }

    #[test]
    fn the_remembered_value_is_the_account() {
        assert_eq!(REMEMBER_KEY, "pc_learn_sync", "beside the progress's key");
        assert_eq!(remembered(None), SyncAccount::SignedOut);
        assert_eq!(
            remembered(Some(String::new())),
            SyncAccount::Remembered { email: None }
        );
        assert_eq!(
            remembered(Some("a@b.c".into())),
            SyncAccount::Remembered {
                email: Some("a@b.c".into())
            }
        );
        assert_eq!(email_of(&SyncAccount::SignedOut), None);
        assert_eq!(
            email_of(&SyncAccount::SignedIn {
                email: Some("a@b.c".into())
            }),
            Some("a@b.c".into())
        );
    }

    #[test]
    fn the_urls_name_the_app_folder_and_the_file() {
        let list = list_url();
        assert!(list.starts_with("https://www.googleapis.com/drive/v3/files?"));
        assert!(list.contains("spaces=appDataFolder"), "{list}");
        assert!(list.contains("name%3D%27pc_learn.json%27"), "{list}");
        assert!(list.contains("orderBy=createdTime"), "{list}");
        assert_eq!(
            read_url("f1"),
            "https://www.googleapis.com/drive/v3/files/f1?alt=media"
        );
        assert_eq!(
            update_url("f1"),
            "https://www.googleapis.com/upload/drive/v3/files/f1?uploadType=media"
        );
        for url in [list.as_str(), CREATE_URL, USERINFO_URL] {
            assert!(url.starts_with("https://www.googleapis.com/"), "{url}");
        }
    }

    #[test]
    fn a_creation_carries_the_name_the_folder_and_the_text() {
        let (content_type, body) = create_body(r#"{"version":1}"#);
        assert_eq!(
            content_type,
            format!("multipart/related; boundary={BOUNDARY}")
        );
        assert!(body.contains(r#""name":"pc_learn.json""#), "{body}");
        assert!(body.contains(r#""parents":["appDataFolder"]"#), "{body}");
        assert!(body.contains("\r\n\r\n{\"version\":1}\r\n"), "{body}");
        assert!(body.ends_with(&format!("--{BOUNDARY}--")), "{body}");
        assert_eq!(body.matches(&format!("--{BOUNDARY}\r\n")).count(), 2);
    }

    #[test]
    fn statuses_become_errors_with_googles_message() {
        assert_eq!(checked(200, ""), Ok(()));
        assert_eq!(checked(204, ""), Ok(()));
        assert_eq!(checked(401, "{}"), Err(SyncError::SignedOut));
        assert_eq!(
            checked(403, r#"{"error":{"code":403,"message":"Rate limit"}}"#),
            Err(SyncError::Drive("403 Rate limit".into()))
        );
        assert_eq!(checked(500, "<html>"), Err(SyncError::Drive("500".into())));
    }

    #[test]
    fn a_list_gives_the_first_file_or_none() {
        assert_eq!(first_file_id(r#"{"files":[]}"#), Ok(None));
        assert_eq!(
            first_file_id(r#"{"files":[{"id":"old"},{"id":"new"}]}"#),
            Ok(Some("old".into()))
        );
        assert!(first_file_id("{}").is_err());
        assert!(first_file_id("nope").is_err());
        assert_eq!(created_id(r#"{"id":"f9"}"#), Ok("f9".into()));
        assert!(created_id("{}").is_err());
        assert_eq!(
            userinfo_email(r#"{"sub":"1","email":"a@b.c"}"#),
            Some("a@b.c".into())
        );
        assert_eq!(userinfo_email("{}"), None);
    }

    #[test]
    fn a_token_expires_a_minute_early_and_an_error_is_no_token() {
        let token = token_of(Some("t".into()), Some(3600.0), None, 1000.0).unwrap();
        assert_eq!(token.expires_at, 1000.0 + 3540.0 * 1000.0);
        assert!(token.valid(1000.0));
        assert!(!token.valid(token.expires_at));
        for (access, error) in [
            (None, None),
            (Some(String::new()), None),
            (Some("t".to_owned()), Some("access_denied".to_owned())),
            (None, Some("access_denied".to_owned())),
        ] {
            assert_eq!(
                token_of(access, Some(3600.0), error, 0.0),
                Err(SyncError::Cancelled)
            );
        }
        assert_eq!(
            sign_in_error(Some("popup_failed_to_open")),
            SyncError::Popup
        );
        assert_eq!(sign_in_error(Some("popup_closed")), SyncError::Cancelled);
        assert_eq!(sign_in_error(None), SyncError::Cancelled);
    }

    #[test]
    fn the_server_transport_is_signed_out_and_inert() {
        use leptos::prelude::{GetUntracked, Owner};
        Owner::new().with(|| {
            let sync = web_sync();
            assert_eq!(sync.account().get_untracked(), SyncAccount::SignedOut);
            sync.sign_in(Box::new(|_| panic!("no sign-in on the server")));
            sync.pull(Box::new(|_| panic!("no pull on the server")));
            sync.push("{}", Box::new(|_| panic!("no push on the server")));
            sync.sign_out();
        });
    }
}
