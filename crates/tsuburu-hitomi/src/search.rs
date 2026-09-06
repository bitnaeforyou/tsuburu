//! 질의 파싱과 집합 연산.
//!
//! 갤러리 ID 목록은 **날짜순이지 ID순이 아니다.** 대체로 내림차순으로 보이지만
//! 같은 시각에 올라온 갤러리들은 ID가 뒤바뀐다(실측: 인접 쌍의 84 %만
//! 내림차순). 그래서 집합 연산은 정렬을 전제하지 않는다.

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

/// 두 목록의 교집합. **첫 번째 목록의 순서를 그대로 유지한다.**
///
/// 정렬을 전제하지 않는다. 검색 결과는 날짜순이지 ID순이 아니어서, 인접한
/// 항목의 ID가 뒤바뀌는 일이 흔하다(실측: 인접 쌍의 84 %만 내림차순). 정렬을
/// 가정한 이진/갤로핑 탐색은 이런 목록에서 대부분의 일치를 놓친다.
///
/// 순서를 유지하는 쪽이 첫 번째 인자이므로, 날짜순 결과를 앞에 두면 결과도
/// 날짜순으로 남는다.
pub fn intersect(a: &[i32], b: &[i32]) -> Vec<i32> {
    if a.is_empty() || b.is_empty() {
        return Vec::new();
    }
    let lookup: HashSet<i32> = b.iter().copied().collect();
    a.iter().copied().filter(|id| lookup.contains(id)).collect()
}

/// `a`에서 `b`에 있는 것을 뺀다. 순서는 유지된다.
pub fn difference(a: &[i32], b: &[i32]) -> Vec<i32> {
    if b.is_empty() {
        return a.to_vec();
    }
    let excluded: HashSet<i32> = b.iter().copied().collect();
    a.iter().copied().filter(|id| !excluded.contains(id)).collect()
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
    fn intersects_and_keeps_the_first_lists_order() {
        assert_eq!(intersect(&[9, 7, 5, 3, 1], &[8, 7, 3, 2]), vec![7, 3]);
    }

    #[test]
    fn works_on_lists_that_are_not_sorted() {
        // 실제 검색 결과는 날짜순이라 ID가 뒤바뀐 자리가 섞여 있다. 정렬을
        // 전제하면 여기서 대부분을 놓친다.
        let a = [4172728, 4172729, 4172710, 4171462, 4171463];
        let b = [4171463, 4172729, 4172710];
        assert_eq!(intersect(&a, &b), vec![4172729, 4172710, 4171463]);
    }

    #[test]
    fn a_large_realistic_intersection_is_not_collapsed() {
        // 회귀 방지: 정렬을 전제한 구현은 이런 입력에서 거의 전부를 놓쳤다.
        let a: Vec<i32> = (0..20_000).map(|i| 1_000_000 - i * 2 + (i % 3)).collect();
        let b: Vec<i32> = (0..20_000).map(|i| 1_000_000 - i * 2 + (i % 3)).collect();
        assert_eq!(intersect(&a, &b).len(), a.len());
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
    fn matches_a_naive_intersection_on_random_unsorted_data() {
        let mut seed = 0x2545_F491_4F6C_DD1Du64;
        let mut next = || {
            seed ^= seed << 13;
            seed ^= seed >> 7;
            seed ^= seed << 17;
            (seed % 500) as i32
        };
        for _ in 0..200 {
            let a: Vec<i32> = (0..50).map(|_| next()).collect();
            let b: Vec<i32> = (0..300).map(|_| next()).collect();
            let set: HashSet<i32> = b.iter().copied().collect();
            let want: Vec<i32> = a.iter().copied().filter(|v| set.contains(v)).collect();
            assert_eq!(intersect(&a, &b), want);
        }
    }

    #[test]
    fn difference_removes_excluded() {
        assert_eq!(difference(&[9, 7, 5, 3], &[7, 3]), vec![9, 5]);
    }
}
