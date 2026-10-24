//! The passages asset (068): every dictionary entry's line from the suttas,
//! built once from the API's corpus databases, locally, by the deck's own
//! method. The site's server and the app read it instead of calling the
//! API five times per added word.
//!
//! [`CorpusFetch`] implements `learn_passage`'s [`Fetch`] over the corpus
//! SQLite files (pāli + en, fr, vi), answering the API's own JSON shapes
//! for `/v1/sutta/{uid}` (the texts) and `/v1/sutta/{uid}/{lang}` (the
//! segments), so [`translate_example`] runs unchanged. [`build`] walks the
//! dictionary's entries whose source maps to a served uid, blanks the
//! marked word as the deck does, and writes one row per lemma: the line,
//! its blank, each language's line with its translator, or the miss by
//! name: a row whose `miss` is set has no language lines, and a lemma with
//! no example has no row (the reader answers `no_example`). An `info` table
//! carries the schema version, the licence (CC BY-NC-SA 4.0, the DPD's),
//! the counts and `complete`, written last.

use std::collections::BTreeMap;
use std::path::Path;

use learn_core::deck::Example;
use learn_passage::blank::blank_example;
use learn_passage::fetch::{Fetch, FetchError};
use learn_passage::translate::{Why, translate_example};
use rusqlite::{Connection, OpenFlags, OptionalExtension, params};

/// The API's name for the Pāli root text's translator.
pub const ROOT_TRANSLATOR: &str = "ms";
/// The site's languages, in the order the asset keeps them.
pub const LANGS: [&str; 3] = ["en", "fr", "vi"];

/// One corpus database: its language and its connection.
struct Corpus {
    lang: &'static str,
    conn: Connection,
}

/// `learn_passage`'s [`Fetch`] over the corpus databases.
pub struct CorpusFetch {
    corpora: Vec<Corpus>,
}

impl CorpusFetch {
    /// Opens the corpus files read-only: the pāli + en one first (it holds
    /// the root text), then fr and vi when given.
    ///
    /// # Errors
    /// A file that cannot be opened.
    pub fn open(en: &Path, fr: Option<&Path>, vi: Option<&Path>) -> Result<CorpusFetch, String> {
        let open = |lang: &'static str, path: &Path| -> Result<Corpus, String> {
            let conn = Connection::open_with_flags(path, OpenFlags::SQLITE_OPEN_READ_ONLY)
                .map_err(|e| format!("cannot open {}: {e}", path.display()))?;
            Ok(Corpus { lang, conn })
        };
        let mut corpora = vec![open("en", en)?];
        if let Some(fr) = fr {
            corpora.push(open("fr", fr)?);
        }
        if let Some(vi) = vi {
            corpora.push(open("vi", vi)?);
        }
        Ok(CorpusFetch { corpora })
    }

    /// A fetch over connections already open (tests: in-memory corpora).
    #[must_use]
    pub fn from_connections(
        en: Connection,
        others: Vec<(&'static str, Connection)>,
    ) -> CorpusFetch {
        let mut corpora = vec![Corpus {
            lang: "en",
            conn: en,
        }];
        corpora.extend(others.into_iter().map(|(lang, conn)| Corpus { lang, conn }));
        CorpusFetch { corpora }
    }

    fn sutta_id(conn: &Connection, uid: &str) -> rusqlite::Result<Option<String>> {
        conn.query_row("SELECT id FROM suttas WHERE sutta_uid = ?1", [uid], |r| {
            r.get::<_, String>(0)
        })
        .map(Some)
        .or_else(|e| match e {
            rusqlite::Error::QueryReturnedNoRows => Ok(None),
            e => Err(e),
        })
    }

    /// `/v1/sutta/{uid}`: the texts the sutta has, as the API lists them.
    fn info(&self, uid: &str) -> rusqlite::Result<Option<String>> {
        let mut texts: Vec<(String, String)> = Vec::new();
        let mut known = false;
        for corpus in &self.corpora {
            let Some(id) = Self::sutta_id(&corpus.conn, uid)? else {
                continue;
            };
            known = true;
            if corpus.lang == "en" {
                let has_root: bool = corpus.conn.query_row(
                    "SELECT EXISTS(SELECT 1 FROM segments WHERE sutta_id = ?1 AND trim(coalesce(pali_content, '')) <> '')",
                    [&id],
                    |r| r.get(0),
                )?;
                if has_root {
                    texts.push(("pli".to_string(), ROOT_TRANSLATOR.to_string()));
                }
            }
            let mut stmt = corpus.conn.prepare(
                "SELECT DISTINCT translator FROM segments WHERE sutta_id = ?1 AND trim(coalesce(content, '')) <> '' ORDER BY translator",
            )?;
            for translator in stmt.query_map([&id], |r| r.get::<_, String>(0))? {
                texts.push((corpus.lang.to_string(), translator?));
            }
        }
        if !known {
            return Ok(None);
        }
        let list: Vec<String> = texts
            .iter()
            .map(|(lang, tr)| {
                format!(
                    r#"{{"lang":{},"translator":{}}}"#,
                    json_str(lang),
                    json_str(tr)
                )
            })
            .collect();
        Ok(Some(format!(r#"{{"texts":[{}]}}"#, list.join(","))))
    }

    /// `/v1/sutta/{uid}/{lang}`: the segments of the language's first text.
    fn text(&self, uid: &str, lang: &str) -> rusqlite::Result<Option<String>> {
        let (corpus, column) = if lang == "pli" {
            (self.corpora.iter().find(|c| c.lang == "en"), "pali_content")
        } else {
            (self.corpora.iter().find(|c| c.lang == lang), "content")
        };
        let Some(corpus) = corpus else {
            return Ok(None);
        };
        let Some(id) = Self::sutta_id(&corpus.conn, uid)? else {
            return Ok(None);
        };
        let translator: Option<String> = if lang == "pli" {
            Some(ROOT_TRANSLATOR.to_string())
        } else {
            corpus
                .conn
                .query_row(
                    "SELECT translator FROM segments WHERE sutta_id = ?1 AND trim(coalesce(content, '')) <> '' ORDER BY translator LIMIT 1",
                    [&id],
                    |r| r.get(0),
                )
                .ok()
        };
        let Some(translator) = translator else {
            return Ok(None);
        };
        let sql = format!(
            "SELECT segment_key, {column} FROM segments WHERE sutta_id = ?1 AND trim(coalesce({column}, '')) <> '' AND (?2 = '{ROOT_TRANSLATOR}' OR translator = ?2) ORDER BY position"
        );
        let mut stmt = corpus.conn.prepare(&sql)?;
        let segments: Vec<String> = stmt
            .query_map(params![&id, &translator], |r| {
                let key: String = r.get(0)?;
                let text: String = r.get(1)?;
                Ok(format!(
                    r#"{{"key":{},"text":{}}}"#,
                    json_str(&key),
                    json_str(text.trim())
                ))
            })?
            .collect::<Result<_, _>>()?;
        if segments.is_empty() {
            return Ok(None);
        }
        Ok(Some(format!(
            r#"{{"translator":{},"segments":[{}]}}"#,
            json_str(&translator),
            segments.join(",")
        )))
    }
}

fn json_str(s: &str) -> String {
    serde_json::to_string(s).unwrap_or_else(|_| "\"\"".to_string())
}

impl Fetch for CorpusFetch {
    fn get(&mut self, path: &str) -> Result<Option<String>, FetchError> {
        let rest = path
            .strip_prefix("/v1/sutta/")
            .ok_or_else(|| FetchError::Status(path.to_string(), 404))?;
        let result = match rest.split_once('/') {
            None => self.info(rest),
            Some((uid, lang)) => self.text(uid, lang),
        };
        result.map_err(|e| FetchError::Transport(path.to_string(), e.to_string()))
    }
}

/// One dictionary entry the pipeline reads.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Entry {
    /// The DPD lemma with its homonym number: the asset's key.
    pub lemma: String,
    /// The bare word, as marked in the example.
    pub headword: String,
    /// The DPD example, with its `<b>…</b>` mark.
    pub example: String,
    /// The DPD source (`SN22.87`).
    pub source: String,
    /// The sutta's name.
    pub sutta: String,
}

/// The entries of the dictionary asset that have an example and a source.
///
/// # Errors
/// The database cannot be read.
pub fn read_entries(dictionary: &Connection) -> rusqlite::Result<Vec<Entry>> {
    let mut stmt = dictionary.prepare(
        "SELECT lemma, headword, example, source, coalesce(sutta, '') FROM entries WHERE example IS NOT NULL AND example <> '' AND source IS NOT NULL AND source <> '' ORDER BY lemma",
    )?;
    let rows = stmt.query_map([], |r| {
        Ok(Entry {
            lemma: r.get(0)?,
            headword: r.get(1)?,
            example: r.get(2)?,
            source: r.get(3)?,
            sutta: r.get(4)?,
        })
    })?;
    rows.collect()
}

/// What the pipeline counted.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct Counts {
    /// Entries read.
    pub entries: usize,
    /// Passages found, with at least the Pāli line.
    pub found: usize,
    /// Misses, by reason name.
    pub misses: BTreeMap<String, usize>,
    /// Lines per language.
    pub lines: BTreeMap<String, usize>,
}

/// The reason's name for the asset and the counts.
fn miss_name(why: &Why) -> &'static str {
    use learn_passage::asset::{MissName, RETRY};
    match why {
        Why::UnmappedSource(_) => MissName::UnmappedSource.as_str(),
        Why::NotServed(_) => MissName::TextNotServed.as_str(),
        Why::NoPali(_) => MissName::NoPali.as_str(),
        Why::NotMatched(_) => MissName::NotMatched.as_str(),
        // Not a miss: the reader retries it live (review 070).
        Why::Fetch(_) => RETRY,
    }
}

/// Builds the asset at `out` from `entries` and `fetch`.
///
/// # Errors
/// The output exists, or SQLite refuses.
pub fn build(
    entries: &[Entry],
    fetch: &mut dyn Fetch,
    out: &Connection,
) -> rusqlite::Result<Counts> {
    out.execute_batch(
        "CREATE TABLE passages (lemma TEXT PRIMARY KEY, text TEXT NOT NULL, source TEXT NOT NULL, sutta TEXT NOT NULL, blank_before TEXT, blank_answer TEXT, blank_after TEXT, en_text TEXT, en_translator TEXT, fr_text TEXT, fr_translator TEXT, vi_text TEXT, vi_translator TEXT, miss TEXT);
         CREATE TABLE info (key TEXT PRIMARY KEY, value TEXT NOT NULL);",
    )?;
    let mut counts = Counts::default();
    let mut insert = out.prepare(
        "INSERT INTO passages (lemma, text, source, sutta, blank_before, blank_answer, blank_after, en_text, en_translator, fr_text, fr_translator, vi_text, vi_translator, miss) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14)",
    )?;
    for entry in entries {
        counts.entries += 1;
        let blanked = blank_example(&entry.example, &entry.headword, &[]);
        let example = Example {
            text: blanked.text,
            source: entry.source.clone(),
            sutta: entry.sutta.clone(),
            blank: blanked.blank,
            translations: BTreeMap::new(),
        };
        let (translations, miss) = match translate_example(&example, fetch) {
            Ok(found) => (found.translations, None),
            Err(why) => (BTreeMap::new(), Some(miss_name(&why))),
        };
        if let Some(name) = miss {
            *counts.misses.entry(name.to_string()).or_default() += 1;
        } else {
            counts.found += 1;
        }
        for lang in translations.keys() {
            *counts.lines.entry(lang.clone()).or_default() += 1;
        }
        write_row(&mut insert, entry, &example, &translations, miss)?;
    }
    drop(insert);
    write_info(out, &counts)?;
    Ok(counts)
}

/// One passage row: the line, its blank, each language's text and
/// translator, or the miss.
fn write_row(
    insert: &mut rusqlite::Statement<'_>,
    entry: &Entry,
    example: &Example,
    translations: &BTreeMap<String, learn_core::deck::Translation>,
    miss: Option<&str>,
) -> rusqlite::Result<()> {
    let line = |lang: &str| {
        translations
            .get(lang)
            .map(|t| (t.text.clone(), t.translator.clone()))
    };
    let (en, fr, vi) = (line("en"), line("fr"), line("vi"));
    let blank = example.blank.as_ref();
    insert.execute(params![
        entry.lemma,
        example.text,
        example.source,
        example.sutta,
        blank.map(|b| b.before.clone()),
        blank.map(|b| b.answer.clone()),
        blank.map(|b| b.after.clone()),
        en.as_ref().map(|t| t.0.clone()),
        en.as_ref().map(|t| t.1.clone()),
        fr.as_ref().map(|t| t.0.clone()),
        fr.as_ref().map(|t| t.1.clone()),
        vi.as_ref().map(|t| t.0.clone()),
        vi.as_ref().map(|t| t.1.clone()),
        miss,
    ])?;
    Ok(())
}

/// The `info` table: the licence and the counts.
/// The asset's schema version: bumped when [`COLUMNS`] changes, so a reader
/// (the app's `dictionary_passage`) refuses an asset it does not know.
pub const SCHEMA: &str = "1";

/// The `passages` columns, in order: what [`SCHEMA`] stands for.
pub const COLUMNS: [&str; 14] = [
    "lemma",
    "text",
    "source",
    "sutta",
    "blank_before",
    "blank_answer",
    "blank_after",
    "en_text",
    "en_translator",
    "fr_text",
    "fr_translator",
    "vi_text",
    "vi_translator",
    "miss",
];

fn write_info(out: &Connection, counts: &Counts) -> rusqlite::Result<()> {
    let mut info = out.prepare("INSERT INTO info (key, value) VALUES (?1, ?2)")?;
    info.execute(params!["schema", SCHEMA])?;
    info.execute(params!["licence", "The examples are the Digital Pāḷi Dictionary's (CC BY-NC-SA 4.0); the lines are SuttaCentral's texts and translations, each under its translator's licence."])?;
    info.execute(params!["entries", counts.entries.to_string()])?;
    info.execute(params!["found", counts.found.to_string()])?;
    for (name, n) in &counts.misses {
        info.execute(params![format!("miss_{name}"), n.to_string()])?;
    }
    for (lang, n) in &counts.lines {
        info.execute(params![format!("lines_{lang}"), n.to_string()])?;
    }
    // Written last: an asset without it stopped before the end.
    info.execute(params!["complete", "true"])?;
    Ok(())
}

/// Reads an asset back as a reader does (review 068, Deep: nothing pinned
/// the schema): its schema must be [`SCHEMA`] and it must be complete.
///
/// # Errors
/// The asset is not one this version reads, or it is not complete.
pub fn check_asset(conn: &Connection) -> Result<Counts, String> {
    let info = |key: &str| -> Option<String> {
        conn.query_row("SELECT value FROM info WHERE key = ?1", [key], |r| r.get(0))
            .ok()
    };
    match info("schema").as_deref() {
        Some(SCHEMA) => {}
        other => return Err(format!("schema {other:?}, expected {SCHEMA:?}")),
    }
    if info("complete").as_deref() != Some("true") {
        return Err("the asset is not complete".to_string());
    }
    let columns: Vec<String> = conn
        .prepare("PRAGMA table_info(passages)")
        .and_then(|mut s| {
            s.query_map([], |r| r.get::<_, String>(1))?
                .collect::<rusqlite::Result<_>>()
        })
        .map_err(|e| e.to_string())?;
    if columns != COLUMNS {
        return Err(format!("columns {columns:?}, expected {COLUMNS:?}"));
    }
    // One found row read through every column a reader uses.
    conn.query_row(
        "SELECT text, source, sutta, blank_before, blank_answer, blank_after, en_text, en_translator, fr_text, fr_translator, vi_text, vi_translator FROM passages WHERE miss IS NULL LIMIT 1",
        [],
        |r| {
            for i in 0..12 {
                r.get::<_, Option<String>>(i)?;
            }
            Ok(())
        },
    )
    .optional()
    .map_err(|e| e.to_string())?;
    let count = |sql: &str| -> Result<usize, String> {
        conn.query_row(sql, [], |r| r.get::<_, i64>(0))
            .map_err(|e| e.to_string())
            .and_then(|n| usize::try_from(n).map_err(|e| e.to_string()))
    };
    Ok(Counts {
        entries: count("SELECT count(*) FROM passages")?,
        found: count("SELECT count(*) FROM passages WHERE miss IS NULL")?,
        misses: BTreeMap::new(),
        lines: BTreeMap::new(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A passage row as the asset test reads it.
    type Row = (
        String,
        Option<String>,
        Option<String>,
        Option<String>,
        Option<String>,
        Option<String>,
    );

    fn corpus(lang: &str, uid: &str, segments: &[(&str, &str, &str)]) -> Connection {
        let conn = Connection::open_in_memory().unwrap();
        conn.execute_batch(
            "CREATE TABLE suttas (id TEXT, sutta_uid TEXT, display_id TEXT, title TEXT, pali_title TEXT, sort_order INTEGER);
             CREATE TABLE segments (sutta_id TEXT, segment_key TEXT, pali_content TEXT, content TEXT, translator TEXT, position INTEGER);",
        )
        .unwrap();
        conn.execute(
            "INSERT INTO suttas VALUES ('s1', ?1, 'SN 22.87', 'With Vakkali', 'Vakkalisutta', 1)",
            [uid],
        )
        .unwrap();
        for (position, (key, pali, text)) in (0_i64..).zip(segments.iter()) {
            let translator = if lang == "en" { "sujato" } else { "christelle" };
            conn.execute(
                "INSERT INTO segments VALUES ('s1', ?1, ?2, ?3, ?4, ?5)",
                params![key, pali, text, translator, position],
            )
            .unwrap();
        }
        conn
    }

    fn fetch() -> CorpusFetch {
        let en = corpus(
            "en",
            "sn22.87",
            &[
                (
                    "sn22.87:1.1",
                    "Alaṃ, vakkali, kiṃ te iminā pūtikāyena diṭṭhena?",
                    "Enough, Vakkali! Why would you want to see this rotting body?",
                ),
                (
                    "sn22.87:1.2",
                    "Yo kho, vakkali, dhammaṃ passati so maṃ passati.",
                    "One who sees the teaching sees me.",
                ),
            ],
        );
        let fr = corpus(
            "fr",
            "sn22.87",
            &[(
                "sn22.87:1.1",
                "",
                "Assez, Vakkali ! Pourquoi vouloir voir ce corps pourrissant ?",
            )],
        );
        CorpusFetch::from_connections(en, vec![("fr", fr)])
    }

    fn entry(lemma: &str, example: &str, source: &str) -> Entry {
        Entry {
            lemma: lemma.into(),
            headword: lemma.split(' ').next().unwrap().into(),
            example: example.into(),
            source: source.into(),
            sutta: "Vakkalisutta".into(),
        }
    }

    #[test]
    fn the_fetch_answers_the_apis_shapes_from_sqlite() {
        let mut f = fetch();
        let info = f.get("/v1/sutta/sn22.87").unwrap().unwrap();
        assert!(
            info.contains(r#"{"lang":"pli","translator":"ms"}"#),
            "{info}"
        );
        assert!(
            info.contains(r#"{"lang":"en","translator":"sujato"}"#),
            "{info}"
        );
        assert!(
            info.contains(r#"{"lang":"fr","translator":"christelle"}"#),
            "{info}"
        );
        let pli = f.get("/v1/sutta/sn22.87/pli").unwrap().unwrap();
        assert!(
            pli.contains(r#""translator":"ms""#) && pli.contains("sn22.87:1.2"),
            "{pli}"
        );
        let fr = f.get("/v1/sutta/sn22.87/fr").unwrap().unwrap();
        assert!(fr.contains("Assez, Vakkali"), "{fr}");
        assert_eq!(
            f.get("/v1/sutta/sn22.87/vi").unwrap(),
            None,
            "no vi corpus: no text"
        );
        assert_eq!(f.get("/v1/sutta/mn1").unwrap(), None, "an unknown sutta");
        assert!(matches!(
            f.get("/v1/other").unwrap_err(),
            FetchError::Status(_, 404)
        ));
    }

    #[test]
    #[allow(clippy::too_many_lines)]
    fn the_asset_holds_the_line_its_blank_and_each_languages_text_or_the_miss() {
        let mut f = fetch();
        let out = Connection::open_in_memory().unwrap();
        let entries = vec![
            entry(
                "vakkali",
                "alaṃ, <b>vakkali</b>, kiṃ te iminā pūtikāyena diṭṭhena",
                "SN22.87",
            ),
            entry("dhamma 1.01", "natthi idha <b>dhammo</b>", "SN22.87"),
            entry("sati 1.1", "<b>sati</b> paccupaṭṭhitā", "KHP5"),
            entry("mettā", "<b>mettā</b> ca sabbalokasmi", "SN1.1"),
        ];
        let counts = build(&entries, &mut f, &out).unwrap();
        assert_eq!(counts.entries, 4);
        assert_eq!(counts.found, 1);
        assert_eq!(counts.misses["not_matched"], 1);
        assert_eq!(counts.misses["unmapped_source"], 1);
        assert_eq!(counts.misses["text_not_served"], 1);
        assert_eq!(counts.lines["en"], 1);
        assert_eq!(counts.lines["fr"], 1);
        let row: Row = out
            .query_row(
                "SELECT text, blank_answer, en_text, en_translator, fr_text, miss FROM passages WHERE lemma = 'vakkali'",
                [],
                |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?, r.get(5)?)),
            )
            .unwrap();
        assert_eq!(
            row.0, "alaṃ, vakkali, kiṃ te iminā pūtikāyena diṭṭhena",
            "the markup stripped"
        );
        assert_eq!(row.1.as_deref(), Some("vakkali"), "the marked word blanked");
        assert_eq!(
            row.2.as_deref(),
            Some("Enough, Vakkali! Why would you want to see this rotting body?")
        );
        assert_eq!(row.3.as_deref(), Some("sujato"));
        assert!(row.4.as_deref().unwrap().starts_with("Assez"), "{row:?}");
        assert_eq!(row.5, None);
        let miss: Option<String> = out
            .query_row(
                "SELECT miss FROM passages WHERE lemma = 'sati 1.1'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(miss.as_deref(), Some("unmapped_source"));
        let found: String = out
            .query_row("SELECT value FROM info WHERE key = 'found'", [], |r| {
                r.get(0)
            })
            .unwrap();
        assert_eq!(found, "1");
        let licence: String = out
            .query_row("SELECT value FROM info WHERE key = 'licence'", [], |r| {
                r.get(0)
            })
            .unwrap();
        assert!(licence.contains("CC BY-NC-SA 4.0"));
    }

    #[test]
    fn the_asset_reads_back_with_its_schema_and_the_rows_a_reader_expects() {
        let mut f = fetch();
        let out = Connection::open_in_memory().unwrap();
        let entries = vec![
            entry("vakkali", "alaṃ, <b>vakkali</b>, kiṃ te", "SN22.87"),
            entry("sati 1.1", "<b>sati</b> paccupaṭṭhitā", "KHP5"),
        ];
        build(&entries, &mut f, &out).unwrap();
        let counts = check_asset(&out).unwrap();
        assert_eq!((counts.entries, counts.found), (2, 1));
        let (en, miss): (Option<String>, Option<String>) = out
            .query_row(
                "SELECT en_text, miss FROM passages WHERE lemma = 'sati 1.1'",
                [],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .unwrap();
        assert_eq!(
            (en, miss.as_deref()),
            (None, Some("unmapped_source")),
            "a miss has no line"
        );
        let absent: i64 = out
            .query_row(
                "SELECT count(*) FROM passages WHERE lemma = 'nobody'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(absent, 0, "no example: no row");
        out.execute("UPDATE info SET value = '0' WHERE key = 'schema'", [])
            .unwrap();
        assert!(check_asset(&out).unwrap_err().contains("schema"));
        out.execute("UPDATE info SET value = '1' WHERE key = 'schema'", [])
            .unwrap();
        out.execute("DELETE FROM info WHERE key = 'complete'", [])
            .unwrap();
        assert!(check_asset(&out).unwrap_err().contains("complete"));
        out.execute("INSERT INTO info VALUES ('complete', 'true')", [])
            .unwrap();
        out.execute("ALTER TABLE passages DROP COLUMN fr_translator", [])
            .unwrap();
        assert!(
            check_asset(&out).unwrap_err().contains("columns"),
            "a column gone is refused"
        );
    }

    #[test]
    fn the_entries_read_are_those_with_an_example_and_a_source() {
        let d = Connection::open_in_memory().unwrap();
        d.execute_batch(
            "CREATE TABLE entries (id INTEGER, lemma TEXT, headword TEXT, folded TEXT, pos TEXT, meaning TEXT, meaning_fr TEXT, example TEXT, source TEXT, sutta TEXT, freq INTEGER);
             INSERT INTO entries VALUES (1, 'vakkali', 'vakkali', 'vakkali', 'masc', 'a monk', NULL, 'alaṃ <b>vakkali</b>', 'SN22.87', 'Vakkalisutta', 3);
             INSERT INTO entries VALUES (2, 'kho', 'kho', 'kho', 'ind', 'indeed', NULL, '', '', NULL, 9);
             INSERT INTO entries VALUES (3, 'ca', 'ca', 'ca', 'ind', 'and', NULL, '<b>ca</b> pana', NULL, NULL, 9);",
        )
        .unwrap();
        let entries = read_entries(&d).unwrap();
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].lemma, "vakkali");
        assert_eq!(entries[0].sutta, "Vakkalisutta");
    }
}
