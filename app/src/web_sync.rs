//! The site's sync transport (task 050): the learner's Google Drive, from
//! the browser, with no server of ours in between ([`web_sync`]).
//!
//! - **Sign-in.** Google Identity Services' token client (the script
//!   [`GIS_SCRIPT`]), loaded when the sync UI shows where a tap may follow
//!   (`prepare`: Settings opened, or the home's Sync chip shown): a page
//!   that never opens Settings asks nothing of Google. The web client
//!   [`CLIENT_ID`] (a build-time constant, `LEARN_PALI_GOOGLE_CLIENT_ID`
//!   overrides it) asks [`scope`]: `drive.appdata` (the hidden app folder)
//!   and `email` (to show the account). The Sign in or Sync tap calls
//!   `requestAccessToken` inside its gesture, with `prompt: ''` and the
//!   remembered email as `login_hint`, so the browser lets Google's window
//!   open; a tap before the script is there answers [`SyncError::Network`]
//!   at once (Retry) and asks for it. A window the browser still blocks
//!   answers [`SyncError::Popup`].
//! - **The token** lives in this tab's memory only, with its expiry
//!   (`expires_in`), never in storage and never sent to our server.
//!   `localStorage[pc_learn_sync]` keeps only that sync is on and the
//!   email (the hint): after a reload the account is
//!   [`SyncAccount::Remembered`] and the home shows its Sync chip. It is
//!   read after hydration, so the first paint matches the server's.
//! - **The file** is where [`drive_location`] says (today `pc_learn.json`
//!   in the Drive's `appDataFolder`): listed (the oldest one if two devices raced to create
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
//! (`tests/web_sync.rs`). On the server (no `web-sync`), [`web_sync`] is
//! none: the first paint shows no sync UI.

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
use crate::drive_location;

/// The web OAuth client of project `palicanon-app`; the build's
/// `LEARN_PALI_GOOGLE_CLIENT_ID` replaces it.
pub const CLIENT_ID: &str = match option_env!("LEARN_PALI_GOOGLE_CLIENT_ID") {
    Some(id) => id,
    None => "873594463453-ag83cvs5t5uqa6uvir9n4p9igbmdans7.apps.googleusercontent.com",
};

/// What the token is asked for: where the progress lives
/// ([`drive_location::SCOPE`]) and `email`, to show the account.
pub fn scope() -> String {
    format!("{} email", drive_location::SCOPE)
}

/// Google Identity Services' script.
pub const GIS_SCRIPT: &str = "https://accounts.google.com/gsi/client";

/// The `localStorage` key that keeps the sync on, its value the email.
pub(crate) const REMEMBER_KEY: &str = "pc_learn_sync";

/// The account `localStorage[pc_learn_sync]` remembers: its email; no
/// value, or an empty one, is no account (signed out).
pub(crate) fn remembered(value: Option<String>) -> SyncAccount {
    match value {
        Some(email) if !email.is_empty() => SyncAccount::Remembered { email: Some(email) },
        _ => SyncAccount::SignedOut,
    }
}

/// The email of an account, if it has one.
pub(crate) fn email_of(account: &SyncAccount) -> Option<String> {
    match account {
        SyncAccount::SignedOut => None,
        SyncAccount::Remembered { email } | SyncAccount::SignedIn { email } => email.clone(),
    }
}

/// The account's email, from Google's userinfo.
pub(crate) const USERINFO_URL: &str = "https://www.googleapis.com/oauth2/v3/userinfo";

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

/// The first file id of a list answer (an entry without one is skipped);
/// none when the folder has no file.
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
        .iter()
        .find_map(|file| file["id"].as_str())
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

/// A token's life when Google's answer does not say (`expires_in`), in
/// seconds: Google's usual hour, less a second.
const DEFAULT_EXPIRES_IN: f64 = 3599.0;

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
            expires_at: now + (expires_in.unwrap_or(DEFAULT_EXPIRES_IN) - 60.0).max(0.0) * 1000.0,
        }),
        _ => Err(SyncError::Cancelled),
    }
}

/// The account to turn into when the token has expired: `Remembered`
/// (the email kept) when `account` is `SignedIn` and `token` is missing or
/// no longer valid at `now`; `None` when nothing changes (signed out,
/// already remembered, or a valid token). The paused state is derived from
/// the account, never from a failed call, so no request is wasted.
pub(crate) fn expired(
    account: &SyncAccount,
    token: Option<&Token>,
    now: f64,
) -> Option<SyncAccount> {
    let SyncAccount::SignedIn { email } = account else {
        return None;
    };
    if token.is_some_and(|token| token.valid(now)) {
        return None;
    }
    Some(SyncAccount::Remembered {
        email: email.clone(),
    })
}

/// The failure of a token client's `error_callback`, by its `type`.
pub(crate) fn sign_in_error(kind: Option<&str>) -> SyncError {
    match kind {
        Some("popup_failed_to_open") => SyncError::Popup,
        _ => SyncError::Cancelled,
    }
}

/// The site's sync transport: Google Drive from the browser (see the
/// module documentation). None in a build without `web-sync` (the
/// server): no transport, so no sync UI, never a Sign in that does
/// nothing.
#[must_use]
pub fn web_sync() -> Option<SharedSync> {
    #[cfg(feature = "web-sync")]
    {
        Some(browser::transport())
    }
    #[cfg(not(feature = "web-sync"))]
    {
        None
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
        assert_eq!(
            scope(),
            "https://www.googleapis.com/auth/drive.appdata email"
        );
        assert_eq!(GIS_SCRIPT, "https://accounts.google.com/gsi/client");
        assert!(USERINFO_URL.starts_with("https://www.googleapis.com/"));
    }

    #[test]
    fn the_remembered_value_is_the_account() {
        assert_eq!(REMEMBER_KEY, "pc_learn_sync", "beside the progress's key");
        assert_eq!(remembered(None), SyncAccount::SignedOut);
        assert_eq!(
            remembered(Some(String::new())),
            SyncAccount::SignedOut,
            "an empty value is no account"
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
            first_file_id(r#"{"files":[{"name":"x"},{"id":"f2"}]}"#),
            Ok(Some("f2".into())),
            "an entry without an id is skipped"
        );
        assert_eq!(first_file_id(r#"{"files":[{"name":"x"}]}"#), Ok(None));
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
        let default = token_of(Some("t".into()), None, None, 0.0).unwrap();
        assert_eq!(default.expires_at, 3539.0 * 1000.0, "no expires_in: 3599 s");
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

    /// Design (059): "the web transport turns `SignedIn` into Remembered when
    /// token.valid(now) fails". Mutations it does not catch: a `refresh`
    /// that never calls `expired` (the browser step does), a wrong clock.
    #[test]
    fn a_signed_in_account_with_no_valid_token_turns_remembered() {
        let token = Token {
            access: "t".into(),
            expires_at: 5000.0,
        };
        let signed_in = |email: Option<&str>| SyncAccount::SignedIn {
            email: email.map(str::to_owned),
        };
        let remembered = |email: Option<&str>| {
            Some(SyncAccount::Remembered {
                email: email.map(str::to_owned),
            })
        };
        let a = Some("a@b.c");
        assert_eq!(expired(&signed_in(a), Some(&token), 4999.0), None, "valid");
        assert_eq!(
            expired(&signed_in(a), Some(&token), 5000.0),
            remembered(a),
            "valid is now < expires_at: at the instant it is over"
        );
        assert_eq!(expired(&signed_in(a), Some(&token), 9000.0), remembered(a));
        assert_eq!(expired(&signed_in(a), None, 0.0), remembered(a), "no token");
        assert_eq!(expired(&signed_in(None), None, 0.0), remembered(None));
        for account in [
            SyncAccount::Remembered {
                email: a.map(str::to_owned),
            },
            SyncAccount::Remembered { email: None },
        ] {
            assert_eq!(expired(&account, None, 0.0), None, "already remembered");
            assert_eq!(expired(&account, Some(&token), 9000.0), None);
        }
        assert_eq!(expired(&SyncAccount::SignedOut, None, 0.0), None);
        assert_eq!(expired(&SyncAccount::SignedOut, Some(&token), 9000.0), None);
    }

    #[test]
    #[cfg(not(feature = "web-sync"))]
    fn a_build_without_web_sync_has_no_transport() {
        assert!(web_sync().is_none());
    }
}
