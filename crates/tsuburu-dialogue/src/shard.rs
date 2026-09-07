//! Portable snapshots of recognised text.
//!
//! The expensive artefact is the text, not the images: a whole language's
//! worth fits in a few hundred megabytes. A shard is one gallery-id range,
//! named by the hash of its bytes, so that copies from anywhere can be
//! verified and merged. How shards travel (HTTP, torrent, a USB stick) is
//! deliberately outside this crate.

use crate::store::PageText;
use flate2::Compression;
use flate2::read::DeflateDecoder;
use flate2::write::DeflateEncoder;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::io::{Read, Write};

const MAGIC: &[u8; 4] = b"TSDX";
const VERSION: u16 = 1;

#[derive(Debug, thiserror::Error)]
pub enum ShardError {
    #[error("not a tsuburu dialogue shard")]
    BadMagic,
    #[error("shard version {0} is newer than this tsuburu understands")]
    NewerVersion(u16),
    #[error("shard is corrupt: {0}")]
    Corrupt(String),
    #[error("shard hash does not match its name")]
    HashMismatch,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ShardEntry {
    pub gallery_id: i32,
    pub pages: Vec<PageText>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Shard {
    pub first_id: i32,
    pub last_id: i32,
    pub entries: Vec<ShardEntry>,
}

impl Shard {
    pub fn encode(&self) -> Result<Vec<u8>, ShardError> {
        let json = serde_json::to_vec(self).map_err(|e| ShardError::Corrupt(e.to_string()))?;
        let mut encoder = DeflateEncoder::new(Vec::new(), Compression::best());
        encoder.write_all(&json).map_err(|e| ShardError::Corrupt(e.to_string()))?;
        let body = encoder.finish().map_err(|e| ShardError::Corrupt(e.to_string()))?;

        let mut out = Vec::with_capacity(body.len() + 6);
        out.extend_from_slice(MAGIC);
        out.extend_from_slice(&VERSION.to_be_bytes());
        out.extend_from_slice(&body);
        Ok(out)
    }

    pub fn decode(bytes: &[u8]) -> Result<Self, ShardError> {
        if bytes.len() < 6 || &bytes[0..4] != MAGIC {
            return Err(ShardError::BadMagic);
        }
        let version = u16::from_be_bytes([bytes[4], bytes[5]]);
        if version > VERSION {
            return Err(ShardError::NewerVersion(version));
        }
        let mut json = Vec::new();
        DeflateDecoder::new(&bytes[6..])
            .read_to_end(&mut json)
            .map_err(|e| ShardError::Corrupt(e.to_string()))?;
        serde_json::from_slice(&json).map_err(|e| ShardError::Corrupt(e.to_string()))
    }

    /// `dialogue-<first>-<last>-<hash16>.tsd`
    pub fn file_name(&self, encoded: &[u8]) -> String {
        format!("dialogue-{}-{}-{}.tsd", self.first_id, self.last_id, short_hash(encoded))
    }

    /// Refuses a file whose contents do not match the hash in its name.
    pub fn decode_named(name: &str, bytes: &[u8]) -> Result<Self, ShardError> {
        let expected = name
            .strip_suffix(".tsd")
            .and_then(|s| s.rsplit('-').next())
            .ok_or(ShardError::BadMagic)?;
        if expected != short_hash(bytes) {
            return Err(ShardError::HashMismatch);
        }
        Self::decode(bytes)
    }
}

fn short_hash(bytes: &[u8]) -> String {
    let digest = Sha256::digest(bytes);
    digest[..8].iter().map(|b| format!("{b:02x}")).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample() -> Shard {
        Shard {
            first_id: 1000,
            last_id: 1999,
            entries: vec![ShardEntry {
                gallery_id: 1234,
                pages: vec![PageText { page: 0, lines: vec!["일단".into(), "구급차라도".into()] }],
            }],
        }
    }

    #[test]
    fn round_trips() {
        let shard = sample();
        let bytes = shard.encode().unwrap();
        assert_eq!(Shard::decode(&bytes).unwrap(), shard);
    }

    #[test]
    fn name_carries_range_and_hash_and_is_verified() {
        let shard = sample();
        let bytes = shard.encode().unwrap();
        let name = shard.file_name(&bytes);
        assert!(name.starts_with("dialogue-1000-1999-"));
        assert_eq!(Shard::decode_named(&name, &bytes).unwrap(), shard);

        let mut tampered = bytes.clone();
        let last = tampered.len() - 1;
        tampered[last] ^= 0xff;
        assert!(matches!(Shard::decode_named(&name, &tampered), Err(ShardError::HashMismatch)));
    }

    #[test]
    fn rejects_foreign_and_future_files() {
        assert!(matches!(Shard::decode(b"nope"), Err(ShardError::BadMagic)));
        let mut future = sample().encode().unwrap();
        future[4..6].copy_from_slice(&99u16.to_be_bytes());
        assert!(matches!(Shard::decode(&future), Err(ShardError::NewerVersion(99))));
    }
}
