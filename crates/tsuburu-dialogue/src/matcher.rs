//! Scoring a query against a page of recognised text.
//!
//! An exact match on the normalised jamo string wins outright. Failing that,
//! trigram overlap gives a graded score so OCR noise and a misremembered
//! word still surface the page, ranked below exact hits.

use crate::jamo::normalize;
use std::collections::HashSet;

/// Trigram overlap below this is treated as no match.
const FUZZY_THRESHOLD: f32 = 0.55;

#[derive(Debug, Clone, PartialEq)]
pub struct Match {
    /// 1.0 for an exact hit, otherwise the trigram overlap ratio.
    pub score: f32,
    pub exact: bool,
}

pub struct Query {
    normalized: String,
    trigrams: HashSet<[char; 3]>,
}

impl Query {
    pub fn new(text: &str) -> Self {
        let normalized = normalize(text);
        Self { trigrams: trigrams(&normalized), normalized }
    }

    pub fn is_empty(&self) -> bool {
        self.normalized.is_empty()
    }

    /// `haystack` must already be normalised.
    pub fn score(&self, haystack: &str) -> Option<Match> {
        if self.normalized.is_empty() {
            return None;
        }
        if haystack.contains(&self.normalized) {
            return Some(Match { score: 1.0, exact: true });
        }
        if self.trigrams.is_empty() {
            return None;
        }
        let candidate = trigrams(haystack);
        let shared = self.trigrams.intersection(&candidate).count();
        let ratio = shared as f32 / self.trigrams.len() as f32;
        (ratio >= FUZZY_THRESHOLD).then_some(Match { score: ratio, exact: false })
    }
}

fn trigrams(s: &str) -> HashSet<[char; 3]> {
    let chars: Vec<char> = s.chars().collect();
    chars.windows(3).map(|w| [w[0], w[1], w[2]]).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn page(text: &str) -> String {
        normalize(text)
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
        assert!(m.score > 0.7, "score {}", m.score);
    }

    #[test]
    fn unrelated_text_does_not_match() {
        let q = Query::new("구급차라도 부르는 게");
        assert!(q.score(&page("오늘 날씨가 참 좋네요")).is_none());
    }

    #[test]
    fn exact_outranks_fuzzy() {
        let q = Query::new("부르는 게 좋겠어요");
        let exact = q.score(&page("부르는 게 좋겠어요")).unwrap();
        let fuzzy = q.score(&page("부르눈 게 좋겠어요")).unwrap();
        assert!(exact.score > fuzzy.score);
    }

    #[test]
    fn empty_query_matches_nothing() {
        assert!(Query::new("  ").score(&page("아무거나")).is_none());
    }
}
