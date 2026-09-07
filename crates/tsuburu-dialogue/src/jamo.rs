//! Hangul normalisation for fuzzy matching.
//!
//! OCR gets a final consonant wrong far more often than it gets a whole
//! syllable wrong. Comparing decomposed jamo instead of syllables lets a
//! one-jamo error still match, at the cost of a slightly longer string.

const BASE: u32 = 0xAC00;
const LAST: u32 = 0xD7A3;
const CHO: [char; 19] = [
    'ㄱ', 'ㄲ', 'ㄴ', 'ㄷ', 'ㄸ', 'ㄹ', 'ㅁ', 'ㅂ', 'ㅃ', 'ㅅ', 'ㅆ', 'ㅇ', 'ㅈ', 'ㅉ', 'ㅊ', 'ㅋ', 'ㅌ',
    'ㅍ', 'ㅎ',
];
const JUNG: [char; 21] = [
    'ㅏ', 'ㅐ', 'ㅑ', 'ㅒ', 'ㅓ', 'ㅔ', 'ㅕ', 'ㅖ', 'ㅗ', 'ㅘ', 'ㅙ', 'ㅚ', 'ㅛ', 'ㅜ', 'ㅝ', 'ㅞ', 'ㅟ',
    'ㅠ', 'ㅡ', 'ㅢ', 'ㅣ',
];
const JONG: [char; 28] = [
    '\0', 'ㄱ', 'ㄲ', 'ㄳ', 'ㄴ', 'ㄵ', 'ㄶ', 'ㄷ', 'ㄹ', 'ㄺ', 'ㄻ', 'ㄼ', 'ㄽ', 'ㄾ', 'ㄿ', 'ㅀ', 'ㅁ',
    'ㅂ', 'ㅄ', 'ㅅ', 'ㅆ', 'ㅇ', 'ㅈ', 'ㅊ', 'ㅋ', 'ㅌ', 'ㅍ', 'ㅎ',
];

/// Lowercase, drop whitespace and punctuation, and split every Hangul
/// syllable into its jamo. Everything else passes through unchanged.
pub fn normalize(text: &str) -> String {
    let mut out = String::with_capacity(text.len() * 2);
    for c in text.chars() {
        let code = c as u32;
        if (BASE..=LAST).contains(&code) {
            let index = code - BASE;
            let cho = index / (21 * 28);
            let jung = (index % (21 * 28)) / 28;
            let jong = index % 28;
            out.push(CHO[cho as usize]);
            out.push(JUNG[jung as usize]);
            if jong != 0 {
                out.push(JONG[jong as usize]);
            }
        } else if c.is_alphanumeric() || ('ㄱ'..='ㅣ').contains(&c) {
            out.extend(c.to_lowercase());
        }
        // Whitespace and punctuation carry nothing worth matching on.
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn decomposes_syllables() {
        assert_eq!(normalize("한"), "ㅎㅏㄴ");
        assert_eq!(normalize("가"), "ㄱㅏ");
        assert_eq!(normalize("닭"), "ㄷㅏㄺ");
    }

    #[test]
    fn strips_whitespace_and_punctuation_and_lowercases() {
        assert_eq!(normalize("Hello, 세계!"), "helloㅅㅔㄱㅖ");
        assert_eq!(normalize("좋겠어요."), normalize("좋겠어요"));
    }

    #[test]
    fn a_final_consonant_error_differs_by_one_jamo() {
        let right = normalize("부르는");
        let wrong = normalize("부르눈");
        assert_eq!(right.chars().count(), wrong.chars().count());
        let diff = right.chars().zip(wrong.chars()).filter(|(a, b)| a != b).count();
        assert_eq!(diff, 1);
    }
}
