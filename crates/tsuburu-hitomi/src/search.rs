//! 질의 파싱과 집합 연산.
//!
//! 갤러리 ID 목록은 최신순, 즉 **내림차순**으로 정렬돼 있다(실측:
//! `[4170513, 4169752, ...]`). 오름차순을 가정하면 교집합이 항상 빈 결과를
//! 낸다.

use std::collections::HashSet;

#[derive(Debug, Default, PartialEq, Eq)]
pub struct Query {
    pub include: Vec<String>,
    pub exclude: Vec<String>,
}

impl Query {
    pub fn is_empty(&self) -> bool {
        self.include.is_empty() && self.exclude.is_empty()
    }
}

pub fn parse_query(s: &str) -> Query {
    let mut q = Query::default();
    for token in s.split_whitespace() {
        match token.strip_prefix('-') {
            Some(rest) if !rest.is_empty() => q.exclude.push(rest.to_lowercase()),
            _ => q.include.push(token.to_lowercase()),
        }
    }
    q
}

/// `large[j..]`에서 `v` 이하가 처음 나타나는 위치. 내림차순 전제.
fn gallop(large: &[i32], j: usize, v: i32) -> usize {
    let n = large.len();
    if j >= n || large[j] <= v {
        return j.min(n);
    }
    let mut step = 1usize;
    while j + step < n && large[j + step] > v {
        step *= 2;
    }
    // large[lo] > v 가 확인된 마지막 지점, hi는 그 너머
    let lo = j + step / 2;
    let hi = (j + step).min(n);
    let mut left = lo + 1;
    let mut right = hi;
    while left < right {
        let mid = left + (right - left) / 2;
        if large[mid] > v {
            left = mid + 1;
        } else {
            right = mid;
        }
    }
    left
}

/// 내림차순 정렬된 두 목록의 교집합. 크기 차가 큰 경우를 위해 갤로핑으로
/// 건너뛴다. 결과도 내림차순이다.
pub fn intersect(a: &[i32], b: &[i32]) -> Vec<i32> {
    let (small, large) = if a.len() <= b.len() { (a, b) } else { (b, a) };
    let mut out = Vec::new();
    let mut j = 0usize;
    for &v in small {
        j = gallop(large, j, v);
        if j >= large.len() {
            break;
        }
        if large[j] == v {
            out.push(v);
            j += 1;
        }
    }
    out
}

/// `a`에서 `b`에 있는 것을 뺀다. 순서는 유지된다.
pub fn difference(a: &[i32], b: &[i32]) -> Vec<i32> {
    let excluded: HashSet<i32> = b.iter().copied().collect();
    a.iter().copied().filter(|v| !excluded.contains(v)).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_terms_and_negations() {
        let q = parse_query("  school -yaoi Glasses ");
        assert_eq!(q.include, vec!["school", "glasses"]);
        assert_eq!(q.exclude, vec!["yaoi"]);
    }

    #[test]
    fn bare_dash_is_treated_as_a_term() {
        assert_eq!(parse_query("-").include, vec!["-"]);
    }

    #[test]
    fn intersects_descending_lists() {
        assert_eq!(intersect(&[9, 7, 5, 3, 1], &[8, 7, 3, 2]), vec![7, 3]);
    }

    #[test]
    fn intersection_of_disjoint_is_empty() {
        assert!(intersect(&[9, 7], &[8, 6]).is_empty());
    }

    #[test]
    fn intersection_with_empty_is_empty() {
        assert!(intersect(&[9, 7], &[]).is_empty());
        assert!(intersect(&[], &[9, 7]).is_empty());
    }

    #[test]
    fn identical_lists_intersect_to_themselves() {
        let v = vec![9, 7, 5, 3, 1];
        assert_eq!(intersect(&v, &v), v);
    }

    #[test]
    fn galloping_handles_very_lopsided_inputs() {
        let big: Vec<i32> = (0..10_000).rev().collect();
        assert_eq!(intersect(&big, &[5000, 17]), vec![5000, 17]);
    }

    #[test]
    fn galloping_matches_naive_intersection_on_random_data() {
        // 결정적 의사난수. 갤로핑 구현이 단순 구현과 같은 답을 내는지 확인한다.
        let mut seed = 0x2545_F491_4F6C_DD1Du64;
        let mut next = || {
            seed ^= seed << 13;
            seed ^= seed >> 7;
            seed ^= seed << 17;
            (seed % 500) as i32
        };
        for _ in 0..200 {
            let mut a: Vec<i32> = (0..50).map(|_| next()).collect();
            let mut b: Vec<i32> = (0..300).map(|_| next()).collect();
            a.sort_unstable_by(|x, y| y.cmp(x));
            a.dedup();
            b.sort_unstable_by(|x, y| y.cmp(x));
            b.dedup();

            let want: Vec<i32> = {
                let set: HashSet<i32> = b.iter().copied().collect();
                a.iter().copied().filter(|v| set.contains(v)).collect()
            };
            assert_eq!(intersect(&a, &b), want);
        }
    }

    #[test]
    fn difference_removes_excluded() {
        assert_eq!(difference(&[9, 7, 5, 3], &[7, 3]), vec![9, 5]);
    }
}
