//! The browser side of the web sync: Google Identity Services' token
//! client, `fetch` to the Drive, `localStorage[pc_learn_sync]`. Tested in
//! a headless Chrome against a fake Google (`tests/web_sync.rs`).

use std::cell::{Cell, RefCell};
use std::rc::Rc;

use js_sys::{Function, Object, Reflect};
use leptos::prelude::*;
use wasm_bindgen::closure::Closure;
use wasm_bindgen::{JsCast, JsValue};
use wasm_bindgen_futures::JsFuture;
use web_sys::{Headers, Request, RequestInit, Response};

use super::{
    CLIENT_ID, GIS_SCRIPT, REMEMBER_KEY, Token, USERINFO_URL, checked, created_id, email_of,
    expired, first_file_id, remembered, scope, sign_in_error, token_of, userinfo_email,
};
use crate::backend::{SharedSync, SyncAccount, SyncError, SyncTransport};
use crate::drive_location::{CREATE_URL, create_body, list_url, read_url, update_url};

/// A sign-in, a push: what waits for their answer.
type Done = Box<dyn FnOnce(Result<(), SyncError>)>;

/// The transport's state, shared with Google's callbacks.
struct Inner {
    /// Who is signed in.
    account: RwSignal<SyncAccount>,
    /// The access token, in memory only.
    token: RefCell<Option<Token>>,
    /// The progress file's id, once a pull or a creation resolved it.
    file_id: RefCell<Option<String>>,
    /// The token client, made once the script is there.
    client: RefCell<Option<JsValue>>,
    /// The sign-in waiting for Google's answer.
    waiting: RefCell<Option<Done>>,
    /// Google's script while it loads (none: not asked, loaded, or failed
    /// and removed, so the next readying asks it again).
    loading: RefCell<Option<web_sys::HtmlScriptElement>>,
    /// Moved by each sign-out: a call's answer from an older generation
    /// never touches the token or the account (a late 401 of a cut pull).
    generation: Cell<u64>,
}

/// The web transport over [`Inner`].
struct WebSync(Rc<Inner>);

/// The site's transport. The remembered account is read once the page is
/// hydrated (an animation frame later), so the first paint matches the
/// server's, which is always signed out.
pub(super) fn transport() -> SharedSync {
    let inner = Rc::new(Inner {
        account: RwSignal::new(SyncAccount::SignedOut),
        token: RefCell::new(None),
        file_id: RefCell::new(None),
        client: RefCell::new(None),
        waiting: RefCell::new(None),
        loading: RefCell::new(None),
        generation: Cell::new(0),
    });
    let account = inner.account;
    request_animation_frame(move || {
        if matches!(account.get_untracked(), SyncAccount::SignedOut) {
            account.set(remembered(storage_get()));
        }
    });
    Rc::new(WebSync(inner))
}

impl SyncTransport for WebSync {
    /// Inside the tap: Google's window opens at once when the script is
    /// there (readied when the sync UI showed); else the answer is Network
    /// at once (Retry) and the script is asked, never a window opened later
    /// outside the gesture, which a browser blocks.
    fn sign_in(&self, on_done: Box<dyn FnOnce(Result<(), SyncError>)>) {
        if oauth2().is_some() {
            self.0.request(on_done);
        } else {
            ready_gis(&self.0);
            on_done(Err(SyncError::Network));
        }
    }

    fn prepare(&self) {
        ready_gis(&self.0);
    }

    fn sign_out(&self) {
        if let Some(token) = self.0.token.borrow_mut().take()
            && let Some(revoke) = oauth2().and_then(|oauth2| function(&oauth2, "revoke"))
        {
            let _ = revoke.call2(
                &JsValue::NULL,
                &token.access.into(),
                &Function::new_no_args(""),
            );
        }
        *self.0.file_id.borrow_mut() = None;
        self.0.generation.set(self.0.generation.get() + 1);
        storage_remove();
        self.0.account.set(SyncAccount::SignedOut);
    }

    fn account(&self) -> Signal<SyncAccount> {
        self.0.account.into()
    }

    /// The account against the clock: a signed-in account whose token has
    /// expired is remembered (paused). No request, no timer, no Google
    /// call; the remembered email in storage is untouched.
    fn refresh(&self) {
        let now = js_sys::Date::now();
        let next = expired(
            &self.0.account.get_untracked(),
            self.0.token.borrow().as_ref(),
            now,
        );
        if let Some(next) = next {
            *self.0.token.borrow_mut() = None;
            self.0.account.set(next);
        }
    }

    fn pull(&self, on_done: Box<dyn FnOnce(Result<Option<String>, SyncError>)>) {
        let Some(bearer) = self.0.bearer() else {
            self.0.lost_token();
            on_done(Err(SyncError::SignedOut));
            return;
        };
        let inner = Rc::clone(&self.0);
        let generation = self.0.generation.get();
        wasm_bindgen_futures::spawn_local(async move {
            let pulled = inner.pull(&bearer).await;
            if pulled == Err(SyncError::SignedOut) && inner.generation.get() == generation {
                inner.lost_token();
            }
            on_done(pulled);
        });
    }

    fn push(&self, text: &str, on_done: Box<dyn FnOnce(Result<(), SyncError>)>) {
        let Some(bearer) = self.0.bearer() else {
            self.0.lost_token();
            on_done(Err(SyncError::SignedOut));
            return;
        };
        let inner = Rc::clone(&self.0);
        let text = text.to_owned();
        let generation = self.0.generation.get();
        wasm_bindgen_futures::spawn_local(async move {
            let pushed = inner.push(&bearer, &text).await;
            if pushed == Err(SyncError::SignedOut) && inner.generation.get() == generation {
                inner.lost_token();
            }
            on_done(pushed);
        });
    }
}

impl Inner {
    /// The token, while it is valid.
    fn bearer(&self) -> Option<String> {
        let now = js_sys::Date::now();
        self.token
            .borrow()
            .as_ref()
            .filter(|token| token.valid(now))
            .map(|token| token.access.clone())
    }

    /// The token is gone or refused: forgotten; the account, remembered.
    fn lost_token(&self) {
        *self.token.borrow_mut() = None;
        let email = email_of(&self.account.get_untracked());
        if !matches!(self.account.get_untracked(), SyncAccount::SignedOut) {
            self.account.set(SyncAccount::Remembered { email });
        }
    }

    /// Asks Google for a token, in the gesture; `on_done` waits for the
    /// answer (a sign-in still waiting is answered `Cancelled`).
    fn request(self: &Rc<Self>, on_done: Done) {
        if let Some(earlier) = self.waiting.borrow_mut().replace(on_done) {
            earlier(Err(SyncError::Cancelled));
        }
        let Some(client) = self.client() else {
            self.answer(Err(SyncError::Network));
            return;
        };
        let overrides = Object::new();
        set(&overrides, "prompt", &"".into());
        if let Some(email) = email_of(&self.account.get_untracked()) {
            set(&overrides, "login_hint", &email.into());
        }
        if let Some(request) = function(&client, "requestAccessToken") {
            let _ = request.call1(&client, &overrides);
        }
    }

    /// Answers the waiting sign-in.
    fn answer(&self, result: Result<(), SyncError>) {
        let waiting = self.waiting.borrow_mut().take();
        if let Some(on_done) = waiting {
            on_done(result);
        }
    }

    /// The token client, made on first use with its two callbacks.
    fn client(self: &Rc<Self>) -> Option<JsValue> {
        if let Some(client) = self.client.borrow().clone() {
            return Some(client);
        }
        let init = function(&oauth2()?, "initTokenClient")?;
        let config = Object::new();
        set(&config, "client_id", &CLIENT_ID.into());
        set(&config, "scope", &scope().into());
        let inner = Rc::clone(self);
        let callback = Closure::<dyn Fn(JsValue)>::new(move |answer: JsValue| {
            inner.token_answer(&answer);
        });
        set(&config, "callback", &callback.into_js_value());
        let inner = Rc::clone(self);
        let failed = Closure::<dyn Fn(JsValue)>::new(move |error: JsValue| {
            inner.answer(Err(sign_in_error(text(&error, "type").as_deref())));
        });
        set(&config, "error_callback", &failed.into_js_value());
        let client = init.call1(&JsValue::NULL, &config).ok()?;
        *self.client.borrow_mut() = Some(client.clone());
        Some(client)
    }

    /// Google's token answer: kept in memory, the account signed in and
    /// remembered, the email asked; the waiting sign-in answered.
    fn token_answer(self: &Rc<Self>, answer: &JsValue) {
        let expires_in = Reflect::get(answer, &"expires_in".into())
            .ok()
            .and_then(|value| value.as_f64().or_else(|| value.as_string()?.parse().ok()));
        let token = token_of(
            text(answer, "access_token"),
            expires_in,
            text(answer, "error"),
            js_sys::Date::now(),
        );
        match token {
            Ok(token) => {
                let bearer = token.access.clone();
                *self.token.borrow_mut() = Some(token);
                let email = email_of(&self.account.get_untracked());
                // An empty value is no account: the email is written once
                // known (here, or when userinfo answers).
                if let Some(email) = &email {
                    storage_set(email);
                }
                self.account.set(SyncAccount::SignedIn { email });
                self.answer(Ok(()));
                let inner = Rc::clone(self);
                wasm_bindgen_futures::spawn_local(async move { inner.ask_email(&bearer).await });
            }
            Err(error) => self.answer(Err(error)),
        }
    }

    /// Reads the account's email (userinfo) and remembers it.
    async fn ask_email(&self, bearer: &str) {
        let Ok((200, body)) = fetch("GET", USERINFO_URL, bearer, None).await else {
            return;
        };
        if let Some(email) = userinfo_email(&body) {
            storage_set(&email);
            if matches!(self.account.get_untracked(), SyncAccount::SignedIn { .. }) {
                self.account
                    .set(SyncAccount::SignedIn { email: Some(email) });
            }
        }
    }

    /// Lists the app folder, then reads the file if there is one.
    async fn pull(&self, bearer: &str) -> Result<Option<String>, SyncError> {
        let (status, body) = fetch("GET", &list_url(), bearer, None).await?;
        checked(status, &body)?;
        let id = first_file_id(&body)?;
        self.file_id.borrow_mut().clone_from(&id);
        let Some(id) = id else {
            log("no Drive file yet");
            return Ok(None);
        };
        log(&format!("Drive file id {id}"));
        let (status, body) = fetch("GET", &read_url(&id), bearer, None).await?;
        checked(status, &body)?;
        Ok(Some(body))
    }

    /// Updates the file, or creates it when the last pull found none.
    async fn push(&self, bearer: &str, text: &str) -> Result<(), SyncError> {
        let id = self.file_id.borrow().clone();
        if let Some(id) = id {
            let body = Some(("application/json", text));
            let (status, answer) = fetch("PATCH", &update_url(&id), bearer, body).await?;
            return checked(status, &answer);
        }
        let (content_type, body) = create_body(text);
        let (status, answer) =
            fetch("POST", CREATE_URL, bearer, Some((&content_type, &body))).await?;
        checked(status, &answer)?;
        let id = created_id(&answer)?;
        log(&format!("Drive file id {id} (created)"));
        *self.file_id.borrow_mut() = Some(id);
        Ok(())
    }
}

/// One `fetch` with the Bearer token: the status and the body's text.
async fn fetch(
    method: &str,
    url: &str,
    bearer: &str,
    body: Option<(&str, &str)>,
) -> Result<(u16, String), SyncError> {
    let headers = Headers::new().map_err(|_| SyncError::Network)?;
    let _ = headers.set("Authorization", &format!("Bearer {bearer}"));
    let init = RequestInit::new();
    init.set_method(method);
    if let Some((content_type, body)) = body {
        let _ = headers.set("Content-Type", content_type);
        init.set_body(&body.into());
    }
    init.set_headers(&headers);
    let request = Request::new_with_str_and_init(url, &init).map_err(|_| SyncError::Network)?;
    let window = web_sys::window().ok_or(SyncError::Network)?;
    let response: Response = JsFuture::from(window.fetch_with_request(&request))
        .await
        .map_err(|_| SyncError::Network)?
        .dyn_into()
        .map_err(|_| SyncError::Network)?;
    let text = response.text().map_err(|_| SyncError::Network)?;
    let text = JsFuture::from(text).await.map_err(|_| SyncError::Network)?;
    Ok((response.status(), text.as_string().unwrap_or_default()))
}

/// Asks for Google's script unless it is there or loading: injected once;
/// a script that fails to load is removed, so the next readying asks again.
/// No Google call, no token asked.
fn ready_gis(inner: &Rc<Inner>) {
    if oauth2().is_some() || inner.loading.borrow().is_some() {
        return;
    }
    let Some(document) = web_sys::window().and_then(|window| window.document()) else {
        return;
    };
    let Some(script) = document
        .create_element("script")
        .ok()
        .and_then(|element| element.dyn_into::<web_sys::HtmlScriptElement>().ok())
    else {
        return;
    };
    script.set_src(GIS_SCRIPT);
    script.set_async(true);
    let (for_load, for_error) = (Rc::clone(inner), Rc::clone(inner));
    let onload = Closure::once_into_js(move || {
        for_load.loading.borrow_mut().take();
    });
    let onerror = Closure::once_into_js(move || {
        if let Some(failed) = for_error.loading.borrow_mut().take() {
            failed.remove();
        }
    });
    let _ = script.add_event_listener_with_callback("load", onload.unchecked_ref());
    let _ = script.add_event_listener_with_callback("error", onerror.unchecked_ref());
    if document
        .head()
        .is_some_and(|head| head.append_child(&script).is_ok())
    {
        *inner.loading.borrow_mut() = Some(script);
    }
}

/// `google.accounts.oauth2`, once the script defined it.
fn oauth2() -> Option<JsValue> {
    let mut value: JsValue = web_sys::window()?.into();
    for name in ["google", "accounts", "oauth2"] {
        value = Reflect::get(&value, &name.into()).ok()?;
        if value.is_undefined() || value.is_null() {
            return None;
        }
    }
    Some(value)
}

/// The function `name` of `object`.
fn function(object: &JsValue, name: &str) -> Option<Function> {
    Reflect::get(object, &name.into()).ok()?.dyn_into().ok()
}

/// `object[name] = value`.
fn set(object: &Object, name: &str, value: &JsValue) {
    let _ = Reflect::set(object, &name.into(), value);
}

/// The string `object[name]`, if it is one.
fn text(object: &JsValue, name: &str) -> Option<String> {
    Reflect::get(object, &name.into()).ok()?.as_string()
}

/// Logs a line to the console, prefixed.
fn log(line: &str) {
    web_sys::console::log_1(&format!("learn-pali sync: {line}").into());
}

/// `localStorage`, if the browser gives it.
fn storage() -> Option<web_sys::Storage> {
    web_sys::window()?.local_storage().ok().flatten()
}

/// `localStorage[pc_learn_sync]`.
fn storage_get() -> Option<String> {
    storage()?.get_item(REMEMBER_KEY).ok().flatten()
}

/// Remembers the sync is on, with `email` as the hint (no token).
fn storage_set(email: &str) {
    if let Some(storage) = storage() {
        let _ = storage.set_item(REMEMBER_KEY, email);
    }
}

/// Forgets the sync.
fn storage_remove() {
    if let Some(storage) = storage() {
        let _ = storage.remove_item(REMEMBER_KEY);
    }
}
