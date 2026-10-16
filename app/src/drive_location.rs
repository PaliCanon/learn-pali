//! Where the progress lives in the learner's Google Drive (task 050): the
//! one module that says it, for every transport.
//!
//! First choice: the Drive's hidden app folder ([`FOLDER`],
//! `appDataFolder`), file [`FILE_NAME`], scope [`SCOPE`] (`drive.appdata`,
//! non-sensitive). If the site and the desktop app turn out not to see the
//! same file there (the user's check compares the file ids both log), this
//! module alone switches to a visible `PaliCanon/pc_learn.json` with scope
//! `drive.file`; nothing else changes.

/// The Drive folder of the file: the app's hidden folder.
pub const FOLDER: &str = "appDataFolder";

/// The progress file's name.
pub const FILE_NAME: &str = "pc_learn.json";

/// The OAuth scope that reaches [`FOLDER`].
pub const SCOPE: &str = "https://www.googleapis.com/auth/drive.appdata";

/// The multipart boundary of a creation; never in a progress's JSON.
const BOUNDARY: &str = "pc_learn_sync_050_boundary";

/// The URL listing the progress file in [`FOLDER`], the oldest first (two
/// devices that raced to create it read the same one), trashed ones
/// excluded.
pub fn list_url() -> String {
    format!(
        "https://www.googleapis.com/drive/v3/files?spaces={FOLDER}\
         &q=name%3D%27{FILE_NAME}%27%20and%20trashed%3Dfalse\
         &orderBy=createdTime&fields=files(id)"
    )
}

/// The URL reading the file `id`'s content.
pub fn read_url(id: &str) -> String {
    format!("https://www.googleapis.com/drive/v3/files/{id}?alt=media")
}

/// The URL replacing the file `id`'s content.
pub fn update_url(id: &str) -> String {
    format!("https://www.googleapis.com/upload/drive/v3/files/{id}?uploadType=media")
}

/// The URL creating the file, metadata and content in one request.
pub const CREATE_URL: &str =
    "https://www.googleapis.com/upload/drive/v3/files?uploadType=multipart&fields=id";

/// The creation's `Content-Type` and body: the file's metadata (its name,
/// in [`FOLDER`]) and `text`, in one multipart upload (never an empty
/// file).
pub fn create_body(text: &str) -> (String, String) {
    let metadata = serde_json::json!({ "name": FILE_NAME, "parents": [FOLDER] });
    let body = format!(
        "--{BOUNDARY}\r\nContent-Type: application/json; charset=UTF-8\r\n\r\n{metadata}\r\n\
         --{BOUNDARY}\r\nContent-Type: application/json\r\n\r\n{text}\r\n--{BOUNDARY}--"
    );
    (format!("multipart/related; boundary={BOUNDARY}"), body)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_location_is_the_hidden_app_folder() {
        assert_eq!(FOLDER, "appDataFolder");
        assert_eq!(FILE_NAME, "pc_learn.json");
        assert_eq!(SCOPE, "https://www.googleapis.com/auth/drive.appdata");
    }

    #[test]
    fn the_urls_name_the_app_folder_and_the_file() {
        let list = list_url();
        assert!(list.starts_with("https://www.googleapis.com/drive/v3/files?"));
        assert!(list.contains("spaces=appDataFolder"), "{list}");
        assert!(list.contains("name%3D%27pc_learn.json%27"), "{list}");
        assert!(list.contains("orderBy=createdTime"), "{list}");
        assert_eq!(
            read_url("f1"),
            "https://www.googleapis.com/drive/v3/files/f1?alt=media"
        );
        assert_eq!(
            update_url("f1"),
            "https://www.googleapis.com/upload/drive/v3/files/f1?uploadType=media"
        );
        assert!(CREATE_URL.starts_with("https://www.googleapis.com/upload/"));
    }

    #[test]
    fn a_creation_carries_the_name_the_folder_and_the_text() {
        let (content_type, body) = create_body(r#"{"version":1}"#);
        assert_eq!(
            content_type,
            format!("multipart/related; boundary={BOUNDARY}")
        );
        assert!(body.contains(r#""name":"pc_learn.json""#), "{body}");
        assert!(body.contains(r#""parents":["appDataFolder"]"#), "{body}");
        assert!(body.contains("\r\n\r\n{\"version\":1}\r\n"), "{body}");
        assert!(body.ends_with(&format!("--{BOUNDARY}--")), "{body}");
        assert_eq!(body.matches(&format!("--{BOUNDARY}\r\n")).count(), 2);
    }
}
