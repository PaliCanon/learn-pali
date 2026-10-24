//! The passages asset, read (070): the file `build-deck passages` writes
//! (one row per DPD lemma: the line from the suttas, its blank, each
//! language's line with its translator, or the miss by name; an `info`
//! table with `schema` and `complete`). The site's server reads it instead
//! of asking the API five times per added word; the desktop app has its own
//! reader over the same columns.

use std::collections::BTreeMap;
use std::path::Path;

use learn_core::deck::{Blank, Example, Translation};
use rusqlite::{Connection, OpenFlags, OptionalExtension};

/// The schema this reader knows (`build-deck passages` writes it in `info`).
pub const SCHEMA: &str = "1";

/// Why an asset is not read.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AssetError {
    /// The file does not open as a database.
    Open(String),
    /// `info.schema` is not [`SCHEMA`].
    Schema(Option<String>),
    /// `info.complete` is not `true`: the build stopped.
    Incomplete,
    /// A query failed.
    Query(String),
}

impl std::fmt::Display for AssetError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            AssetError::Open(why) => write!(f, "the asset does not open: {why}"),
            AssetError::Schema(got) => write!(f, "schema {got:?}, this reader knows {SCHEMA:?}"),
            AssetError::Incomplete => write!(f, "the asset is not complete"),
            AssetError::Query(why) => write!(f, "the asset could not be read: {why}"),
        }
    }
}

impl std::error::Error for AssetError {}

/// An open, checked asset.
pub struct Asset {
    conn: Connection,
}

/// Why a lemma has no line, as the writer (`build-deck passages`) names it
/// and the readers (the site, the app) read it back: one vocabulary for
/// both sides (review 070, Deep).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MissName {
    /// The DPD source maps to no SuttaCentral uid.
    UnmappedSource,
    /// The API does not serve the text.
    TextNotServed,
    /// The text has no Pāli segments.
    NoPali,
    /// The example was not found in the text.
    NotMatched,
}

impl MissName {
    /// The name written in the asset's `miss` column.
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            MissName::UnmappedSource => "unmapped_source",
            MissName::TextNotServed => "text_not_served",
            MissName::NoPali => "no_pali",
            MissName::NotMatched => "not_matched",
        }
    }

    /// The name read back; `None` for a name this vocabulary does not have.
    #[must_use]
    pub fn parse(name: &str) -> Option<MissName> {
        match name {
            "unmapped_source" => Some(MissName::UnmappedSource),
            "text_not_served" => Some(MissName::TextNotServed),
            "no_pali" => Some(MissName::NoPali),
            "not_matched" => Some(MissName::NotMatched),
            _ => None,
        }
    }
}

/// The name the writer gives a fetch that failed at build time: not a
/// miss, a row to retry live (review 070: a transient frozen as permanent).
pub const RETRY: &str = "fetch";

/// A lemma's row.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Row {
    /// The line with at least its Pāli, as the deck shows it.
    Found(Example),
    /// No line, for a reason the deck knows.
    Miss(MissName),
    /// No answer to trust: the writer's fetch failed (`fetch`), or the name
    /// is not in this reader's vocabulary (the name is carried): the live
    /// method decides.
    Retry(String),
}

impl Asset {
    /// Opens `path` read-only and checks its `info`.
    ///
    /// # Errors
    /// The file does not open, is of another schema, or is not complete.
    pub fn open(path: &Path) -> Result<Asset, AssetError> {
        let conn = Connection::open_with_flags(path, OpenFlags::SQLITE_OPEN_READ_ONLY)
            .map_err(|e| AssetError::Open(e.to_string()))?;
        let info = |key: &str| -> Result<Option<String>, AssetError> {
            conn.query_row("SELECT value FROM info WHERE key = ?1", [key], |r| r.get(0))
                .optional()
                .map_err(|e| AssetError::Query(e.to_string()))
        };
        let schema = info("schema")?;
        if schema.as_deref() != Some(SCHEMA) {
            return Err(AssetError::Schema(schema));
        }
        if info("complete")?.as_deref() != Some("true") {
            return Err(AssetError::Incomplete);
        }
        Ok(Asset { conn })
    }

    /// The row of `lemma` (the DPD lemma with its homonym number); `None`
    /// when the asset has none (the entry has no example, or is newer than
    /// the asset).
    ///
    /// # Errors
    /// The query failed.
    pub fn row(&self, lemma: &str) -> Result<Option<Row>, AssetError> {
        let mut stmt = self
            .conn
            .prepare_cached(
                "SELECT text, source, sutta, blank_before, blank_answer, blank_after, \
                 en_text, en_translator, fr_text, fr_translator, vi_text, vi_translator, miss \
                 FROM passages WHERE lemma = ?1",
            )
            .map_err(|e| AssetError::Query(e.to_string()))?;
        stmt.query_row([lemma], |r| {
            let miss: Option<String> = r.get(12)?;
            if let Some(miss) = miss {
                return Ok(match MissName::parse(&miss) {
                    Some(name) => Row::Miss(name),
                    None => Row::Retry(miss),
                });
            }
            let blank = match (r.get(3)?, r.get(4)?, r.get(5)?) {
                (Some(before), Some(answer), Some(after)) => Some(Blank {
                    before,
                    answer,
                    after,
                }),
                _ => None,
            };
            let mut translations = BTreeMap::new();
            for (lang, at) in [("en", 6), ("fr", 8), ("vi", 10)] {
                let text: Option<String> = r.get(at)?;
                let translator: Option<String> = r.get(at + 1)?;
                if let (Some(text), Some(translator)) = (text, translator) {
                    translations.insert(lang.to_string(), Translation { text, translator });
                }
            }
            Ok(Row::Found(Example {
                text: r.get(0)?,
                source: r.get(1)?,
                sutta: r.get(2)?,
                blank,
                translations,
            }))
        })
        .optional()
        .map_err(|e| AssetError::Query(e.to_string()))
    }
}

/// Writes a small asset at `path` for tests: `dhamma 1.01` found with an
/// English and a French line, `vakkali` missed (`not_matched`), `kho` a
/// failed fetch, `ca` a name no reader knows.
///
/// # Panics
/// The file cannot be written.
#[doc(hidden)]
pub fn write_fixture(path: &Path) {
    let conn = Connection::open(path).expect("the fixture asset");
    conn.execute_batch(
        "CREATE TABLE passages (lemma TEXT PRIMARY KEY, text TEXT NOT NULL, source TEXT NOT NULL, sutta TEXT NOT NULL, blank_before TEXT, blank_answer TEXT, blank_after TEXT, en_text TEXT, en_translator TEXT, fr_text TEXT, fr_translator TEXT, vi_text TEXT, vi_translator TEXT, miss TEXT);
         CREATE TABLE info (key TEXT PRIMARY KEY, value TEXT NOT NULL);
         INSERT INTO info VALUES ('schema', '1');
         INSERT INTO info VALUES ('complete', 'true');
         INSERT INTO passages VALUES ('dhamma 1.01', 'natthi idha dhammo', 'SN22.87', 'Vakkalisutta', 'natthi idha ', 'dhammo', '', 'There is no nature here.', 'sujato', 'Il n''y a pas de nature ici.', 'christelle', NULL, NULL, NULL);
         INSERT INTO passages VALUES ('vakkali', 'alaṃ vakkali', 'SN22.87', 'Vakkalisutta', 'alaṃ ', 'vakkali', '', NULL, NULL, NULL, NULL, NULL, NULL, 'not_matched');
         INSERT INTO passages VALUES ('kho', 'kho pana', 'SN22.87', 'Vakkalisutta', '', 'kho', ' pana', NULL, NULL, NULL, NULL, NULL, NULL, 'fetch');
         INSERT INTO passages VALUES ('ca', 'ca pana', 'SN22.87', 'Vakkalisutta', '', 'ca', ' pana', NULL, NULL, NULL, NULL, NULL, NULL, 'something_new');",
    )
    .expect("the fixture rows");
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixture(name: &str) -> std::path::PathBuf {
        let path = std::env::temp_dir().join(format!("pc-asset-{name}-{}.db", std::process::id()));
        let _ = std::fs::remove_file(&path);
        write_fixture(&path);
        path
    }

    #[test]
    fn a_row_is_found_with_its_blank_and_lines_missed_by_name_or_absent() {
        let asset = Asset::open(&fixture("rows")).unwrap();
        let Some(Row::Found(example)) = asset.row("dhamma 1.01").unwrap() else {
            panic!("found")
        };
        assert_eq!(
            example.blank.as_ref().map(|b| b.answer.as_str()),
            Some("dhammo")
        );
        assert_eq!(example.translations["fr"].translator, "christelle");
        assert_eq!(example.translations.len(), 2, "no vi line");
        assert_eq!(
            asset.row("vakkali").unwrap(),
            Some(Row::Miss(MissName::NotMatched))
        );
        assert_eq!(
            asset.row("kho").unwrap(),
            Some(Row::Retry("fetch".into())),
            "a failed fetch is retried live"
        );
        assert_eq!(
            asset.row("ca").unwrap(),
            Some(Row::Retry("something_new".into())),
            "an unknown name is retried live"
        );
        assert_eq!(asset.row("nobody").unwrap(), None);
        for name in [
            MissName::UnmappedSource,
            MissName::TextNotServed,
            MissName::NoPali,
            MissName::NotMatched,
        ] {
            assert_eq!(MissName::parse(name.as_str()), Some(name));
        }
        assert_eq!(MissName::parse(RETRY), None);
    }

    #[test]
    fn another_schema_an_incomplete_build_or_no_file_is_refused() {
        let path = fixture("schema");
        let conn = Connection::open(&path).unwrap();
        conn.execute("UPDATE info SET value = '2' WHERE key = 'schema'", [])
            .unwrap();
        drop(conn);
        assert_eq!(
            Asset::open(&path).err(),
            Some(AssetError::Schema(Some("2".into())))
        );
        let path = fixture("partial");
        let conn = Connection::open(&path).unwrap();
        conn.execute("DELETE FROM info WHERE key = 'complete'", [])
            .unwrap();
        drop(conn);
        assert_eq!(Asset::open(&path).err(), Some(AssetError::Incomplete));
        let nowhere = std::env::temp_dir().join("pc-asset-nowhere.db");
        assert!(matches!(
            Asset::open(&nowhere),
            Err(AssetError::Open(_) | AssetError::Query(_))
        ));
    }
}
