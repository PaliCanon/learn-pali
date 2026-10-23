//! The server functions are reachable under `/api` (062): the router
//! registers them, the origin guard sits in front, and with no API
//! configured a search answers `Unreachable` as data, never a 404 or a
//! 500. Driven through `tower::ServiceExt::oneshot`, no network.

// The server's side only: the router and axum exist under `ssr`.
#![cfg(feature = "ssr")]

use axum::Router;
use axum::body::Body;
use axum::http::{Request, StatusCode, header};
use learn_pali::origin::{ORIGIN_HEADER, Origin, OriginSecret, router};
use leptos::prelude::LeptosOptions;
use tower::ServiceExt;

const SECRET: &str = "origin-secret-for-tests";

fn app() -> Router {
    let _ = any_spawner::Executor::init_tokio();
    let options = LeptosOptions::builder()
        .output_name("learn-pali")
        .site_root("target/site")
        .build();
    router(options, Origin::Checked(OriginSecret::new(SECRET)))
}

fn post(uri: &str, body: &'static str, with_origin: bool) -> Request<Body> {
    let content_type = if body.starts_with('{') {
        "application/json"
    } else {
        "application/x-www-form-urlencoded"
    };
    let mut builder = Request::builder()
        .method("POST")
        .uri(uri)
        .header(header::CONTENT_TYPE, content_type)
        .header(header::ACCEPT, "application/json");
    if with_origin {
        builder = builder.header(ORIGIN_HEADER, SECRET);
    }
    builder.body(Body::from(body)).unwrap()
}

async fn body_of(response: axum::response::Response) -> (StatusCode, String) {
    let status = response.status();
    let bytes = axum::body::to_bytes(response.into_body(), 1 << 20)
        .await
        .unwrap();
    (status, String::from_utf8_lossy(&bytes).into_owned())
}

#[tokio::test]
async fn search_words_is_served_under_api_and_answers_unreachable_without_an_api() {
    let response = app()
        .oneshot(post("/api/search_words", "q=dham", true))
        .await
        .unwrap();
    let (status, body) = body_of(response).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert!(body.contains("Unreachable"), "data, not an error: {body}");
}

#[tokio::test]
async fn passage_for_is_served_under_api_and_answers_transient_without_an_api() {
    let entry = r#"{"entry":{"headword":"vakkali","lemma":"vakkali","pos":"masc","meaning":"a monk","example":"alaṃ vakkali","source":"SN22.87","sutta":"Vakkalisutta"}}"#;
    let response = app()
        .oneshot(post("/api/passage_for", entry, true))
        .await
        .unwrap();
    let (status, body) = body_of(response).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert!(
        body.contains("Transient"),
        "nothing added, try again: {body}"
    );
}

#[tokio::test]
async fn a_server_function_without_the_origin_header_is_refused() {
    let response = app()
        .oneshot(post("/api/search_words", "q=dham", false))
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::FORBIDDEN);
}

#[tokio::test]
async fn an_unknown_server_function_is_refused_not_the_page() {
    let response = app()
        .oneshot(post("/api/nothing_here", "q=dham", true))
        .await
        .unwrap();
    // The server-function handler answers a 400 for a name it does not
    // know; what matters is that the page's fallback never answers here.
    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
}
