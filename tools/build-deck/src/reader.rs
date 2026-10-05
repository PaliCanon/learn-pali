//! Reading headwords from the DPD database.
//!
//! The database is opened read-only by the command and is never part of a
//! test: the tests here build the three tables they need in memory. Only
//! the columns the deck uses are read; a NULL column reads as an empty
//! string.

use std::fmt;

use learn_core::deck::Root;
use rusqlite::{Connection, OptionalExtension};

/// The raw columns of one `dpd_headwords` row, NULL read as `""`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RawTerm {
    /// `lemma_1`: the unique key (`sati 1.1`).
    pub lemma_1: String,
    /// `pos`: the part-of-speech code.
    pub pos: String,
    /// `meaning_1`: the English sense.
    pub meaning_1: String,
    /// `meaning_lit`: the literal meaning.
    pub meaning_lit: String,
    /// `construction`: how the word is built.
    pub construction: String,
    /// `root_key`: the key into `dpd_roots` (`√sar 2`).
    pub root_key: String,
    /// `example_1`: the example, with `<b>` around the headword.
    pub example_1: String,
    /// `source_1`: the text the example is quoted from.
    pub source_1: String,
    /// `sutta_1`: the sutta reference.
    pub sutta_1: String,
    /// `inflections_html`: the inflection table.
    pub inflections_html: String,
}

/// The database's own release and licence, for the deck's source line.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DbInfo {
    /// `dpd_release_version` (`v0.4.20260926`).
    pub release: String,
    /// `license` (`CC BY-NC-SA 4.0`).
    pub licence: String,
}

/// Why the database could not be read.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ReadError {
    /// No headword has this `lemma_1`.
    MissingKey(String),
    /// `db_info` has no row for this key.
    MissingInfo(String),
    /// SQLite refused (the message).
    Sql(String),
}

impl fmt::Display for ReadError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ReadError::MissingKey(key) => write!(f, "no headword '{key}' in the database"),
            ReadError::MissingInfo(key) => write!(f, "db_info has no '{key}' row"),
            ReadError::Sql(message) => write!(f, "database error: {message}"),
        }
    }
}

impl std::error::Error for ReadError {}

impl From<rusqlite::Error> for ReadError {
    fn from(error: rusqlite::Error) -> Self {
        ReadError::Sql(error.to_string())
    }
}

/// Reads the headword whose `lemma_1` is `key`.
///
/// # Errors
/// [`ReadError::MissingKey`] when there is none, [`ReadError::Sql`] when
/// SQLite fails.
pub fn read_term(conn: &Connection, key: &str) -> Result<RawTerm, ReadError> {
    let mut statement = conn.prepare(
        "SELECT lemma_1, pos, meaning_1, meaning_lit, construction, root_key,
                example_1, source_1, sutta_1, inflections_html
         FROM dpd_headwords WHERE lemma_1 = ?1",
    )?;
    let text = |row: &rusqlite::Row<'_>, at: usize| -> rusqlite::Result<String> {
        Ok(row.get::<_, Option<String>>(at)?.unwrap_or_default())
    };
    let found = statement
        .query_row([key], |row| {
            Ok(RawTerm {
                lemma_1: text(row, 0)?,
                pos: text(row, 1)?,
                meaning_1: text(row, 2)?,
                meaning_lit: text(row, 3)?,
                construction: text(row, 4)?,
                root_key: text(row, 5)?,
                example_1: text(row, 6)?,
                source_1: text(row, 7)?,
                sutta_1: text(row, 8)?,
                inflections_html: text(row, 9)?,
            })
        })
        .optional()?;
    found.ok_or_else(|| ReadError::MissingKey(key.to_string()))
}

/// Reads the root `root_key` (`√sar 2`) and its meaning; `None` when
/// `dpd_roots` has no such root.
///
/// # Errors
/// [`ReadError::Sql`] when SQLite fails.
pub fn read_root(conn: &Connection, root_key: &str) -> Result<Option<Root>, ReadError> {
    let meaning: Option<Option<String>> = conn
        .query_row(
            "SELECT root_meaning FROM dpd_roots WHERE root = ?1",
            [root_key],
            |row| row.get(0),
        )
        .optional()?;
    Ok(meaning.map(|meaning| Root {
        key: root_key.to_string(),
        meaning: meaning.unwrap_or_default(),
    }))
}

/// Reads the release and the licence from the key/value table `db_info`.
///
/// # Errors
/// [`ReadError::MissingInfo`] when either row is absent, [`ReadError::Sql`]
/// when SQLite fails.
pub fn read_db_info(conn: &Connection) -> Result<DbInfo, ReadError> {
    let info = |key: &str| -> Result<String, ReadError> {
        let value: Option<String> = conn
            .query_row("SELECT value FROM db_info WHERE key = ?1", [key], |row| {
                row.get(0)
            })
            .optional()?;
        value.ok_or_else(|| ReadError::MissingInfo(key.to_string()))
    };
    Ok(DbInfo {
        release: info("dpd_release_version")?,
        licence: info("license")?,
    })
}

/// An in-memory database shaped like the DPD's, for the tests of this
/// crate.
#[cfg(test)]
pub(crate) mod testdb {
    use rusqlite::Connection;

    /// The three tables with a handful of rows, in memory.
    pub(crate) fn connection() -> Connection {
        let conn = Connection::open_in_memory().unwrap();
        populate(&conn);
        conn
    }

    /// The inflection table of `sati 1.1`: a plain cell, a grey-only cell
    /// and a padding cell.
    const TABLE: &str = "<p class='heading'>x</p><table class='inflection'><tr>\
        <td title='fem nom sg'>sat<b>i</b></td>\
        <td title='fem loc pl'><span class='gray'>sat<b>isu</b></span></td>\
        <td title=''></td></tr></table>";

    /// Creates and fills the three tables of `conn`.
    pub(crate) fn populate(conn: &Connection) {
        conn.execute_batch(
            "CREATE TABLE dpd_headwords (id INTEGER PRIMARY KEY, lemma_1 TEXT, pos TEXT,
               meaning_1 TEXT, meaning_lit TEXT, construction TEXT, root_key TEXT,
               example_1 TEXT, source_1 TEXT, sutta_1 TEXT, inflections_html TEXT, ebt_count INT);
             CREATE TABLE dpd_roots (root TEXT PRIMARY KEY, root_meaning TEXT);
             CREATE TABLE db_info (id INTEGER PRIMARY KEY, key TEXT, value TEXT);
             INSERT INTO dpd_roots VALUES ('√sar 2', 'remember');
             INSERT INTO db_info (key, value) VALUES
               ('dpd_release_version', 'v0.4.test'), ('license', 'CC BY-NC-SA 4.0');",
        )
        .unwrap();
        for row in &rows() {
            insert(conn, row);
        }
    }

    /// The headwords: `sati 1.1` (marked example, forms, root), `kho`
    /// (indeclinable), `lone 1` (an example with no mark and no lemma),
    /// `badtable 1` (no table), `nomeaning 1` (empty meaning).
    #[rustfmt::skip]
    fn rows() -> [[&'static str; 10]; 5] {
        [
            ["sati 1.1", "fem", "memory; remembering", "remembering", "√sar + ti", "√sar 2",
             "atthi dhammā <b>satiyā</b> sacchikaraṇīyā", "AN4.189", "sacchikaraṇīyasuttaṃ", TABLE],
            ["kho", "ind", "indeed", "", "", "", "evaṃ <b>kho</b> bhagavā", "MN 1", "mn1", ""],
            ["lone 1", "masc", "alone", "", "", "", "a solitary man walks", "MN 2", "mn2", ""],
            ["badtable 1", "masc", "bad", "", "", "", "", "", "", "<p>no table</p>"],
            ["nomeaning 1", "masc", "", "", "", "", "", "", "", ""],
        ]
    }

    /// Inserts one headword row (the ten columns the reader selects).
    fn insert(conn: &Connection, row: &[&str; 10]) {
        conn.execute(
            "INSERT INTO dpd_headwords (lemma_1, pos, meaning_1, meaning_lit, construction,
               root_key, example_1, source_1, sutta_1, inflections_html)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)",
            rusqlite::params_from_iter(row),
        )
        .unwrap();
    }
}

#[cfg(test)]
mod tests {
    use super::testdb::connection;
    use super::*;

    #[test]
    fn a_headword_is_read_with_its_columns() {
        let raw = read_term(&connection(), "sati 1.1").unwrap();
        assert_eq!(raw.lemma_1, "sati 1.1");
        assert_eq!(raw.pos, "fem");
        assert_eq!(raw.meaning_1, "memory; remembering");
        assert_eq!(raw.meaning_lit, "remembering");
        assert_eq!(raw.construction, "√sar + ti");
        assert_eq!(raw.root_key, "√sar 2");
        assert!(raw.example_1.contains("<b>satiyā</b>"));
        assert_eq!(raw.source_1, "AN4.189");
        assert_eq!(raw.sutta_1, "sacchikaraṇīyasuttaṃ");
        assert!(raw.inflections_html.starts_with("<p class='heading'>"));
    }

    #[test]
    fn empty_columns_read_as_empty_strings() {
        let raw = read_term(&connection(), "kho").unwrap();
        assert_eq!(raw.meaning_lit, "");
        assert_eq!(raw.inflections_html, "");
    }

    #[test]
    fn null_columns_read_as_empty_strings() {
        let conn = connection();
        conn.execute(
            "INSERT INTO dpd_headwords (lemma_1, pos, meaning_1) VALUES ('nulls 1', 'ind', 'x')",
            [],
        )
        .unwrap();
        let raw = read_term(&conn, "nulls 1").unwrap();
        assert_eq!(raw.example_1, "");
        assert_eq!(raw.root_key, "");
    }

    #[test]
    fn a_key_that_does_not_exist_is_a_missing_key() {
        assert_eq!(
            read_term(&connection(), "nope 9"),
            Err(ReadError::MissingKey("nope 9".to_string()))
        );
    }

    #[test]
    fn a_root_is_read_with_its_meaning() {
        let root = read_root(&connection(), "√sar 2").unwrap().unwrap();
        assert_eq!(root.key, "√sar 2");
        assert_eq!(root.meaning, "remember");
    }

    #[test]
    fn an_unknown_root_is_none() {
        assert_eq!(read_root(&connection(), "√zzz"), Ok(None));
    }

    #[test]
    fn the_db_info_gives_release_and_licence() {
        assert_eq!(
            read_db_info(&connection()).unwrap(),
            DbInfo {
                release: "v0.4.test".to_string(),
                licence: "CC BY-NC-SA 4.0".to_string()
            }
        );
    }

    #[test]
    fn a_db_info_without_the_licence_is_an_error() {
        let conn = connection();
        conn.execute("DELETE FROM db_info WHERE key = 'license'", [])
            .unwrap();
        assert_eq!(
            read_db_info(&conn),
            Err(ReadError::MissingInfo("license".to_string()))
        );
    }

    #[test]
    fn a_missing_table_is_a_sql_error() {
        let conn = Connection::open_in_memory().unwrap();
        assert!(matches!(read_term(&conn, "kho"), Err(ReadError::Sql(_))));
        assert!(matches!(read_root(&conn, "√sar"), Err(ReadError::Sql(_))));
        assert!(matches!(read_db_info(&conn), Err(ReadError::Sql(_))));
    }

    #[test]
    fn each_error_says_what_is_wrong() {
        assert!(
            ReadError::MissingKey("kho".into())
                .to_string()
                .contains("kho")
        );
        assert!(
            ReadError::MissingInfo("license".into())
                .to_string()
                .contains("license")
        );
        assert!(ReadError::Sql("boom".into()).to_string().contains("boom"));
    }
}
