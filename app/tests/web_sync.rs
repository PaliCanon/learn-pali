//! The site's sync transport in a real browser (task 050), against a fake
//! Google: a fake `google.accounts.oauth2` (the token client, revoke) and a
//! fake `window.fetch` that plays the Drive and userinfo. No network.
//!
//! One test, in order, since the transport's script and token client are
//! the page's: the script is injected on the first sign-in only; the token
//! stays in memory (never in `localStorage`, which keeps the email alone);
//! the file is created once (multipart) then updated (media), every call
//! with the Bearer token; a refused token turns the account remembered;
//! the second sign-in passes the email as `login_hint`; a blocked window is
//! `Popup`; sign out revokes the token and forgets the account; a reload
//! finds the account remembered.
//!
//! Run with `wasm-bindgen-test-runner` and a headless Chrome:
//! `CARGO_TARGET_WASM32_UNKNOWN_UNKNOWN_RUNNER=wasm-bindgen-test-runner
//! CHROMEDRIVER=<chromedriver> cargo test --locked -p learn_pali --features
//! csr,web-sync --target wasm32-unknown-unknown --test web_sync`.

#![cfg(all(target_arch = "wasm32", feature = "web-sync"))]

use std::cell::RefCell;
use std::rc::Rc;

use futures::channel::oneshot;
use js_sys::{Function, Promise};
use learn_pali::fragment::{SharedSync, SyncAccount, SyncError};
use learn_pali::web_sync::{CLIENT_ID, GIS_SCRIPT, scope};
use leptos::prelude::GetUntracked;
use wasm_bindgen::{JsCast, JsValue};
use wasm_bindgen_futures::JsFuture;
use wasm_bindgen_test::{wasm_bindgen_test, wasm_bindgen_test_configure};

wasm_bindgen_test_configure!(run_in_browser);

/// The fake Drive and userinfo behind `window.fetch`; `window.__drive`
/// holds the files, every call, and the switches the test turns.
const FAKE_FETCH: &str = r#"
window.__drive = { files: {}, next: 1, calls: [], fail401: false, revoked: [], popupFails: false, stored: [] };
const setItem = Storage.prototype.setItem;
Storage.prototype.setItem = function (key, value) {
  window.__drive.stored.push(String(value));
  return setItem.call(this, key, value);
};
window.fetch = async (request) => {
  const d = window.__drive;
  const url = request.url, method = request.method;
  const body = method === 'GET' ? '' : await request.text();
  d.calls.push({ method, url, auth: request.headers.get('Authorization'), body,
                 type: request.headers.get('Content-Type') });
  const json = (status, value) => new Response(JSON.stringify(value), { status });
  if (d.delayMs) await new Promise((resolve) => setTimeout(resolve, d.delayMs));
  if (d.fail401) return json(401, { error: { message: 'expired' } });
  if (url === 'https://www.googleapis.com/oauth2/v3/userinfo')
    return json(200, { sub: '1', email: 'learner@example.org' });
  if (method === 'GET' && url.startsWith('https://www.googleapis.com/drive/v3/files?'))
    return json(200, { files: Object.keys(d.files).map((id) => ({ id })) });
  const read = url.match(/^https:\/\/www\.googleapis\.com\/drive\/v3\/files\/([^?]+)\?alt=media$/);
  if (method === 'GET' && read) return new Response(d.files[read[1]], { status: 200 });
  if (method === 'POST' && url.startsWith('https://www.googleapis.com/upload/drive/v3/files?uploadType=multipart')) {
    const id = 'f' + d.next++;
    d.files[id] = body.split('\r\n\r\n')[2].split('\r\n--')[0];
    return json(200, { id });
  }
  const update = url.match(/^https:\/\/www\.googleapis\.com\/upload\/drive\/v3\/files\/([^?]+)\?uploadType=media$/);
  if (method === 'PATCH' && update) { d.files[update[1]] = body; return json(200, { id: update[1] }); }
  return json(404, { error: { message: 'unknown ' + method + ' ' + url } });
};
"#;

/// The fake Google Identity Services: the token client answers a token
/// (or a blocked window), revoke records the token.
const FAKE_GIS: &str = r#"
window.google = { accounts: { oauth2: {
  initTokenClient(config) {
    window.__drive.config = config;
    return { requestAccessToken(overrides) {
      window.__drive.overrides = overrides;
      if (window.__drive.popupFails) {
        setTimeout(() => config.error_callback({ type: 'popup_failed_to_open' }), 0);
      } else {
        setTimeout(() => config.callback({ access_token: 'fake-token', expires_in: 3599 }), 0);
      }
    } };
  },
  revoke(token, done) { window.__drive.revoked.push(token); if (done) done(); },
} } };
"#;

/// Runs `code` in the page and returns its value.
fn js(code: &str) -> JsValue {
    Function::new_no_args(code)
        .call0(&JsValue::NULL)
        .expect("the script runs")
}

/// `js(code)` as a string.
fn js_string(code: &str) -> Option<String> {
    js(code).as_string()
}

/// Lets timers, fetches and animation frames run.
async fn ticks() {
    for _ in 0..5 {
        let wait = Promise::new(&mut |resolve, _| {
            web_sys::window()
                .expect("a window")
                .set_timeout_with_callback_and_timeout_and_arguments_0(&resolve, 20)
                .expect("a timer");
        });
        JsFuture::from(wait).await.expect("the timer");
    }
}

async fn sign_in(sync: &SharedSync) -> Result<(), SyncError> {
    let (tx, rx) = oneshot::channel();
    sync.sign_in(Box::new(move |result| {
        let _ = tx.send(result);
    }));
    rx.await.expect("the sign-in answers")
}

async fn pull(sync: &SharedSync) -> Result<Option<String>, SyncError> {
    let (tx, rx) = oneshot::channel();
    sync.pull(Box::new(move |result| {
        let _ = tx.send(result);
    }));
    rx.await.expect("the pull answers")
}

async fn push(sync: &SharedSync, text: &str) -> Result<(), SyncError> {
    let (tx, rx) = oneshot::channel();
    sync.push(
        text,
        Box::new(move |result| {
            let _ = tx.send(result);
        }),
    );
    rx.await.expect("the push answers")
}

fn email() -> Option<String> {
    Some("learner@example.org".to_owned())
}

/// The GIS scripts in the page.
fn scripts() -> Option<f64> {
    js(&format!(
        "return document.head.querySelectorAll('script[src=\"{GIS_SCRIPT}\"]').length"
    ))
    .as_f64()
}

/// Taps Sign in: the answer if it came inside the call.
fn sign_in_now(sync: &SharedSync) -> Option<Result<(), SyncError>> {
    let answer = Rc::new(RefCell::new(None));
    let seen = Rc::clone(&answer);
    sync.sign_in(Box::new(move |result| *seen.borrow_mut() = Some(result)));
    answer.take()
}

/// Nothing is asked of Google before a tap. A tap before the script is
/// there answers Network at once (Retry) and asks for the script; a script
/// that fails is asked again when the UI readies it; once it is there, the
/// tap opens Google's window inside the call (its gesture). The token
/// never reaches `localStorage`.
async fn first_sign_in(sync: &SharedSync) {
    assert_eq!(scripts(), Some(0.0), "no Google script before a tap");
    assert_eq!(js("return window.__drive.calls.length").as_f64(), Some(0.0));
    assert_eq!(sign_in_now(sync), Some(Err(SyncError::Network)), "at once");
    assert_eq!(scripts(), Some(1.0), "the script is asked");
    js(&format!(
        "document.head.querySelector('script[src=\"{GIS_SCRIPT}\"]').dispatchEvent(new Event('error'))"
    ));
    assert_eq!(scripts(), Some(0.0), "the failed script is removed");
    assert_eq!(
        sign_in_now(sync),
        Some(Err(SyncError::Network)),
        "still none"
    );
    assert_eq!(scripts(), Some(1.0), "the tap asked it again");
    js(&format!(
        "document.head.querySelector('script[src=\"{GIS_SCRIPT}\"]').dispatchEvent(new Event('error'))"
    ));
    assert_eq!(scripts(), Some(0.0));
    sync.prepare();
    sync.prepare();
    assert_eq!(scripts(), Some(1.0), "readied again, once");
    let asked = js("return window.__drive.overrides !== undefined");
    assert_eq!(asked.as_bool(), Some(false), "readying asks no token");
    js(FAKE_GIS);
    js(&format!(
        "document.head.querySelector('script[src=\"{GIS_SCRIPT}\"]').dispatchEvent(new Event('load'))"
    ));
    let answer = Rc::new(RefCell::new(None));
    let seen = Rc::clone(&answer);
    sync.sign_in(Box::new(move |result| *seen.borrow_mut() = Some(result)));
    let asked = js("return window.__drive.overrides !== undefined");
    assert_eq!(
        asked.as_bool(),
        Some(true),
        "the window opens inside the tap"
    );
    ticks().await;
    assert_eq!(answer.take(), Some(Ok(())));
    assert_eq!(
        js_string("return window.__drive.config.client_id").as_deref(),
        Some(CLIENT_ID)
    );
    assert_eq!(
        js_string("return window.__drive.config.scope").as_deref(),
        Some(scope().as_str())
    );
    assert_eq!(
        js_string("return window.__drive.overrides.prompt").as_deref(),
        Some("")
    );
    ticks().await;
    assert_eq!(
        sync.account().get_untracked(),
        SyncAccount::SignedIn { email: email() }
    );
    assert_eq!(
        js_string("return localStorage.getItem('pc_learn_sync')"),
        email()
    );
    let leaked = js("return window.__drive.stored.some((value) => value.includes('fake-token'))");
    assert_eq!(
        leaked.as_bool(),
        Some(false),
        "the token is never written to storage"
    );
}

/// No file: created once (multipart, in the app folder), then read, then
/// updated in place; every call carries the token.
async fn create_read_update(sync: &SharedSync) {
    assert_eq!(pull(sync).await, Ok(None));
    assert_eq!(push(sync, "{\"one\":1}").await, Ok(()));
    assert_eq!(
        js_string("return window.__drive.files.f1").as_deref(),
        Some("{\"one\":1}")
    );
    assert_eq!(pull(sync).await, Ok(Some("{\"one\":1}".to_owned())));
    assert_eq!(push(sync, "{\"two\":2}").await, Ok(()));
    assert_eq!(
        js_string("return window.__drive.files.f1").as_deref(),
        Some("{\"two\":2}")
    );
    let creations = js("return window.__drive.calls.filter((c) => c.method === 'POST').length");
    assert_eq!(creations.as_f64(), Some(1.0), "created once");
    let updates = js("return window.__drive.calls.filter((c) => c.method === 'PATCH').length");
    assert_eq!(updates.as_f64(), Some(1.0));
    let bearer = js("return window.__drive.calls.every((c) => c.auth === 'Bearer fake-token')");
    assert_eq!(bearer.as_bool(), Some(true));
    let parent = js(
        "return window.__drive.calls.some((c) => c.method === 'POST' && c.body.includes('\"parents\":[\"appDataFolder\"]'))",
    );
    assert_eq!(parent.as_bool(), Some(true));
}

/// A refused token: SignedOut, the account remembered, the next call
/// refused at once; a new sign-in passes the email as the hint.
async fn refused_then_signed_in_again(sync: &SharedSync) {
    js("window.__drive.fail401 = true");
    assert_eq!(pull(sync).await, Err(SyncError::SignedOut));
    assert_eq!(
        sync.account().get_untracked(),
        SyncAccount::Remembered { email: email() }
    );
    let calls = js("return window.__drive.calls.length");
    assert_eq!(push(sync, "{}").await, Err(SyncError::SignedOut));
    assert_eq!(
        js("return window.__drive.calls.length"),
        calls,
        "no call without a token"
    );
    js("window.__drive.fail401 = false");
    assert_eq!(sign_in(sync).await, Ok(()));
    assert_eq!(
        js_string("return window.__drive.overrides.login_hint"),
        email()
    );
    assert_eq!(
        js("return document.head.querySelectorAll('script[src=\"https://accounts.google.com/gsi/client\"]').length").as_f64(),
        Some(1.0),
        "the script is injected once"
    );
}

/// A sign-out while a pull is on its way: the pull's late 401 neither
/// brings the account back as remembered (no Sync chip after a sign-out)
/// nor, after a new sign-in, drops the new token.
async fn sign_out_cuts_a_pull(sync: &SharedSync) {
    assert_eq!(sign_in(sync).await, Ok(()));
    ticks().await;
    js("window.__drive.fail401 = true; window.__drive.delayMs = 120;");
    let (tx, rx) = oneshot::channel();
    sync.pull(Box::new(move |result| {
        let _ = tx.send(result);
    }));
    sync.sign_out();
    assert_eq!(sync.account().get_untracked(), SyncAccount::SignedOut);
    assert_eq!(
        js_string("return localStorage.getItem('pc_learn_sync')"),
        None
    );
    assert_eq!(sign_in(sync).await, Ok(()), "signed in again, a new token");
    let _ = rx.await.expect("the old pull answers");
    assert!(
        matches!(sync.account().get_untracked(), SyncAccount::SignedIn { .. }),
        "the old pull's 401 does not drop the new token"
    );
    js("window.__drive.fail401 = false; window.__drive.delayMs = 0;");
    assert!(pull(sync).await.is_ok(), "the new token works");
    sync.sign_out();
}

#[wasm_bindgen_test]
async fn the_web_transport_signs_in_syncs_and_signs_out_against_a_fake_google() {
    js("localStorage.removeItem('pc_learn_sync'); delete window.google;");
    js(FAKE_FETCH);
    let sync = learn_pali::web_sync().expect("web-sync: a transport");
    ticks().await;
    assert_eq!(sync.account().get_untracked(), SyncAccount::SignedOut);
    assert_eq!(pull(&sync).await, Err(SyncError::SignedOut), "no token yet");
    first_sign_in(&sync).await;
    create_read_update(&sync).await;
    refused_then_signed_in_again(&sync).await;

    js("window.__drive.popupFails = true");
    assert_eq!(sign_in(&sync).await, Err(SyncError::Popup));
    js("window.__drive.popupFails = false");

    sync.sign_out();
    assert_eq!(
        js("return window.__drive.revoked")
            .dyn_into::<js_sys::Array>()
            .map(|a| a.to_vec()),
        Ok(vec![JsValue::from_str("fake-token")])
    );
    assert_eq!(
        js_string("return localStorage.getItem('pc_learn_sync')"),
        None
    );
    assert_eq!(sync.account().get_untracked(), SyncAccount::SignedOut);

    sign_out_cuts_a_pull(&sync).await;

    // A reload: the account is read back as remembered, with no token.
    js("localStorage.setItem('pc_learn_sync', 'learner@example.org')");
    let reloaded = learn_pali::web_sync().expect("web-sync: a transport");
    assert_eq!(
        reloaded.account().get_untracked(),
        SyncAccount::SignedOut,
        "the first paint"
    );
    ticks().await;
    assert_eq!(
        reloaded.account().get_untracked(),
        SyncAccount::Remembered { email: email() }
    );
    assert_eq!(pull(&reloaded).await, Err(SyncError::SignedOut));
    js("localStorage.removeItem('pc_learn_sync')");
}
