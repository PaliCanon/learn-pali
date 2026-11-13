//! Actual binary startup for the tunnel/Cloud Run origin policy.
#![cfg(feature = "ssr")]

use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant};

struct Service(Child);

impl Drop for Service {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

#[test]
fn production_without_k_service_or_secret_serves_without_the_header() {
    let reservation = TcpListener::bind("127.0.0.1:0").unwrap();
    let address = reservation.local_addr().unwrap();
    drop(reservation);
    let child = Command::new(env!("CARGO_BIN_EXE_learn_pali"))
        .env_clear()
        .env("LEPTOS_ENV", "PROD")
        .env("LEPTOS_OUTPUT_NAME", "learn-pali")
        .env("LEPTOS_SITE_ADDR", address.to_string())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .unwrap();
    let mut service = Service(child);
    let deadline = Instant::now() + Duration::from_secs(10);
    let mut stream = loop {
        if let Ok(stream) = TcpStream::connect(address) {
            break stream;
        }
        assert!(
            service.0.try_wait().unwrap().is_none(),
            "boot refused outside Cloud Run"
        );
        assert!(Instant::now() < deadline, "no listener");
        std::thread::sleep(Duration::from_millis(20));
    };
    stream
        .set_read_timeout(Some(Duration::from_secs(10)))
        .unwrap();
    write!(stream, "GET / HTTP/1.0\r\nHost: localhost\r\n\r\n").unwrap();
    let mut text = String::new();
    stream.read_to_string(&mut text).unwrap();
    assert!(
        text.starts_with("HTTP/1.0 200") || text.starts_with("HTTP/1.1 200"),
        "{text}"
    );
}

#[test]
fn cloud_run_without_a_secret_refuses_before_binding_the_port() {
    // A missing guard cannot hang this test: the reserved port forces a
    // bounded listen failure, whose stage differs from the required origin error.
    let reserved = TcpListener::bind("127.0.0.1:0").unwrap();
    for service in ["learn-pali-dev", ""] {
        let output = Command::new(env!("CARGO_BIN_EXE_learn_pali"))
            .env_clear()
            .env("K_SERVICE", service)
            .env("LEPTOS_ENV", "PROD")
            .env("LEPTOS_OUTPUT_NAME", "learn-pali")
            .env(
                "LEPTOS_SITE_ADDR",
                reserved.local_addr().unwrap().to_string(),
            )
            .output()
            .unwrap();
        assert_eq!(output.status.code(), Some(1));
        assert_eq!(output.stderr.len(), 0);
        let log: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
        assert_eq!(log["event.name"], "service.failed");
        assert_eq!(log["stage"], "origin_config");
    }
}
