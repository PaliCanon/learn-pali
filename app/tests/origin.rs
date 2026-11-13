//! The origin guard on the real router (task 031, slice G): every request
//! row of the case table, driven through `tower::ServiceExt::oneshot`.
//! Bounded, redacted refusal logs are exercised in `telemetry.rs`.
#![cfg(feature = "ssr")]

use axum::Router;
use axum::body::Body;
use axum::http::{Request, Response, StatusCode, header};
use learn_pali::origin::{ORIGIN_HEADER, Origin, OriginSecret, router};
use leptos::prelude::LeptosOptions;
use tower::ServiceExt;

const SECRET: &str = "origin_canary_it_7d3e19";

fn app(origin: Origin) -> Router {
    // The executor the page's streaming render awaits, as the server runs.
    let _ = any_spawner::Executor::init_tokio();
    let options = LeptosOptions::builder()
        .output_name("learn-pali")
        .site_root("target/site")
        .build();
    router(options, origin)
}

fn checked() -> Router {
    app(Origin::Checked(OriginSecret::new(SECRET)))
}

fn request(method: &str, uri: &str, headers: &[(&str, &str)]) -> Request<Body> {
    let mut builder = Request::builder().method(method).uri(uri);
    for (name, value) in headers {
        builder = builder.header(*name, *value);
    }
    builder.body(Body::empty()).unwrap()
}

async fn send(app: Router, request: Request<Body>) -> (Response<Body>, Vec<u8>) {
    let response = app.oneshot(request).await.unwrap();
    let (parts, body) = response.into_parts();
    let bytes = axum::body::to_bytes(body, usize::MAX).await.unwrap();
    (Response::from_parts(parts, Body::empty()), bytes.to_vec())
}

/// A refusal: 403, an empty body, `no-store`, and nothing of the secret.
fn assert_refused(case: &str, response: &Response<Body>, body: &[u8]) {
    assert_eq!(response.status(), StatusCode::FORBIDDEN, "{case}");
    assert_eq!(body.len(), 0, "{case}: body not empty");
    assert_eq!(
        response.headers().get(header::CACHE_CONTROL).unwrap(),
        "no-store",
        "{case}"
    );
    assert!(response.headers().get(ORIGIN_HEADER).is_none(), "{case}");
    for (name, value) in response.headers() {
        assert!(
            !value.to_str().unwrap_or_default().contains(SECRET),
            "{case}: secret in {name}"
        );
    }
}

#[tokio::test]
async fn a_request_without_the_header_is_refused_on_every_route() {
    for (method, uri) in [
        ("GET", "/"),
        ("POST", "/api/some_server_fn"),
        ("GET", "/pkg/learn-pali.js"),
        ("GET", "/pkg/learn-pali.wasm"),
        ("GET", "/favicon.ico"),
        ("GET", "/no/such/page"),
    ] {
        let (response, body) = send(checked(), request(method, uri, &[])).await;
        assert_refused(&format!("{method} {uri}"), &response, &body);
    }
}

#[tokio::test]
async fn a_wrong_empty_or_near_miss_value_is_refused() {
    let longer = format!("{SECRET}x");
    let cases = [
        ("wrong", "not-the-secret".to_owned()),
        ("empty", String::new()),
        ("a prefix", SECRET[..SECRET.len() - 1].to_owned()),
        ("the secret plus one byte", longer),
    ];
    for (case, value) in cases {
        let (response, body) =
            send(checked(), request("GET", "/", &[(ORIGIN_HEADER, &value)])).await;
        assert_refused(case, &response, &body);
    }
}

#[tokio::test]
async fn two_headers_are_refused_even_when_one_is_right() {
    let two = Request::builder()
        .uri("/")
        .header(ORIGIN_HEADER, SECRET)
        .header(ORIGIN_HEADER, "other")
        .body(Body::empty())
        .unwrap();
    let (response, body) = send(checked(), two).await;
    assert_refused("right then wrong", &response, &body);
    let two = Request::builder()
        .uri("/")
        .header(ORIGIN_HEADER, SECRET)
        .header(ORIGIN_HEADER, SECRET)
        .body(Body::empty())
        .unwrap();
    let (response, body) = send(checked(), two).await;
    assert_refused("right twice", &response, &body);
}

#[tokio::test]
async fn the_right_header_gets_the_routes_usual_answer() {
    let (response, body) = send(checked(), request("GET", "/", &[(ORIGIN_HEADER, SECRET)])).await;
    assert_eq!(response.status(), StatusCode::OK);
    assert!(!body.is_empty(), "the page renders");
    assert!(!String::from_utf8_lossy(&body).contains(SECRET));
    // Past the guard, an unknown path is the app's own 404, not a 403.
    let (response, _) = send(
        checked(),
        request("GET", "/pkg/missing.js", &[(ORIGIN_HEADER, SECRET)]),
    )
    .await;
    assert_eq!(response.status(), StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn an_open_origin_checks_nothing() {
    let (response, _) = send(app(Origin::Open), request("GET", "/", &[])).await;
    assert_eq!(response.status(), StatusCode::OK);
}
