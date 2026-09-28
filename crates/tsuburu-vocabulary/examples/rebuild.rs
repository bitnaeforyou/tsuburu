//! Rebuilds the embedded vocabulary from a metadata snapshot.
//!
//!     cargo run --release -p tsuburu-vocabulary --example rebuild -- <meta.redb>
//!
//! Reads the snapshot's term index directly rather than through
//! `tsuburu-meta`, so the crate that carries the data can rebuild it without
//! depending on the one that imports it.

use std::collections::BTreeSet;
use std::io::Write;

use flate2::{Compression, write::DeflateEncoder};
use redb::{Database, MultimapTableDefinition, ReadableMultimapTable};

const TERMS: MultimapTableDefinition<&str, i32> = MultimapTableDefinition::new("terms");

/// Tags are left out: they come from the Korean dictionary, which also says
/// what each one is in Korean. These four are the ones hitomi indexes by
/// name and nothing else knows.
const WANTED: [&str; 4] = ["artist", "series", "group", "character"];

fn main() {
    let mut args = std::env::args().skip(1);
    let snapshot = args.next().expect("usage: rebuild <meta.redb> [out]");
    let out = args.next().unwrap_or_else(|| {
        concat!(env!("CARGO_MANIFEST_DIR"), "/data/vocabulary.deflate").to_string()
    });

    let db = Database::open(&snapshot).expect("could not open the snapshot");
    let tx = db.begin_read().expect("could not read the snapshot");
    let terms = tx.open_multimap_table(TERMS).expect("the snapshot has no term index");

    // Sorted by name, because that is what a prefix is looked up by; a
    // BTreeSet also drops the duplicates two namespaces can produce.
    let mut rows: BTreeSet<(String, String)> = BTreeSet::new();
    for row in terms.iter().expect("could not walk the term index") {
        let (key, _) = row.expect("could not read a term");
        let key = key.value();
        let Some((namespace, name)) = key.split_once(':') else { continue };
        if WANTED.contains(&namespace) && !name.trim().is_empty() {
            rows.insert((name.to_lowercase(), namespace.to_string()));
        }
    }

    let mut blob = String::new();
    for (name, namespace) in &rows {
        blob.push_str(name);
        blob.push('\t');
        blob.push_str(namespace);
        blob.push('\n');
    }

    let mut encoder = DeflateEncoder::new(Vec::new(), Compression::best());
    encoder.write_all(blob.as_bytes()).expect("could not compress");
    let packed = encoder.finish().expect("could not compress");
    std::fs::write(&out, &packed).expect("could not write the vocabulary");

    println!("{} names, {} B -> {} B, written to {out}", rows.len(), blob.len(), packed.len());
}
