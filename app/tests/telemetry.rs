//! Actual JSON output and HTTP redaction, without any telemetry backend.
#![cfg(feature = "ssr")]

use std::io::{self, Write};
use std::process::Command;
use std::sync::{Arc, Mutex};

#[derive(Clone, Default)]
struct Capture(Arc<Mutex<Vec<u8>>>);

impl Write for Capture {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        self.0.lock().unwrap().extend_from_slice(bytes);
        Ok(bytes.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

#[tokio::test(flavor = "current_thread")]
async fn origin_refusals_are_bounded_and_never_log_request_data() {
    use axum::{Router, body::Body, http::Request, routing::get};
    use learn_pali::origin::{Origin, OriginSecret};
    use tower::ServiceExt;
    use tracing::instrument::WithSubscriber;
    let captured = Capture::default();
    let writer = captured.clone();
    let subscriber = tracing_subscriber::fmt()
        .json()
        .flatten_event(true)
        .with_writer(move || writer.clone())
        .finish();
    let dispatch = tracing::Dispatch::new(subscriber);
    let app = Router::new().route("/", get(|| async { "ok" })).layer(
        axum::middleware::from_fn_with_state(
            Arc::new(Origin::Checked(OriginSecret::new("canary-origin"))),
            learn_pali::origin::guard,
        ),
    );
    for _ in 0..5 {
        let request = Request::builder()
            .uri("/?q=canary-query")
            .header("x-origin-auth", "canary-wrong")
            .body(Body::empty())
            .unwrap();
        let response = app
            .clone()
            .oneshot(request)
            .with_subscriber(dispatch.clone())
            .await
            .unwrap();
        assert_eq!(response.status(), 403);
    }
    let bytes = captured.0.lock().unwrap();
    let text = String::from_utf8_lossy(&bytes);
    assert_eq!(text.lines().count(), 1);
    assert!(!text.contains("canary-"));
    let log: serde_json::Value = serde_json::from_str(text.trim()).unwrap();
    assert_eq!(log["event.name"], "origin.refused");
    assert_eq!(log["severity"], "WARNING");
}

#[tokio::test(flavor = "current_thread")]
async fn http_logs_have_safe_routes_request_ids_and_correct_error_severity() {
    use axum::{Router, body::Body, http::Request, routing::get};
    use tower::ServiceExt;
    use tracing::instrument::WithSubscriber;
    let captured = Capture::default();
    let writer = captured.clone();
    let subscriber = tracing_subscriber::fmt()
        .json()
        .flatten_event(true)
        .with_writer(move || writer.clone())
        .finish();
    let app = Router::new()
        .route(
            "/word/{word}",
            get(|| async { axum::http::StatusCode::BAD_GATEWAY }),
        )
        .layer(axum::middleware::from_fn(
            learn_pali::telemetry::server::request,
        ));
    let request = Request::builder()
        .uri("/word/canary-word?q=canary-query")
        .header("authorization", "Bearer canary-token")
        .header("cookie", "account=canary-email@example.com")
        .header("x-request-id", "canary-request-id")
        .body(Body::empty())
        .unwrap();
    assert_eq!(
        app.oneshot(request)
            .with_subscriber(subscriber)
            .await
            .unwrap()
            .status(),
        502
    );
    let bytes = captured.0.lock().unwrap();
    let text = String::from_utf8_lossy(&bytes);
    assert!(!text.contains("canary-"), "{text}");
    let log: serde_json::Value = serde_json::from_str(text.trim()).unwrap();
    assert_eq!(log["http.route"], "/word/{word}");
    assert_eq!(log["http.response.status_code"], 502);
    assert_eq!(log["severity"], "ERROR");
    assert!(log["duration_ms"].as_f64().is_some());
    assert!(log["span"]["request.id"].as_str().is_some());
}

#[test]
fn startup_failure_carries_the_deployed_tag_and_never_the_configuration_values() {
    let output = Command::new(env!("CARGO_BIN_EXE_learn_pali"))
        .env("ORIGIN_SECRET", " ")
        .env("LEARN_PALI_ENV", "v9.8.7")
        .env("LEARN_PALI_SHA", "abcdef0123")
        .env("LEARN_PALI_DEPLOYMENT_ENV", "test")
        .env("LEARN_PALI_LOG_LEVEL", "info")
        .env("LEARN_PALI_API_TOKEN", "canary-api-secret")
        .env("OTEL_EXPORTER_OTLP_ENDPOINT", "http://127.0.0.1:1")
        .output()
        .unwrap();
    assert!(!output.status.success());
    assert_eq!(output.stderr.len(), 0);
    let text = String::from_utf8(output.stdout).unwrap();
    assert!(!text.contains("canary-api-secret"));
    let lines: Vec<serde_json::Value> = text
        .lines()
        .map(|line| serde_json::from_str(line).unwrap())
        .collect();
    assert_eq!(lines.len(), 1);
    assert_eq!(lines[0]["service.version"], "v9.8.7");
    assert_eq!(lines[0]["service.commit"], "abcdef0");
    assert_eq!(lines[0]["deployment.environment.name"], "test");
    assert_eq!(lines[0]["event.name"], "service.failed");
    assert_eq!(lines[0]["severity"], "ERROR");
    assert_eq!(lines[0]["stage"], "origin_config");
}
