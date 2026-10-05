//! This service's server: renders the page, serves the WASM bundle and
//! the assets. Run with `cargo leptos watch` from the repository root
//! (README.md: the task's own port, 3201).

#[cfg(feature = "ssr")]
#[tokio::main]
async fn main() {
    use axum::Router;
    use learn_pali::app::{App, shell};
    use leptos::prelude::*;
    use leptos_axum::{LeptosRoutes, generate_route_list};

    let conf = get_configuration(None).expect("leptos configuration");
    let options = conf.leptos_options;
    let addr = options.site_addr;
    let routes = generate_route_list(App);
    let app = Router::new()
        .leptos_routes(&options, routes, {
            let options = options.clone();
            move || shell(options.clone())
        })
        .fallback(leptos_axum::file_and_error_handler(shell))
        .with_state(options);

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
