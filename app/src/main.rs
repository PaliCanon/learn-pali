//! This service's server: renders the page, serves the WASM bundle and
//! the assets. Run with `cargo leptos watch` from the repository root
//! (README.md: the task's own port, 3201). A deployed build answers only
//! the requests that carry `ORIGIN_SECRET` (`origin.rs`, task 031).

#[cfg(feature = "ssr")]
#[tokio::main]
async fn main() {
    use learn_pali::origin::{Origin, router};
    use leptos::prelude::*;

    // The origin guard decides first: a deployed build without its secret
    // does not start. The message names the variable, never a value.
    let origin = match Origin::from_env(|name| std::env::var(name).ok()) {
        Ok(origin) => origin,
        Err(err) => {
            eprintln!("learn-pali: {err}");
            std::process::exit(1);
        }
    };
    println!("{}", origin.log_line());

    let conf = get_configuration(None).expect("leptos configuration");
    let options = conf.leptos_options;
    let addr = options.site_addr;
    let app = router(options, origin);

    let listener = tokio::net::TcpListener::bind(&addr)
        .await
        .expect("bind this service's address");
    println!("learn-pali on http://{addr}");
    axum::serve(listener, app.into_make_service())
        .await
        .expect("serve this service");
}

#[cfg(not(feature = "ssr"))]
fn main() {}
