//! The site's local console sink. Nothing is sent over the network.

use std::io::{self, Write};

use tracing_subscriber::{Layer, filter::filter_fn, prelude::*};

use super::Identity;

pub(super) fn identity() -> Identity {
    thread_local! {
        static IDENTITY: Identity = Identity {
            environment: leptos::prelude::document()
                .query_selector(&format!(r#"meta[name="{}"]"#, super::ENV_META))
                .ok().flatten().and_then(|meta| meta.get_attribute("content"))
                .unwrap_or_else(|| "unknown".into()),
            version: crate::release::ReleaseInfo::from_document().env,
            commit: crate::release::ReleaseInfo::from_document().sha,
            runtime: "browser",
        };
    }
    IDENTITY.with(Clone::clone)
}

/// A clock using the browser API rather than wasm's unavailable `SystemTime`.
struct Clock;

impl tracing_subscriber::fmt::time::FormatTime for Clock {
    fn format_time(
        &self,
        writer: &mut tracing_subscriber::fmt::format::Writer<'_>,
    ) -> std::fmt::Result {
        writer.write_str(
            &js_sys::Date::new_0()
                .to_iso_string()
                .as_string()
                .unwrap_or_default(),
        )
    }
}

struct Console {
    bytes: Vec<u8>,
    level: tracing::Level,
}

struct ConsoleWriter;

impl<'a> tracing_subscriber::fmt::MakeWriter<'a> for ConsoleWriter {
    type Writer = Console;

    fn make_writer(&'a self) -> Console {
        Console {
            bytes: Vec::new(),
            level: tracing::Level::INFO,
        }
    }

    fn make_writer_for(&'a self, metadata: &tracing::Metadata<'_>) -> Console {
        Console {
            bytes: Vec::new(),
            level: *metadata.level(),
        }
    }
}

impl Write for Console {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        self.bytes.extend_from_slice(bytes);
        Ok(bytes.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

impl Drop for Console {
    fn drop(&mut self) {
        if self.bytes.is_empty() {
            return;
        }
        let line = String::from_utf8_lossy(&self.bytes);
        let value = wasm_bindgen::JsValue::from_str(line.trim_end());
        match self.level {
            tracing::Level::ERROR => web_sys::console::error_1(&value),
            tracing::Level::WARN => web_sys::console::warn_1(&value),
            _ => web_sys::console::log_1(&value),
        }
    }
}

pub(super) fn init() {
    let logs = tracing_subscriber::fmt::layer()
        .json()
        .flatten_event(true)
        .with_span_list(false)
        .with_current_span(false)
        .with_timer(Clock)
        .with_writer(ConsoleWriter)
        .with_filter(filter_fn(|metadata| {
            metadata.target() == "learn" && *metadata.level() <= tracing::Level::INFO
        }));
    if tracing_subscriber::registry().with(logs).try_init().is_ok() {
        std::panic::set_hook(Box::new(super::panic));
        super::event!(INFO, "browser.started");
    }
}
