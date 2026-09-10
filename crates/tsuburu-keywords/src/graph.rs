//! Reading artifact's `graph.csv`.
//!
//! One row per word: `article_id,rank,keyword,score,tf,df,total_pages,
//! dialogue_count,char_count`. Rows for a work are together and already in
//! rank order, so the file streams straight into the store a work at a time.

use crate::{KEEP_PER_WORK, KeywordError, KeywordStore, MAX_DOCUMENT_FREQUENCY, Scored};
use std::io::BufRead;
use std::path::Path;

const HEADER: &str = "article_id,rank,keyword,score";
/// Works per write transaction. One transaction for four million rows holds
/// the whole file in memory before it commits.
const BATCH: usize = 2_000;

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct Imported {
    pub works: u64,
    pub words: u64,
    /// Rows dropped because the word is in too many works to mean anything.
    pub too_common: u64,
}

/// Loads `path` into `store`, calling `progress` every few thousand works.
pub fn import(
    store: &KeywordStore,
    path: impl AsRef<Path>,
    progress: &mut dyn FnMut(&Imported),
) -> Result<Imported, KeywordError> {
    let path = path.as_ref();
    let file = std::fs::File::open(path)
        .map_err(|e| KeywordError::Io(path.display().to_string(), e.to_string()))?;
    let mut lines = std::io::BufReader::with_capacity(1 << 20, file).lines();

    let first = lines
        .next()
        .transpose()
        .map_err(|e| KeywordError::Io(path.display().to_string(), e.to_string()))?
        .unwrap_or_default();
    if !first.starts_with(HEADER) {
        return Err(KeywordError::NotGraphCsv(
            path.display().to_string(),
            format!("its first line is {:?}", first.chars().take(40).collect::<String>()),
        ));
    }

    let mut totals = Imported::default();
    let mut common: std::collections::HashMap<String, u32> = std::collections::HashMap::new();
    let mut pending: Vec<(i32, Vec<Scored>)> = Vec::new();
    let mut current: Option<(i32, Vec<Scored>)> = None;

    for line in lines {
        let line = line.map_err(|e| KeywordError::Io(path.display().to_string(), e.to_string()))?;
        let Some(row) = parse_row(&line) else { continue };
        if row.document_frequency > MAX_DOCUMENT_FREQUENCY {
            totals.too_common += 1;
            common.insert(row.keyword, row.document_frequency);
            continue;
        }
        match &mut current {
            Some((id, words)) if *id == row.article => {
                if words.len() < KEEP_PER_WORK {
                    words.push(Scored { word: row.keyword, score: row.score });
                }
            }
            _ => {
                if let Some(done) = current.take() {
                    pending.push(done);
                }
                current = Some((row.article, vec![Scored { word: row.keyword, score: row.score }]));
            }
        }
        if pending.len() >= BATCH {
            totals = write_batch(store, &mut pending, totals)?;
            progress(&totals);
        }
    }
    if let Some(done) = current.take() {
        pending.push(done);
    }
    totals = write_batch(store, &mut pending, totals)?;
    for (word, works) in common {
        store.note_common(&word, works)?;
    }
    Ok(totals)
}

fn write_batch(
    store: &KeywordStore,
    pending: &mut Vec<(i32, Vec<Scored>)>,
    mut totals: Imported,
) -> Result<Imported, KeywordError> {
    if pending.is_empty() {
        return Ok(totals);
    }
    let tx = store.db.begin_write().map_err(crate::db_err)?;
    for (id, words) in pending.iter() {
        store.put_within(&tx, *id, words)?;
        totals.works += 1;
        totals.words += words.len() as u64;
    }
    tx.commit().map_err(crate::db_err)?;
    pending.clear();
    Ok(totals)
}

struct Row {
    article: i32,
    keyword: String,
    score: f32,
    document_frequency: u32,
}

/// The keyword may hold commas, so the row is read from both ends: two fields
/// from the left, six from the right, and whatever is left over between them
/// is the word.
fn parse_row(line: &str) -> Option<Row> {
    let mut head = line.splitn(3, ',');
    let article = head.next()?.trim().parse::<i32>().ok()?;
    head.next()?;
    let rest = head.next()?;

    let mut tail = rest.rsplitn(7, ',');
    tail.next()?; // char_count
    tail.next()?; // dialogue_count
    tail.next()?; // total_pages
    let document_frequency = tail.next()?.trim().parse::<u32>().ok()?;
    tail.next()?; // tf
    let score = tail.next()?.trim().parse::<f32>().ok()?;
    let keyword = tail.next()?.trim().trim_matches('"').trim();
    if keyword.is_empty() {
        return None;
    }
    Some(Row { article, keyword: keyword.to_string(), score, document_frequency })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn write(dir: &Path, body: &str) -> std::path::PathBuf {
        let path = dir.join("graph.csv");
        std::fs::write(&path, body).unwrap();
        path
    }

    const HEAD: &str =
        "article_id,rank,keyword,score,tf,df,total_pages,dialogue_count,char_count\n";

    #[test]
    fn reads_a_work_and_its_words() {
        let dir = tempfile::tempdir().unwrap();
        let path = write(
            dir.path(),
            &format!(
                "{HEAD}8035,1,에미,20.360738,7,292,157,1702,8542\n\
                 8035,2,에츠코,18.736700,3,38,157,1702,8542\n\
                 8040,1,보스,18.225372,8,790,157,1702,8542\n"
            ),
        );
        let store = KeywordStore::open(dir.path().join("k.redb")).unwrap();
        let done = import(&store, &path, &mut |_| {}).unwrap();
        assert_eq!(done.works, 2);
        assert_eq!(done.words, 3);

        let words = store.of(8035).unwrap().unwrap();
        assert_eq!(words.len(), 2);
        assert_eq!(words[0].word, "에미");
        assert!((words[0].score - 20.360738).abs() < 0.01);
        assert_eq!(store.search("보스", 5).unwrap().first().map(|(id, _)| *id), Some(8040));
    }

    #[test]
    fn words_in_too_many_works_are_left_out() {
        let dir = tempfile::tempdir().unwrap();
        let common = MAX_DOCUMENT_FREQUENCY + 1;
        let path = write(
            dir.path(),
            &format!(
                "{HEAD}1,1,그래서,5.0,9,{common},10,20,30\n\
                 1,2,에미,20.0,9,10,10,20,30\n"
            ),
        );
        let store = KeywordStore::open(dir.path().join("k.redb")).unwrap();
        let done = import(&store, &path, &mut |_| {}).unwrap();
        assert_eq!(done.too_common, 1);
        assert_eq!(
            store.of(1).unwrap().unwrap(),
            vec![Scored { word: "에미".into(), score: 20.0 }]
        );
        // Searching for it later can say why, rather than "nowhere".
        assert_eq!(store.common("그래서").unwrap(), Some(common));
        assert_eq!(store.common("에미").unwrap(), None);
    }

    #[test]
    fn a_word_holding_a_comma_survives() {
        let row = parse_row("8035,1,\"어, 그래\",20.0,7,292,157,1702,8542").unwrap();
        assert_eq!(row.keyword, "어, 그래");
        assert_eq!(row.article, 8035);
        assert_eq!(row.document_frequency, 292);
    }

    #[test]
    fn a_short_or_broken_row_is_skipped_not_fatal() {
        assert!(parse_row("8035,1,에미").is_none());
        assert!(parse_row("not a number,1,에미,20.0,7,292,157,1702,8542").is_none());
        assert!(parse_row("8035,1,,20.0,7,292,157,1702,8542").is_none());
    }

    #[test]
    fn another_file_is_refused_by_its_header() {
        let dir = tempfile::tempdir().unwrap();
        let path = write(dir.path(), "id,name\n1,a\n");
        let store = KeywordStore::open(dir.path().join("k.redb")).unwrap();
        let err = import(&store, &path, &mut |_| {}).unwrap_err();
        assert!(matches!(err, KeywordError::NotGraphCsv(..)), "{err}");
    }

    #[test]
    fn progress_is_reported_while_reading() {
        let dir = tempfile::tempdir().unwrap();
        let mut body = String::from(HEAD);
        for id in 0..BATCH + 10 {
            body.push_str(&format!("{id},1,w{id},1.0,1,1,1,1,1\n"));
        }
        let path = write(dir.path(), &body);
        let store = KeywordStore::open(dir.path().join("k.redb")).unwrap();
        let mut seen = Vec::new();
        let done = import(&store, &path, &mut |p| seen.push(p.works)).unwrap();
        assert_eq!(done.works as usize, BATCH + 10);
        assert_eq!(seen, vec![BATCH as u64]);
    }
}
