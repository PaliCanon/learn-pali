//! `GET /robots.txt` and `GET /sitemap.xml`: what lets a search engine find
//! learn.palicanon.app. Answered by the server itself, without rendering the
//! app, each asking not to be indexed itself (`X-Robots-Tag: noindex`).
#![cfg(feature = "ssr")]

use axum::Router;
use axum::body::Body;
use axum::http::{HeaderMap, Method, Request, StatusCode};
use learn_pali::origin::{ORIGIN_HEADER, Origin, OriginSecret, router};
use leptos::prelude::LeptosOptions;
use tower::ServiceExt;

const SECRET: &str = "origin_canary_seo_5b1c02";

fn app(origin: Origin) -> Router {
    let _ = any_spawner::Executor::init_tokio();
    let options = LeptosOptions::builder()
        .output_name("learn-pali")
        .site_root("target/site")
        .build();
    router(options, origin)
}

async fn call(origin: Origin, method: Method, uri: &str) -> (StatusCode, HeaderMap, String) {
    let request = Request::builder()
        .method(method)
        .uri(uri)
        .body(Body::empty())
        .unwrap();
    let response = app(origin).oneshot(request).await.unwrap();
    let status = response.status();
    let headers = response.headers().clone();
    let bytes = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    (status, headers, String::from_utf8(bytes.to_vec()).unwrap())
}

fn header<'a>(headers: &'a HeaderMap, name: &str) -> Option<&'a str> {
    headers.get(name).map(|v| v.to_str().unwrap())
}

#[tokio::test]
async fn robots_txt_allows_everything_and_names_the_sitemap() {
    for method in [Method::GET, Method::HEAD] {
        let (status, headers, body) = call(Origin::Open, method.clone(), "/robots.txt").await;
        assert_eq!(status, StatusCode::OK, "{method}");
        assert_eq!(
            header(&headers, "content-type"),
            Some("text/plain; charset=utf-8"),
            "{method}"
        );
        assert_eq!(
            header(&headers, "x-robots-tag"),
            Some("noindex"),
            "{method}"
        );
        if method == Method::GET {
            let lines: Vec<&str> = body.lines().collect();
            assert!(lines.contains(&"User-agent: *"), "{body}");
            assert!(lines.contains(&"Allow: /"), "{body}");
            assert!(
                lines.contains(&"Sitemap: https://learn.palicanon.app/sitemap.xml"),
                "{body}"
            );
            assert!(!body.contains("Disallow"), "{body}");
        }
    }
}

/// The site has one stable page, the home page; its language is negotiated
/// on that one URL (cookie, then `Accept-Language`), so its only alternate
/// is `x-default`. No server function, no dictionary route, no query.
#[tokio::test]
async fn the_sitemap_lists_the_home_page_only() {
    for method in [Method::GET, Method::HEAD] {
        let (status, headers, _) = call(Origin::Open, method.clone(), "/sitemap.xml").await;
        assert_eq!(status, StatusCode::OK, "{method}");
        assert_eq!(
            header(&headers, "content-type"),
            Some("application/xml"),
            "{method}"
        );
        assert_eq!(
            header(&headers, "x-robots-tag"),
            Some("noindex"),
            "{method}"
        );
    }
    let (_, _, body) = call(Origin::Open, Method::GET, "/sitemap.xml").await;
    assert!(
        body.starts_with(r#"<?xml version="1.0" encoding="UTF-8"?>"#),
        "{body}"
    );
    assert!(
        body.contains(r#"xmlns="http://www.sitemaps.org/schemas/sitemap/0.9""#),
        "{body}"
    );
    assert!(
        body.contains(r#"xmlns:xhtml="http://www.w3.org/1999/xhtml""#),
        "{body}"
    );
    let locs: Vec<&str> = body
        .split("<loc>")
        .skip(1)
        .filter_map(|rest| rest.split_once("</loc>").map(|(loc, _)| loc))
        .collect();
    assert_eq!(locs, ["https://learn.palicanon.app/"], "{body}");
    assert!(
        body.contains(
            r#"<xhtml:link rel="alternate" hreflang="x-default" href="https://learn.palicanon.app/"/>"#
        ),
        "{body}"
    );
    let hrefs = body
        .split("href=\"")
        .skip(1)
        .filter_map(|rest| rest.split_once('"').map(|(href, _)| href));
    for url in locs.iter().copied().chain(hrefs) {
        assert!(!url.contains(['?', '&']), "a query in {url}");
        assert!(!url.contains("/api/") && !url.contains("/v1/"), "{url}");
    }
}

/// Both files are behind the origin guard like every route.
#[tokio::test]
async fn robots_and_sitemap_are_behind_the_origin_guard() {
    for path in ["/robots.txt", "/sitemap.xml"] {
        let checked = Origin::Checked(OriginSecret::new(SECRET));
        let (status, _, body) = call(checked, Method::GET, path).await;
        assert_eq!(status, StatusCode::FORBIDDEN, "{path}");
        assert_eq!(body, "", "{path}");

        let request = Request::get(path)
            .header(ORIGIN_HEADER, SECRET)
            .body(Body::empty())
            .unwrap();
        let response = app(Origin::Checked(OriginSecret::new(SECRET)))
            .oneshot(request)
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK, "{path}");
        assert_eq!(
            header(response.headers(), "x-robots-tag"),
            Some("noindex"),
            "{path}"
        );
    }
}
