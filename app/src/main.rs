//! This service's server: renders the page, serves the WASM bundle and
//! the assets. Run with `cargo leptos watch` from the repository root
//! (README.md: the task's own port, 3201). A deployed build answers only
//! the requests that carry `ORIGIN_SECRET` (`origin.rs`, task 031).

#[cfg(feature = "ssr")]
#[tokio::main]
async fn main() {
    use learn_pali::telemetry::server;
    server::init();
    if let Err(stage) = serve().await {
        server::failed(stage);
        std::process::exit(1);
    }
    server::stopped();
}

#[cfg(feature = "ssr")]
async fn serve() -> Result<(), &'static str> {
    use learn_pali::origin::{Origin, router};
    use learn_pali::telemetry::server;
    use leptos::prelude::*;

    let started = std::time::Instant::now();
    let origin = Origin::from_env(|name| std::env::var(name).ok()).map_err(|_| "origin_config")?;
    let checked = matches!(origin, Origin::Checked(_));
    let passages = learn_pali::dictionary::server::init_passages(|name| std::env::var(name).ok())
        .map_err(|_| "passages_asset")?
        .is_some();
    let conf = get_configuration(None).map_err(|_| "leptos_config")?;
    let options = conf.leptos_options;
    let addr = options.site_addr;
    let app = router(options, origin);

    let listener = tokio::net::TcpListener::bind(&addr)
        .await
        .map_err(|_| "listen")?;
    server::started(checked, passages, started.elapsed());
    axum::serve(listener, app.into_make_service())
        .with_graceful_shutdown(server::shutdown())
        .await
        .map_err(|_| "serve")
}

#[cfg(not(feature = "ssr"))]
fn main() {}
