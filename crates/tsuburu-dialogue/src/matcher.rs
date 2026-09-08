//! Scoring a query against a page of recognised text.
//!
//! Both sides are reduced to match codes (see [`crate::jamo::codes`]). An
//! exact substring hit wins outright. Otherwise the page is scored by how
//! many of the query's distinct n-grams it contains, which tolerates OCR
//! noise and a misremembered word, ranked below exact hits.
//!
//! The fuzzy pass has a gate and a score. The gate asks whether one of the
//! query's halves (or quarters, for long queries) appears verbatim: an OCR
//! error or a misremembered word breaks one fragment and leaves the others,
//! while an unrelated page almost never contains three syllables of the
//! query in a row. Only pages through the gate are scored, by sweeping
//! every n-byte window against a 64K-bit set built from the query. No
//! allocation per candidate, which is what a scan over 2.5 million pages
//! needs.

use crate::jamo::codes;

/// Window length in code bytes; roughly two syllables. Three-jamo windows
/// are so common in Korean that unrelated pages scored well.
const N: usize = 6;
/// Floor on the n-gram overlap for a page that passed the gate.
const FUZZY_THRESHOLD: f32 = 0.3;
/// Shortest fragment worth testing verbatim: about three syllables.
const MIN_FRAGMENT: usize = 8;
/// Distinct query n-grams tracked. Longer queries keep the first ones.
const MAX_GRAMS: usize = 64;

#[derive(Debug, Clone, PartialEq)]
pub struct Match {
    /// 1.0 for an exact hit, otherwise the n-gram overlap ratio.
    pub score: f32,
    pub exact: bool,
}

pub struct Query {
    codes: Vec<u8>,
    /// Distinct n-grams of the query in order of first appearance.
    grams: Vec<[u8; N]>,
    /// One bit per 16-bit hash of a query n-gram.
    bits: Box<[u64; 1024]>,
    /// Byte ranges of the query that must appear verbatim for a fuzzy hit.
    fragments: Vec<std::ops::Range<usize>>,
}

impl Query {
    pub fn new(text: &str) -> Self {
        let codes = codes(text);
        let mut grams: Vec<[u8; N]> = Vec::new();
        let mut bits = Box::new([0u64; 1024]);
        for w in codes.windows(N) {
            let gram: [u8; N] = w.try_into().expect("window of N");
            if grams.contains(&gram) {
                continue;
            }
            if grams.len() >= MAX_GRAMS {
                break;
            }
            let h = hash(&gram);
            bits[(h >> 6) as usize] |= 1 << (h & 63);
            grams.push(gram);
        }
        let fragments = fragments(codes.len());
        Self { codes, grams, bits, fragments }
    }

    pub fn is_empty(&self) -> bool {
        self.codes.is_empty()
    }

    pub fn codes(&self) -> &[u8] {
        &self.codes
    }

    /// `haystack` is a page's match codes.
    pub fn score(&self, haystack: &[u8]) -> Option<Match> {
        if self.is_exact(haystack) {
            return Some(Match { score: 1.0, exact: true });
        }
        self.fuzzy(haystack)
    }

    /// Substring test only. Cheap enough to run over the whole corpus first;
    /// the fuzzy sweep is only worth paying for when exact hits run out.
    pub fn is_exact(&self, haystack: &[u8]) -> bool {
        !self.codes.is_empty() && memchr::memmem::find(haystack, &self.codes).is_some()
    }

    pub fn fuzzy(&self, haystack: &[u8]) -> Option<Match> {
        if self.grams.is_empty() || haystack.len() < N {
            return None;
        }
        let through_gate = self
            .fragments
            .iter()
            .any(|r| memchr::memmem::find(haystack, &self.codes[r.clone()]).is_some());
        if !through_gate {
            return None;
        }
        let mut found = 0u64;
        for i in 0..=haystack.len() - N {
            let h = hash(&haystack[i..i + N]);
            if self.bits[(h >> 6) as usize] & (1 << (h & 63)) == 0 {
                continue;
            }
            // Hash hit: resolve to the actual gram (few, so a scan is fine).
            let w = &haystack[i..i + N];
            if let Some(k) = self.grams.iter().position(|g| g.as_slice() == w) {
                found |= 1 << k;
            }
        }
        let ratio = found.count_ones() as f32 / self.grams.len() as f32;
        (ratio >= FUZZY_THRESHOLD).then_some(Match { score: ratio, exact: false })
    }
}

/// Halves, and quarters when they are still three syllables long. A query
/// too short to split gets no fragments and therefore no fuzzy matches;
/// it is too short to be reliable anyway.
fn fragments(len: usize) -> Vec<std::ops::Range<usize>> {
    let mut out = Vec::new();
    for parts in [2usize, 4] {
        let size = len / parts;
        if size < MIN_FRAGMENT {
            break;
        }
        for i in 0..parts {
            let start = i * size;
            let end = if i + 1 == parts { len } else { start + size };
            out.push(start..end);
        }
    }
    out
}

/// Six bytes packed into a word and mixed once; runs at the speed of a
/// multiply per position, which the sweep over 1.5 billion windows needs.
#[inline]
fn hash(window: &[u8]) -> u16 {
    let w = (window[0] as u64)
        | (window[1] as u64) << 8
        | (window[2] as u64) << 16
        | (window[3] as u64) << 24
        | (window[4] as u64) << 32
        | (window[5] as u64) << 40;
    (w.wrapping_mul(0x9E37_79B9_7F4A_7C15) >> 48) as u16
}

#[cfg(test)]
mod tests {
    use super::*;

    fn page(text: &str) -> Vec<u8> {
        codes(text)
    }

    #[test]
    fn exact_substring_is_a_perfect_score() {
        let q = Query::new("구급차라도 부르는 게");
        let m = q.score(&page("일단 구급차라도 부르는 게 좋겠어요.")).unwrap();
        assert!(m.exact);
        assert_eq!(m.score, 1.0);
    }

    #[test]
    fn punctuation_and_spacing_do_not_matter() {
        let q = Query::new("좋겠어요");
        assert!(q.score(&page("좋 겠 어 요 !")).unwrap().exact);
    }

    #[test]
    fn one_wrong_final_consonant_still_matches_fuzzily() {
        let q = Query::new("구급차라도 부르는 게 좋겠어요");
        let m = q.score(&page("구급차라도 부르눈 게 좋겠어요")).unwrap();
        assert!(!m.exact);
        assert!(m.score > 0.6, "score {}", m.score);
    }

    #[test]
    fn unrelated_text_does_not_match() {
        let q = Query::new("구급차라도 부르는 게");
        assert!(q.score(&page("오늘 날씨가 참 좋네요")).is_none());
    }

    #[test]
    fn common_syllables_alone_do_not_match_a_long_page() {
        // Three-jamo windows matched almost any Korean page; two-syllable
        // windows must not.
        let q = Query::new("이 세상에 없는 문장 zzqx");
        let long = "그럼 나머진 부탁할게 재알바 2년째에 본사 바리스타대회 입상자니까 꼼꼼하게 가르쳐 줄거야 \
                    어휴 귀찮겠네 사람말 좀 들어라 와 대단하시네요 다른 선배한테 부탁하는 편이 \
                    이렇게 내키진 종류가 많으니 헷갈리지 않게 조심하시고 창고의 원두를 카운터까지 옮겨주세요";
        assert!(q.score(&page(long)).is_none());
    }

    #[test]
    fn exact_outranks_fuzzy() {
        let q = Query::new("부르는 게 좋겠어요");
        let exact = q.score(&page("부르는 게 좋겠어요")).unwrap();
        let fuzzy = q.score(&page("부르눈 게 좋겠어요")).unwrap();
        assert!(exact.score > fuzzy.score);
    }

    #[test]
    fn a_misremembered_word_in_the_middle_still_matches() {
        let q = Query::new("일단 구급차라도 부르는 게 좋겠어요");
        let m = q.score(&page("일단 구급차라도 불러오는 게 좋겠어요")).unwrap();
        assert!(!m.exact);
        assert!(m.score >= FUZZY_THRESHOLD);
    }

    #[test]
    fn fragments_split_into_halves_and_quarters_when_long_enough() {
        assert_eq!(fragments(19), vec![0..9, 9..19]);
        assert_eq!(fragments(40), vec![0..20, 20..40, 0..10, 10..20, 20..30, 30..40]);
        assert!(fragments(10).is_empty(), "too short to split into three-syllable halves");
    }

    #[test]
    fn empty_query_matches_nothing() {
        assert!(Query::new("  ").score(&page("아무거나")).is_none());
    }
}
