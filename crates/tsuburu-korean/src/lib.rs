//! 한국어 검색어를 hitomi가 이해하는 영어로 보정한다.
//!
//! 사전은 퍼블릭 도메인으로 공개된 태그 대응표에서 만들었고
//! `tools/build-korean-dictionary.py`로 재생성한다.
//!
//! # 한계
//!
//! 커버리지가 영역마다 크게 다르다. 태그는 81 %, 시리즈는 55 %, 캐릭터는
//! 23 %이고 **작가 이름은 1 % 미만이라 사실상 되지 않는다.** 코드로 메울 수
//! 있는 문제가 아니므로 감추지 않는다. [`translate`]는 번역이 있었는지를
//! 낱말마다 돌려주고, 화면은 그것을 그대로 보여준다.
//!
//! # 하지 않는 것
//!
//! 형태소 분석, 자모 분해, 오타 교정, 번역 API 호출. 사전에 있는 낱말을
//! 그대로 찾는 것까지만 한다.

use serde::Serialize;
use std::collections::HashMap;
use std::sync::OnceLock;

/// 한국어 항목 하나가 가질 수 있는 최대 낱말 수. 최장 일치를 시도할 창의 크기다.
const MAX_PHRASE_WORDS: usize = 8;

const EMBEDDED: &str = include_str!("../data/korean.tsv");

pub struct Dictionary {
    /// 한국어 구 -> 영어 후보. 후보 순서는 생성 시점에 고정된다.
    entries: HashMap<String, Vec<String>>,
    /// The same the other way, for saying an English tag in Korean.
    ///
    /// hitomi's vocabulary is English, so a Korean reader is handed
    /// `mosaic censorship` and `sole female` on every work. The dictionary
    /// that lets them search in Korean already knows what those are called.
    korean: HashMap<String, String>,
}

impl Dictionary {
    pub fn parse(tsv: &str) -> Self {
        let mut entries = HashMap::new();
        for line in tsv.lines() {
            let mut parts = line.split('\t').map(str::trim).filter(|s| !s.is_empty());
            let Some(korean) = parts.next() else { continue };
            let english: Vec<String> = parts.map(str::to_string).collect();
            if english.is_empty() {
                continue;
            }
            entries.insert(korean.to_lowercase(), english);
        }

        // One English tag can be reached from several Korean phrases - they
        // are spellings or synonyms of each other. The one to show back is
        // the phrase that means the fewest things: a Korean phrase standing
        // for one English tag is a tighter fit than one standing for six.
        // Ties go to the shorter, and then to the alphabet, so a rebuild
        // gives the same answer as the last one.
        let mut korean: HashMap<String, String> = HashMap::new();
        let mut rows: Vec<(&String, &Vec<String>)> = entries.iter().collect();
        rows.sort_unstable_by(|a, b| {
            a.1.len().cmp(&b.1.len()).then(a.0.len().cmp(&b.0.len())).then(a.0.cmp(b.0))
        });
        for (phrase, english) in rows {
            for tag in english {
                korean.entry(tag.to_lowercase()).or_insert_with(|| phrase.clone());
            }
        }

        Self { entries, korean }
    }

    /// 바이너리에 임베드된 사전. 처음 쓰일 때 한 번만 파싱한다.
    pub fn embedded() -> &'static Dictionary {
        static DICTIONARY: OnceLock<Dictionary> = OnceLock::new();
        DICTIONARY.get_or_init(|| Dictionary::parse(EMBEDDED))
    }

    pub fn len(&self) -> usize {
        self.entries.len()
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    pub fn lookup(&self, phrase: &str) -> Option<&[String]> {
        self.entries.get(&phrase.to_lowercase()).map(Vec::as_slice)
    }

    /// Words that begin with or contain what has been typed so far.
    ///
    /// Both languages are searched, because a reader may type either, and
    /// what comes back is the pair: the English hitomi indexes by, and what
    /// to show it as. The ones that start with what was typed come first, and
    /// the shorter before the longer - `big ass` before `bike shorts` for
    /// `bi` - so the likeliest is at the front.
    pub fn suggest(&self, typed: &str, limit: usize) -> Vec<Suggestion> {
        let typed = typed.trim().to_lowercase();
        if typed.is_empty() || limit == 0 {
            return Vec::new();
        }
        let mut found: Vec<(u8, usize, &str, &str)> = Vec::new();
        for (english, korean) in &self.korean {
            let rank = match (english.find(&typed), korean.find(&typed)) {
                (Some(0), _) | (_, Some(0)) => 0u8,
                (Some(_), _) | (_, Some(_)) => 1,
                _ => continue,
            };
            found.push((rank, english.len(), english, korean));
        }
        found.sort_unstable_by(|a, b| a.0.cmp(&b.0).then(a.1.cmp(&b.1)).then(a.2.cmp(b.2)));
        found
            .into_iter()
            .take(limit)
            .map(|(_, _, english, korean)| Suggestion {
                used: english.to_string(),
                korean: korean.to_string(),
            })
            .collect()
    }

    /// Whether this is a tag hitomi uses, said in English.
    ///
    /// Worth asking because hitomi's tags are several words long - `big
    /// breasts`, `sole female` - and a query is otherwise cut at every space.
    /// Someone typing the tag they can see on a work was getting `big` and
    /// `breasts` looked up separately.
    pub fn knows_english(&self, phrase: &str) -> bool {
        self.korean.contains_key(&phrase.trim().to_lowercase())
    }

    /// What an English tag is called in Korean, where the dictionary knows.
    ///
    /// Namespaced tags keep their namespace: `female:big breasts` comes back
    /// as `female:거유`, because who a tag is about is drawn from that half.
    pub fn korean_for(&self, tag: &str) -> Option<String> {
        let (prefix, word) = match tag.split_once(':') {
            Some((ns, rest)) if matches!(ns, "female" | "male") => (Some(ns), rest),
            _ => (None, tag),
        };
        let said = self.korean.get(&word.trim().to_lowercase())?;
        Some(match prefix {
            Some(ns) => format!("{ns}:{said}"),
            None => said.clone(),
        })
    }
}

/// A word the reader might have meant: what hitomi calls it, and what to
/// show it as.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Suggestion {
    /// hitomi's own word, which is what a search is made of.
    pub used: String,
    /// The same thing in Korean, where the dictionary knows it.
    pub korean: String,
}

/// 입력의 한 조각과 그것이 무엇으로 바뀌었는지.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Term {
    /// 사용자가 친 그대로.
    pub input: String,
    /// 실제로 검색에 쓸 말.
    pub used: String,
    /// 다른 후보들. 비어 있으면 선택의 여지가 없다.
    pub alternatives: Vec<String>,
    /// 사전에서 찾았는가. `false`면 입력을 그대로 쓴 것이다.
    pub translated: bool,
    /// 제외 항목(`-`로 시작)인가.
    pub excluded: bool,
}

/// 질의를 낱말 단위로 훑으며 사전에 있는 것을 영어로 바꾼다.
///
/// 사전 항목에는 `기동전사 건담`처럼 띄어쓰기가 들어간 것이 많아, 공백으로
/// 자르면 찾지 못한다. 그래서 긴 구부터 맞춰본다.
pub fn translate(dictionary: &Dictionary, query: &str) -> Vec<Term> {
    let words: Vec<&str> = query.split_whitespace().collect();
    let mut out = Vec::new();
    let mut at = 0usize;

    while at < words.len() {
        let mut matched = None;
        let mut english = None;
        let longest = MAX_PHRASE_WORDS.min(words.len() - at);
        for take in (1..=longest).rev() {
            let phrase = words[at..at + take].join(" ");
            let (body, excluded) = split_exclusion(&phrase);
            if body.is_empty() {
                continue;
            }
            if let Some(candidates) = dictionary.lookup(body) {
                matched = Some((take, phrase.clone(), body.to_string(), excluded, candidates));
                break;
            }
            // Already hitomi's own word for it. Kept whole rather than cut at
            // its spaces, which is what made `big breasts` two searches.
            if take > 1 && dictionary.knows_english(body) {
                english = Some((take, phrase.clone(), body.to_string(), excluded));
                break;
            }
        }

        match matched {
            Some((take, input, _body, excluded, candidates)) => {
                out.push(Term {
                    input,
                    used: candidates[0].clone(),
                    alternatives: candidates[1..].to_vec(),
                    translated: true,
                    excluded,
                });
                at += take;
            }
            None => match english {
                Some((take, input, body, excluded)) => {
                    out.push(Term {
                        input,
                        used: body,
                        alternatives: Vec::new(),
                        translated: false,
                        excluded,
                    });
                    at += take;
                }
                None => {
                    let word = words[at];
                    let (body, excluded) = split_exclusion(word);
                    out.push(Term {
                        input: word.to_string(),
                        used: body.to_string(),
                        alternatives: Vec::new(),
                        translated: false,
                        excluded,
                    });
                    at += 1;
                }
            },
        }
    }

    out
}

/// 앞의 `-`를 떼어내고 제외 여부를 함께 돌려준다.
fn split_exclusion(word: &str) -> (&str, bool) {
    match word.strip_prefix('-') {
        Some(rest) if !rest.is_empty() => (rest, true),
        _ => (word, false),
    }
}

/// 보정된 낱말들을 다시 하나의 질의 문자열로 만든다.
///
/// 여러 낱말로 된 영어 검색어는 따옴표 없이도 그대로 넘길 수 없으므로, 검색
/// 쪽에서 낱말 목록을 직접 받는 편이 낫다. 이 함수는 로그와 표시용이다.
pub fn to_query_string(terms: &[Term]) -> String {
    terms
        .iter()
        .map(|t| if t.excluded { format!("-{}", t.used) } else { t.used.clone() })
        .collect::<Vec<_>>()
        .join(" ")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn dictionary() -> Dictionary {
        Dictionary::parse(
            "거유\tbig breasts\n\
             안경\tglasses\n\
             펠라치오\tblow job\tblowjob\n\
             기동전사 건담\tmobile suit gundam\tgundam seed\n",
        )
    }

    #[test]
    fn translates_a_known_word() {
        let terms = translate(&dictionary(), "거유");
        assert_eq!(terms.len(), 1);
        assert_eq!(terms[0].used, "big breasts");
        assert!(terms[0].translated);
        assert!(terms[0].alternatives.is_empty());
    }

    #[test]
    fn keeps_an_unknown_word_as_typed() {
        let terms = translate(&dictionary(), "시와스노오키나");
        assert_eq!(terms[0].used, "시와스노오키나");
        assert!(!terms[0].translated, "the UI must be able to say why this found nothing");
    }

    #[test]
    fn prefers_the_longest_matching_phrase() {
        // `기동전사`와 `건담`으로 잘리면 사전에 없어 번역이 안 된다.
        let terms = translate(&dictionary(), "기동전사 건담");
        assert_eq!(terms.len(), 1);
        assert_eq!(terms[0].used, "mobile suit gundam");
        assert_eq!(terms[0].alternatives, vec!["gundam seed"]);
    }

    #[test]
    fn mixes_translated_and_untranslated_words() {
        let terms = translate(&dictionary(), "거유 안경 zzz");
        assert_eq!(
            terms.iter().map(|t| t.used.as_str()).collect::<Vec<_>>(),
            vec!["big breasts", "glasses", "zzz"]
        );
        assert_eq!(terms.iter().map(|t| t.translated).collect::<Vec<_>>(), vec![true, true, false]);
    }

    #[test]
    fn carries_exclusions_through_translation() {
        let terms = translate(&dictionary(), "안경 -거유");
        assert!(!terms[0].excluded);
        assert!(terms[1].excluded);
        assert_eq!(terms[1].used, "big breasts");
        assert_eq!(to_query_string(&terms), "glasses -big breasts");
    }

    #[test]
    fn ambiguous_words_expose_their_alternatives() {
        let terms = translate(&dictionary(), "펠라치오");
        assert_eq!(terms[0].used, "blow job");
        assert_eq!(terms[0].alternatives, vec!["blowjob"]);
    }

    #[test]
    fn english_input_passes_through_untouched() {
        let terms = translate(&dictionary(), "glasses");
        assert_eq!(terms[0].used, "glasses");
        assert!(!terms[0].translated);
    }

    #[test]
    fn a_bare_dash_is_not_an_exclusion() {
        let terms = translate(&dictionary(), "-");
        assert!(!terms[0].excluded);
        assert_eq!(terms[0].used, "-");
    }

    #[test]
    fn the_embedded_dictionary_loads_and_covers_common_tags() {
        let dictionary = Dictionary::embedded();
        assert!(dictionary.len() > 5_000, "got {} entries", dictionary.len());
        assert_eq!(dictionary.lookup("거유").unwrap()[0], "big breasts");
        assert_eq!(dictionary.lookup("안경").unwrap()[0], "glasses");
        assert_eq!(dictionary.lookup("나루토").unwrap()[0], "naruto");
    }

    #[test]
    fn the_embedded_dictionary_has_no_namespace_prefixes() {
        // hitomi의 갤러리 인덱스는 `female:glasses`를 받지 않는다(실측).
        let dictionary = Dictionary::embedded();
        for candidates in dictionary.entries.values() {
            for candidate in candidates {
                assert!(
                    !candidate.starts_with("female:") && !candidate.starts_with("male:"),
                    "namespaced candidate would never match: {candidate}"
                );
            }
        }
    }

    /// The dictionary read backwards, for showing a tag rather than finding
    /// one.
    #[test]
    fn an_english_tag_can_be_said_in_korean() {
        let d = Dictionary::embedded();
        assert_eq!(d.korean_for("big breasts").as_deref(), Some("거유"));
        assert_eq!(d.korean_for("ahegao").as_deref(), Some("아헤가오"));
        assert_eq!(d.korean_for("nakadashi").as_deref(), Some("질내 사정"));
        // Asked however it is spelled.
        assert_eq!(d.korean_for("Big Breasts").as_deref(), Some("거유"));
        // Who it is about is kept, because the colour is drawn from it.
        assert_eq!(d.korean_for("female:big breasts").as_deref(), Some("female:거유"));
        assert_eq!(d.korean_for("male:sole male").as_deref(), Some("male:단독남성"));
        // A namespace the dictionary does not know is part of the word.
        assert_eq!(d.korean_for("artist:someone"), None);
        assert_eq!(d.korean_for("a tag nobody wrote down"), None);
    }

    /// hitomi's tags are several words long, so a query cut at every space
    /// looks for the wrong things.
    #[test]
    fn an_english_tag_is_not_cut_at_its_spaces() {
        let d = Dictionary::embedded();
        let terms = translate(d, "big breasts");
        assert_eq!(terms.len(), 1, "one tag, not two words: {terms:?}");
        assert_eq!(terms[0].used, "big breasts");

        // Beside another word it is still one of them.
        let terms = translate(d, "big breasts glasses");
        assert_eq!(
            terms.iter().map(|t| t.used.as_str()).collect::<Vec<_>>(),
            vec!["big breasts", "glasses"]
        );

        // And excluded the same way.
        let terms = translate(d, "-sole female");
        assert_eq!(terms.len(), 1);
        assert!(terms[0].excluded);
        assert_eq!(terms[0].used, "sole female");

        // Words nobody has written down are still one word each.
        let terms = translate(d, "zzz qqq");
        assert_eq!(terms.len(), 2);
    }

    /// What is offered while a word is being typed.
    #[test]
    fn suggestions_lead_with_what_was_typed() {
        let d = Dictionary::embedded();
        let found = d.suggest("big b", 10);
        assert!(!found.is_empty());
        assert!(
            found[0].used.starts_with("big b"),
            "a word that starts with it comes first: {found:?}"
        );
        assert!(found.iter().all(|s| s.used.contains("big b") || s.korean.contains("big b")));

        // Typed in Korean, answered with hitomi's word for it.
        let found = d.suggest("거유", 5);
        assert!(found.iter().any(|s| s.used == "big breasts"), "{found:?}");

        assert!(d.suggest("", 10).is_empty());
        assert!(d.suggest("big", 0).is_empty());
    }
}
