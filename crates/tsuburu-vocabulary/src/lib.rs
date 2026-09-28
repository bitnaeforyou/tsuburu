//! Every artist, series, group and character hitomi lists, so one can be
//! offered while the reader is still typing it.
//!
//! hitomi publishes nothing that can be searched by prefix: its indexes are
//! keyed on a hash of the whole term, so a name can be confirmed but never
//! guessed at, and the list of names it keeps per namespace cannot be
//! enumerated. The names are therefore carried here, taken from a metadata
//! snapshot and rebuilt with
//! `cargo run --release -p tsuburu-vocabulary --example rebuild -- <meta.redb>`.
//!
//! # What is not here
//!
//! How many works each name has. That moves daily, and one small request to
//! hitomi answers it for a name the reader is actually being offered - a
//! count baked in at release time would be stale before anybody read it.
//!
//! Tags, which come from the Korean dictionary instead: it carries the same
//! words and also knows what each is called in Korean.

use std::sync::OnceLock;

use flate2::read::DeflateDecoder;

const EMBEDDED: &[u8] = include_bytes!("../data/vocabulary.deflate");

/// Names hitomi indexes, ready to be matched against what has been typed.
pub struct Vocabulary {
    /// `name\tnamespace\n` per row, in order of name.
    rows: String,
    /// Where each row starts. Held apart from the text so that 92,000 names
    /// cost one allocation and an offset each, rather than two strings each.
    starts: Vec<u32>,
}

impl Vocabulary {
    /// The vocabulary built into this binary. Unpacked once, when something
    /// first asks for a suggestion.
    pub fn embedded() -> &'static Vocabulary {
        static VOCABULARY: OnceLock<Vocabulary> = OnceLock::new();
        VOCABULARY.get_or_init(|| {
            let mut rows = String::new();
            // A vocabulary that will not unpack is one feature missing, not
            // a reason to refuse to start.
            if std::io::Read::read_to_string(&mut DeflateDecoder::new(EMBEDDED), &mut rows).is_err()
            {
                rows.clear();
            }
            Self::of(rows)
        })
    }

    fn of(rows: String) -> Self {
        let mut starts = Vec::new();
        let mut at = 0usize;
        for line in rows.split_inclusive('\n') {
            starts.push(at as u32);
            at += line.len();
        }
        Self { rows, starts }
    }

    pub fn len(&self) -> usize {
        self.starts.len()
    }

    pub fn is_empty(&self) -> bool {
        self.starts.is_empty()
    }

    /// One row as the name and what kind of thing it names.
    fn row(&self, at: usize) -> (&str, &str) {
        let from = self.starts[at] as usize;
        let upto = self.starts.get(at + 1).map_or(self.rows.len(), |n| *n as usize);
        let line = self.rows[from..upto].trim_end_matches('\n');
        line.split_once('\t').unwrap_or((line, ""))
    }

    fn name(&self, at: usize) -> &str {
        self.row(at).0
    }

    /// The first row whose name is not before `typed`.
    ///
    /// Written out rather than `partition_point`, which searches a slice;
    /// what is being compared here is the text a row points at, not the
    /// offset the slice holds.
    fn first_at_or_after(&self, typed: &str) -> usize {
        let (mut low, mut high) = (0usize, self.starts.len());
        while low < high {
            let mid = low + (high - low) / 2;
            if self.name(mid) < typed { low = mid + 1 } else { high = mid }
        }
        low
    }

    /// Names the reader may be typing, as (namespace, name).
    ///
    /// What was typed at the front comes before what was typed in the
    /// middle: somebody typing `blue` means a name that begins with it, and
    /// `blue archive` should not queue behind `sailor blues`. The rows are
    /// held in order of name, so the first of those is a binary search and
    /// the second is only reached for when it has not filled the list.
    pub fn named_like(&self, typed: &str, limit: usize) -> Vec<(&str, &str)> {
        // More than half of these names are two or more words, and the
        // search box cuts what is typed at every space - so a name like
        // `blue archive` can only ever arrive written the way hitomi writes
        // it in its own box, with underscores. No name here holds one.
        let typed = typed.trim().to_lowercase().replace('_', " ");
        if typed.is_empty() || limit == 0 {
            return Vec::new();
        }

        let mut found = Vec::new();
        for at in self.first_at_or_after(&typed)..self.starts.len() {
            let (name, namespace) = self.row(at);
            if !name.starts_with(&typed) {
                break;
            }
            found.push((namespace, name));
            if found.len() >= limit {
                return found;
            }
        }

        for at in 0..self.starts.len() {
            let (name, namespace) = self.row(at);
            if name.starts_with(&typed) || !name.contains(&typed) {
                continue;
            }
            found.push((namespace, name));
            if found.len() >= limit {
                break;
            }
        }
        found
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_embedded_vocabulary_unpacks() {
        let v = Vocabulary::embedded();
        assert!(v.len() > 50_000, "only {} names", v.len());
    }

    /// The whole point: hitomi cannot be asked "which artists begin with
    /// ke", so the answer has to already be here.
    #[test]
    fn a_prefix_finds_the_artist_it_names() {
        let v = Vocabulary::embedded();
        let found = v.named_like("keso", 20);
        assert!(
            found.iter().any(|(ns, name)| *ns == "artist" && *name == "keso"),
            "keso not offered: {found:?}"
        );
    }

    #[test]
    fn a_prefix_finds_a_series_and_a_group() {
        let v = Vocabulary::embedded();
        let series = v.named_like("blue archive", 20);
        assert!(series.iter().any(|(ns, n)| *ns == "series" && *n == "blue archive"), "{series:?}");
        let group = v.named_like("bluemage", 20);
        assert!(group.iter().any(|(ns, n)| *ns == "group" && *n == "bluemage"), "{group:?}");
    }

    /// What was typed at the front beats what was typed in the middle.
    #[test]
    fn what_begins_with_it_comes_first() {
        let v = Vocabulary::embedded();
        let found = v.named_like("blue", 30);
        let starts = found.iter().position(|(_, n)| !n.starts_with("blue"));
        if let Some(at) = starts {
            assert!(
                found[at..].iter().all(|(_, n)| !n.starts_with("blue")),
                "a prefix match came after a substring one: {found:?}"
            );
        }
    }

    /// The search box cuts at spaces, so a name of two words reaches this
    /// spelled the way hitomi spells it in its own box.
    #[test]
    fn underscores_stand_for_the_spaces_in_a_name() {
        let v = Vocabulary::embedded();
        let found = v.named_like("blue_arch", 20);
        assert!(
            found.iter().any(|(ns, n)| *ns == "series" && *n == "blue archive"),
            "blue archive not reached by underscore: {found:?}"
        );
        assert_eq!(v.named_like("blue_arch", 20), v.named_like("blue arch", 20));
    }

    #[test]
    fn it_answers_nothing_for_nothing() {
        let v = Vocabulary::embedded();
        assert!(v.named_like("", 10).is_empty());
        assert!(v.named_like("keso", 0).is_empty());
        assert!(v.named_like("  ", 10).is_empty());
    }

    #[test]
    fn it_stops_at_the_limit() {
        let v = Vocabulary::embedded();
        assert_eq!(v.named_like("a", 5).len(), 5);
    }

    /// Typing is not always in the case the index holds.
    #[test]
    fn case_does_not_matter() {
        let v = Vocabulary::embedded();
        assert!(v.named_like("KESO", 20).iter().any(|(_, n)| *n == "keso"));
    }

    #[test]
    fn a_vocabulary_that_did_not_unpack_is_simply_empty() {
        let v = Vocabulary::of(String::new());
        assert!(v.is_empty());
        assert!(v.named_like("keso", 10).is_empty());
    }
}
