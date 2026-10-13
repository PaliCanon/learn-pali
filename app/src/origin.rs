//! The origin guard (task 031): the service answers Cloudflare's Worker only.
//!
//! The Cloud Run URL is public, so a request that skipped Cloudflare (its
//! WAF, its per-IP limit, its Access) could still reach the service. The
//! Worker adds a secret in `x-origin-auth` to every request it relays; the
//! layer [`router`] puts outermost refuses any request that does not carry
//! it with a 403 and an empty body, the page, the server functions, `/pkg`
//! and the fallback alike. The secret is compared in constant time, lives in
//! [`OriginSecret`] (whose `Debug` is redacted) and is in no log line, no
//! response and no error message. The layer writes nothing: a rejected
//! request leaves no trace on stdout or stderr (`tests/origin.rs` pins it).

use std::fmt;
use std::sync::Arc;

use axum::extract::{Request, State};
use axum::http::{HeaderMap, StatusCode, header};
use axum::middleware::Next;
use axum::response::Response;
use leptos::prelude::LeptosOptions;
use leptos_axum::{LeptosRoutes, generate_route_list};
use subtle::ConstantTimeEq;

use crate::app::{App, shell};

/// The header the Worker puts the secret in.
pub const ORIGIN_HEADER: &str = "x-origin-auth";

/// The variable holding the secret the Worker sends in [`ORIGIN_HEADER`].
pub const ORIGIN_VAR: &str = "ORIGIN_SECRET";

/// The Worker's secret. Its `Debug` never shows it.
#[derive(Clone)]
pub struct OriginSecret(String);

impl OriginSecret {
    /// Wraps the secret read from the environment at boot.
    pub fn new(secret: impl Into<String>) -> Self {
        OriginSecret(secret.into())
    }

    /// Whether `candidate` is the secret.
    #[must_use]
    pub fn matches(&self, candidate: &[u8]) -> bool {
        let secret = self.0.as_bytes();
        // Only the length is public: a different length is refused without
        // comparing; equal lengths are compared without an early exit.
        secret.len() == candidate.len() && secret.ct_eq(candidate).into()
    }
}

impl fmt::Debug for OriginSecret {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("OriginSecret(<redacted>)")
    }
}

/// Why the service refuses to start. Names a variable, never a value.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum OriginError {
    /// The variable is set to nothing, or to blanks.
    Empty,
    /// A deployed build (Cloud Run, or `LEPTOS_ENV` prod) has no origin secret.
    Required,
}

impl OriginError {
    /// The variable the error is about (a name, never a value).
    #[must_use]
    pub fn variable(self) -> &'static str {
        ORIGIN_VAR
    }
}

impl fmt::Display for OriginError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            OriginError::Empty => write!(
                f,
                "{ORIGIN_VAR} is set but empty: give it a value, or unset it for a local run"
            ),
            OriginError::Required => write!(
                f,
                "{ORIGIN_VAR} is not set: a deployed build answers only the requests that carry it"
            ),
        }
    }
}

impl std::error::Error for OriginError {}

/// Whether the service checks where a request came from.
#[derive(Clone, Debug)]
pub enum Origin {
    /// No check: a local run.
    Open,
    /// Every request must carry the secret.
    Checked(OriginSecret),
}

impl Origin {
    /// Reads `ORIGIN_SECRET` through `get` (`std::env::var(..).ok()` in
    /// production, a table in tests): set means checked (trimmed), unset
    /// means open for a local run and an error for a deployed build, blank
    /// is an error either way.
    ///
    /// # Errors
    /// [`OriginError`] when the variable is blank, or unset in a deployed build.
    pub fn from_env(get: impl Fn(&str) -> Option<String>) -> Result<Origin, OriginError> {
        match get(ORIGIN_VAR) {
            Some(value) if value.trim().is_empty() => Err(OriginError::Empty),
            Some(value) => Ok(Origin::Checked(OriginSecret::new(value.trim()))),
            None if deployed(&get) => Err(OriginError::Required),
            None => Ok(Origin::Open),
        }
    }

    /// Whether a request with these headers is let through.
    #[must_use]
    pub fn admits(&self, headers: &HeaderMap) -> bool {
        match self {
            Origin::Open => true,
            Origin::Checked(secret) => {
                let mut values = headers.get_all(ORIGIN_HEADER).iter();
                match (values.next(), values.next()) {
                    (Some(value), None) => secret.matches(value.as_bytes()),
                    // No header, or two of them: refused.
                    _ => false,
                }
            }
        }
    }

    /// The one line logged at boot: whether the check is on, never the secret.
    #[must_use]
    pub fn log_line(&self) -> &'static str {
        match self {
            Origin::Open => "origin: open (local)",
            Origin::Checked(_) => "origin: checked",
        }
    }
}

/// Whether this is a deployed build: Cloud Run sets `K_SERVICE` in every
/// container, and the image sets `LEPTOS_ENV` to `PROD`. A local
/// `cargo leptos watch` (`LEPTOS_ENV=DEV`) is neither.
fn deployed(get: &impl Fn(&str) -> Option<String>) -> bool {
    let production = get("LEPTOS_ENV").is_some_and(|env| {
        let env = env.trim();
        env.eq_ignore_ascii_case("prod") || env.eq_ignore_ascii_case("production")
    });
    production || get("K_SERVICE").is_some()
}

/// The guard, as an axum middleware.
pub async fn guard(State(origin): State<Arc<Origin>>, request: Request, next: Next) -> Response {
    if origin.admits(request.headers()) {
        next.run(request).await
    } else {
        forbidden()
    }
}

/// The refusal: a 403, nothing cacheable, nothing in the body.
fn forbidden() -> Response {
    let mut response = Response::new(axum::body::Body::empty());
    *response.status_mut() = StatusCode::FORBIDDEN;
    response.headers_mut().insert(
        header::CACHE_CONTROL,
        header::HeaderValue::from_static("no-store"),
    );
    response
}

/// The service's router: the page's routes, the static files and the error
/// page as the fallback, `origin` guarding all of it as the outermost layer
/// (the signature asks for it, so `main` cannot forget the guard).
pub fn router(options: LeptosOptions, origin: Origin) -> axum::Router {
    let routes = generate_route_list(App);
    axum::Router::new()
        .leptos_routes(&options, routes, {
            let options = options.clone();
            move || shell(options.clone())
        })
        .fallback(leptos_axum::file_and_error_handler(shell))
        .with_state(options)
        .layer(axum::middleware::from_fn_with_state(
            Arc::new(origin),
            guard,
        ))
}

#[cfg(test)]
mod tests {
    use super::*;

    const SECRET: &str = "origin_canary_unit_41aa";

    fn lookup<'a>(env: &'a [(&'a str, &'a str)]) -> impl Fn(&str) -> Option<String> + 'a {
        move |name| {
            env.iter()
                .find(|(k, _)| *k == name)
                .map(|(_, v)| (*v).to_owned())
        }
    }

    #[test]
    fn only_the_exact_secret_matches() {
        let secret = OriginSecret::new(SECRET);
        assert!(secret.matches(SECRET.as_bytes()));
        let longer = format!("{SECRET}x");
        let same_length = SECRET.replace('a', "b");
        assert_eq!(same_length.len(), SECRET.len());
        for (case, candidate) in [
            ("empty", ""),
            ("prefix", &SECRET[..SECRET.len() - 1]),
            ("suffix", &SECRET[1..]),
            ("longer", longer.as_str()),
            ("same length, other bytes", same_length.as_str()),
        ] {
            assert!(!secret.matches(candidate.as_bytes()), "{case}");
        }
    }

    #[test]
    fn the_debug_of_a_secret_is_redacted() {
        let secret = OriginSecret::new(SECRET);
        assert!(!format!("{secret:?}").contains(SECRET));
        let checked = Origin::Checked(secret);
        assert!(!format!("{checked:?}").contains(SECRET));
    }

    #[test]
    fn an_open_origin_admits_everything_a_checked_one_wants_one_right_header() {
        let mut headers = HeaderMap::new();
        assert!(Origin::Open.admits(&headers));
        let checked = Origin::Checked(OriginSecret::new(SECRET));
        assert!(!checked.admits(&headers), "no header");
        headers.insert(ORIGIN_HEADER, SECRET.parse().unwrap());
        assert!(checked.admits(&headers), "the right header");
        headers.append(ORIGIN_HEADER, SECRET.parse().unwrap());
        assert!(!checked.admits(&headers), "twice");
    }

    #[test]
    fn the_header_name_is_case_blind_and_the_value_is_exact() {
        let checked = Origin::Checked(OriginSecret::new(SECRET));
        // HTTP/1 clients send any case; the map lowercases the name.
        let mut headers = HeaderMap::new();
        headers.insert(
            header::HeaderName::from_bytes(b"X-Origin-Auth").unwrap(),
            SECRET.parse().unwrap(),
        );
        assert!(checked.admits(&headers), "X-Origin-Auth");
        // The value is compared as is: a trailing blank is another value.
        for (case, value) in [
            ("trailing space", format!("{SECRET} ")),
            ("leading space", format!(" {SECRET}")),
            ("trailing tab", format!("{SECRET}\t")),
        ] {
            let mut headers = HeaderMap::new();
            headers.insert(ORIGIN_HEADER, value.parse().unwrap());
            assert!(!checked.admits(&headers), "{case}");
        }
    }

    #[test]
    fn a_header_that_is_not_utf8_text_is_refused() {
        let mut headers = HeaderMap::new();
        headers.insert(
            ORIGIN_HEADER,
            header::HeaderValue::from_bytes(&[0xff, 0xfe, 0x80]).unwrap(),
        );
        let checked = Origin::Checked(OriginSecret::new(SECRET));
        assert!(!checked.admits(&headers));
    }

    #[test]
    fn an_unset_secret_is_open_only_for_a_local_run() {
        for (case, env) in [
            ("nothing set", vec![]),
            ("LEPTOS_ENV=DEV", vec![("LEPTOS_ENV", "DEV")]),
        ] {
            let origin = Origin::from_env(lookup(&env)).unwrap();
            assert!(matches!(origin, Origin::Open), "{case}");
        }
    }

    #[test]
    fn an_unset_secret_in_a_deployed_build_refuses_to_start() {
        for (case, env) in [
            ("K_SERVICE", vec![("K_SERVICE", "learn-pali-dev")]),
            ("LEPTOS_ENV=PROD", vec![("LEPTOS_ENV", "PROD")]),
            ("LEPTOS_ENV=prod", vec![("LEPTOS_ENV", "prod")]),
            ("LEPTOS_ENV=Production", vec![("LEPTOS_ENV", "Production")]),
            (
                "LEPTOS_ENV= production ",
                vec![("LEPTOS_ENV", " production ")],
            ),
        ] {
            let err = Origin::from_env(lookup(&env)).unwrap_err();
            assert_eq!(err, OriginError::Required, "{case}");
        }
    }

    #[test]
    fn a_k_service_that_is_set_but_empty_still_counts_as_deployed() {
        // Cloud Run always sets it; empty is odd, and the safe reading of an
        // odd environment is "deployed": the service refuses to start.
        let env = [("K_SERVICE", "")];
        let err = Origin::from_env(lookup(&env)).unwrap_err();
        assert_eq!(err, OriginError::Required);
    }

    #[test]
    fn a_blank_secret_is_an_error_even_locally() {
        for value in ["", " ", "  \t\n"] {
            let env = [(ORIGIN_VAR, value)];
            let err = Origin::from_env(lookup(&env)).unwrap_err();
            assert_eq!(err, OriginError::Empty, "{value:?}");
        }
    }

    #[test]
    fn a_set_secret_is_checked_and_trimmed() {
        let env = [(ORIGIN_VAR, " origin_canary_unit_41aa\n")];
        let Ok(Origin::Checked(secret)) = Origin::from_env(lookup(&env)) else {
            panic!("expected Checked");
        };
        assert!(secret.matches(SECRET.as_bytes()));
        // Set wins over a deployed build: checked, no error.
        let env = [(ORIGIN_VAR, SECRET), ("K_SERVICE", "learn-pali-dev")];
        assert!(matches!(
            Origin::from_env(lookup(&env)),
            Ok(Origin::Checked(_))
        ));
    }

    #[test]
    fn the_errors_name_the_variable_and_never_the_value() {
        for (case, env) in [
            ("blank", vec![(ORIGIN_VAR, "  ")]),
            ("required", vec![("K_SERVICE", SECRET)]),
        ] {
            let err = Origin::from_env(lookup(&env)).unwrap_err();
            assert_eq!(err.variable(), ORIGIN_VAR, "{case}");
            for shown in [err.to_string(), format!("{err:?}")] {
                assert!(!shown.contains(SECRET), "{case}: leaked: {shown}");
            }
            assert!(err.to_string().contains(ORIGIN_VAR), "{case}");
        }
    }

    #[test]
    fn the_boot_line_says_only_whether_the_check_is_on() {
        let env = [(ORIGIN_VAR, SECRET)];
        let origin = Origin::from_env(lookup(&env)).unwrap();
        assert_eq!(origin.log_line(), "origin: checked");
        assert!(!origin.log_line().contains(SECRET));
        assert_eq!(Origin::Open.log_line(), "origin: open (local)");
    }
}
