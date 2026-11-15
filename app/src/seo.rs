//! `/robots.txt` and `/sitemap.xml`: what lets a search engine find the
//! site (map.palicanon.app and palicanon.app do the same).
//!
//! The site has one stable, indexable page: the home page (the learning
//! loop; the server functions under `/api` and the dictionary routes are
//! not pages, never listed). The page's language is negotiated on that one
//! URL (ui-lib's `resolve_language`: cookie `pc_lang`, then
//! `Accept-Language`), so its only alternate is `x-default`: there is no
//! per-language address to name. Both files ask not to be indexed
//! themselves (`X-Robots-Tag: noindex`).

use axum::http::header;
use axum::routing::get;

/// The site's public address, as the sitemap and robots.txt name it.
const BASE: &str = "https://learn.palicanon.app";

/// Everyone may crawl everything; the sitemap is named. Cloudflare's
/// managed robots.txt prepends its own lines in front of these.
const ROBOTS: &str = concat!(
    "User-agent: *\n",
    "Allow: /\n",
    "\n",
    "Sitemap: https://learn.palicanon.app/sitemap.xml\n",
);

/// The stable pages, by path: the home page only.
const PAGES: [&str; 1] = ["/"];

/// The sitemap: one `<url>` per page of [`PAGES`], with its `x-default`
/// alternate (the page that picks the visitor's language).
fn sitemap() -> String {
    use std::fmt::Write as _;

    let urls = PAGES.iter().fold(String::new(), |mut urls, path| {
        let _ = writeln!(
            urls,
            "  <url><loc>{BASE}{path}</loc>\
             <xhtml:link rel=\"alternate\" hreflang=\"x-default\" href=\"{BASE}{path}\"/></url>"
        );
        urls
    });
    format!(
        "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n\
         <urlset xmlns=\"http://www.sitemaps.org/schemas/sitemap/0.9\" \
         xmlns:xhtml=\"http://www.w3.org/1999/xhtml\">\n{urls}</urlset>\n"
    )
}

/// The two routes, GET (and HEAD), answered before any page is rendered.
pub(crate) fn routes<S>() -> axum::Router<S>
where
    S: Clone + Send + Sync + 'static,
{
    axum::Router::new()
        .route(
            "/robots.txt",
            get(|| async {
                (
                    [
                        (header::CONTENT_TYPE, "text/plain; charset=utf-8"),
                        (header::HeaderName::from_static("x-robots-tag"), "noindex"),
                    ],
                    ROBOTS,
                )
            }),
        )
        .route(
            "/sitemap.xml",
            get(|| async {
                (
                    [
                        (header::CONTENT_TYPE, "application/xml"),
                        (header::HeaderName::from_static("x-robots-tag"), "noindex"),
                    ],
                    sitemap(),
                )
            }),
        )
}

#[cfg(test)]
mod tests {
    use super::*;

    /// robots.txt names the sitemap at the address the sitemap uses.
    #[test]
    fn robots_names_the_sitemap_at_the_base() {
        let want = format!("Sitemap: {BASE}/sitemap.xml");
        assert!(ROBOTS.lines().any(|line| line == want), "{ROBOTS}");
    }
}
