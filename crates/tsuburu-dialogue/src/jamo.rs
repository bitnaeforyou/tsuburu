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

/// Match codes: one byte per jamo, ASCII alphanumerics lowercased, everything
/// else dropped.
///
/// Jamo take the bytes `0x80..=0xC3`, which never occur in ASCII, so a query
/// cannot match across a Korean/Latin boundary by accident. A syllable costs
/// 2–3 bytes instead of the 9 that its jamo take as UTF-8, which is what
/// keeps a full-corpus scan under a second. Text outside Hangul and ASCII
/// (kana, kanji) is not matchable through this stream; the corpus is Korean.
pub fn codes(text: &str) -> Vec<u8> {
    let mut out = Vec::with_capacity(text.len());
    codes_into(text, &mut out);
    out
}

pub fn codes_into(text: &str, out: &mut Vec<u8>) {
    for c in text.chars() {
        let code = c as u32;
        if (BASE..=LAST).contains(&code) {
            let index = code - BASE;
            let cho = index / (21 * 28);
            let jung = (index % (21 * 28)) / 28;
            let jong = index % 28;
            out.push(CODE_BASE + cho as u8);
            out.push(CODE_BASE + 19 + jung as u8);
            if jong != 0 {
                out.push(CODE_BASE + 19 + 21 + (jong as u8 - 1));
            }
        } else if let Some(code) = compat_jamo_code(c) {
            out.push(code);
        } else if c.is_ascii_alphanumeric() {
            out.push(c.to_ascii_lowercase() as u8);
        }
    }
}

const CODE_BASE: u8 = 0x80;

/// A bare compatibility jamo (typed, or from decomposition elsewhere) maps to
/// the same code its syllable position would: consonants as initials when
/// they can be, otherwise finals; vowels as medials.
fn compat_jamo_code(c: char) -> Option<u8> {
    if let Some(i) = CHO.iter().position(|&j| j == c) {
        return Some(CODE_BASE + i as u8);
    }
    if let Some(i) = JUNG.iter().position(|&j| j == c) {
        return Some(CODE_BASE + 19 + i as u8);
    }
    if let Some(i) = JONG.iter().position(|&j| j == c)
        && i > 0
    {
        return Some(CODE_BASE + 19 + 21 + (i as u8 - 1));
    }
    None
}

/// Lowercase, drop whitespace and punctuation, and split every Hangul
/// syllable into its jamo. Everything else passes through unchanged.
pub fn normalize(text: &str) -> String {
    let mut out = String::with_capacity(text.len() * 2);
    normalize_into(text, &mut out);
    out
}

/// Same as [`normalize`], appending into a reused buffer.
pub fn normalize_into(text: &str, out: &mut String) {
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
    fn codes_are_one_byte_per_jamo_and_ascii_safe() {
        let c = codes("한 A1");
        assert_eq!(c.len(), 5);
        assert!(c[..3].iter().all(|&b| (0x80..=0xC3).contains(&b)));
        assert_eq!(&c[3..], b"a1");
        // A bare consonant is read as an initial; only composed syllables
        // carry final-consonant codes.
        assert_eq!(codes("ㅎㅏ"), codes("하"));
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
