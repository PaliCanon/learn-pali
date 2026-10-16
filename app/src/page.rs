//! The page-level browser calls the screens make.

/// Reloads the page (the webview too): the host's start and its read of
/// the progress run again. Nothing on the server.
pub(crate) fn reload_page() {
    #[cfg(feature = "browser")]
    if let Some(window) = web_sys::window() {
        let _ = window.location().reload();
    }
}
