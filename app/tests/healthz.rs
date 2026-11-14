//! `GET /healthz` (task 085): the uniform liveness check of every public
//! service. Answered by the server itself, without rendering the app, never
//! cached.
#![cfg(feature = "ssr")]

use axum::Router;
use axum::body::Body;
use axum::http::{Request, StatusCode, header};
use learn_pali::origin::{Origin, router};
use leptos::prelude::LeptosOptions;
use tower::ServiceExt;

fn app() -> Router {
    let _ = any_spawner::Executor::init_tokio();
    let options = LeptosOptions::builder()
        .output_name("learn-pali")
        .site_root("target/site")
        .build();
    router(options, Origin::Open)
}

async fn get(uri: &str) -> (StatusCode, Option<String>, String) {
    let request = Request::get(uri).body(Body::empty()).unwrap();
    let response = app().oneshot(request).await.unwrap();
    let status = response.status();
    let cache = response
        .headers()
        .get(header::CACHE_CONTROL)
        .map(|v| v.to_str().unwrap().to_owned());
    let bytes = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    (status, cache, String::from_utf8(bytes.to_vec()).unwrap())
}

#[tokio::test]
async fn healthz_answers_200_ok_uncached_without_the_app() {
    let (status, cache, body) = get("/healthz").await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body, "ok");
    assert_eq!(cache.as_deref(), Some("no-store"));
}

#[tokio::test]
async fn healthz_takes_get_only() {
    let request = Request::post("/healthz").body(Body::empty()).unwrap();
    let response = app().oneshot(request).await.unwrap();
    assert_ne!(response.status(), StatusCode::OK);
}
