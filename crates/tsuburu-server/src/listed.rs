//! Which works hitomi still lists.
//!
//! A work that has been taken off the site keeps its pages: the index stops
//! naming it, and everything else about it stays where it was. That is why a
//! number pasted into the search box still opens something the site will not
//! show you - and why it is worth knowing which of the works on screen are in
//! that state, because they are the ones worth keeping a copy of.
//!
//! The list is hitomi's own `index-all.nozomi`: four bytes per work, about
//! 1.2 million of them. It is fetched once, in the background, and kept
//! sorted so asking about a work is a binary search rather than a request.

use std::sync::{Arc, RwLock};
use std::time::{Duration, Instant};

/// Long enough that a session never fetches it twice, short enough that a
/// program left running for days notices new works.
const KEEP_FOR: Duration = Duration::from_secs(6 * 60 * 60);

#[derive(Default)]
pub struct Listing {
    inner: RwLock<Option<Known>>,
    fetching: RwLock<bool>,
}

struct Known {
    /// Ascending, so `binary_search` answers.
    ids: Vec<i32>,
    at: Instant,
}

impl Listing {
    /// Whether hitomi still lists this work, or `None` while nobody knows.
    pub fn listed(&self, id: i32) -> Option<bool> {
        let held = self.inner.read().ok()?;
        let known = held.as_ref()?;
        (known.at.elapsed() < KEEP_FOR).then(|| known.ids.binary_search(&id).is_ok())
    }

    pub fn ready(&self) -> bool {
        self.inner.read().ok().and_then(|h| h.as_ref().map(|k| k.at.elapsed() < KEEP_FOR))
            == Some(true)
    }

    /// Starts fetching the list if nobody has. Returns at once either way:
    /// the answer is worth having but never worth waiting for.
    pub fn warm(self: &Arc<Self>, fetcher: Arc<tsuburu_fetch::HttpFetcher>, url: String) {
        if self.ready() {
            return;
        }
        {
            let Ok(mut busy) = self.fetching.write() else { return };
            if *busy {
                return;
            }
            *busy = true;
        }

        let me = Arc::clone(self);
        tokio::spawn(async move {
            match tsuburu_hitomi::nozomi::all_ids(fetcher.as_ref(), &url).await {
                Ok(bytes) => {
                    let ids = parse(&bytes);
                    tracing::debug!(works = ids.len(), "read hitomi's list of works");
                    if let Ok(mut held) = me.inner.write() {
                        *held = Some(Known { ids, at: Instant::now() });
                    }
                }
                Err(error) => tracing::debug!(%error, "could not read hitomi's list of works"),
            }
            if let Ok(mut busy) = me.fetching.write() {
                *busy = false;
            }
        });
    }
}

/// `.nozomi` is a flat run of big-endian 32 bit ids, newest first. Sorted
/// here so that asking about one work is a binary search.
fn parse(bytes: &[u8]) -> Vec<i32> {
    let mut ids = tsuburu_hitomi::nozomi::decode_ids(bytes);
    ids.sort_unstable();
    ids.dedup();
    ids
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_the_ids_and_sorts_them_for_asking() {
        let bytes: Vec<u8> = [300i32, 100, 200, 100].iter().flat_map(|n| n.to_be_bytes()).collect();
        assert_eq!(parse(&bytes), vec![100, 200, 300]);
    }

    #[test]
    fn a_trailing_byte_is_not_half_an_id() {
        let mut bytes: Vec<u8> = [7i32, 9].iter().flat_map(|n| n.to_be_bytes()).collect();
        bytes.push(0xff);
        assert_eq!(parse(&bytes), vec![7, 9]);
    }

    #[test]
    fn nobody_knows_until_it_has_been_read() {
        let listing = Listing::default();
        assert_eq!(listing.listed(123), None);
        assert!(!listing.ready());
    }

    #[test]
    fn says_which_works_are_still_listed() {
        let listing = Listing::default();
        *listing.inner.write().unwrap() =
            Some(Known { ids: vec![100, 200, 300], at: Instant::now() });
        assert!(listing.ready());
        assert_eq!(listing.listed(200), Some(true));
        assert_eq!(listing.listed(250), Some(false));
    }

    #[test]
    fn a_list_old_enough_to_be_wrong_is_no_answer() {
        let listing = Listing::default();
        *listing.inner.write().unwrap() = Some(Known {
            ids: vec![100],
            at: Instant::now().checked_sub(KEEP_FOR + Duration::from_secs(1)).unwrap(),
        });
        assert_eq!(listing.listed(100), None);
        assert!(!listing.ready());
    }
}
