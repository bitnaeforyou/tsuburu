//! Text recognition behind a platform-neutral trait.
//!
//! No model files are shipped. Each platform calls the OCR its operating
//! system already has: Vision on macOS, and nothing yet elsewhere. The
//! pipeline only ever sees [`Ocr`], so a backend can be added without
//! touching it.
//!
//! Measured on an M4 Pro against Korean manga pages: Vision's accurate mode
//! with language correction off, fed a 75% downscale, reaches about
//! 11 pages/s. More cores do not help because requests serialise on the
//! Neural Engine, and fast mode reads Hangul as Latin letters.

use std::fmt;

#[derive(Debug, thiserror::Error)]
pub enum OcrError {
    #[error("image could not be decoded")]
    Decode,
    #[error("recognition failed: {0}")]
    Recognize(String),
    #[error("text recognition is not available on this platform")]
    Unsupported,
}

/// One recognised line and where it sat on the page.
///
/// Coordinates are normalised to the page, with `y` growing downwards, so
/// lines can be sorted into reading order and grouped into bubbles later.
#[derive(Debug, Clone, PartialEq)]
pub struct Line {
    pub text: String,
    pub confidence: f32,
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
}

#[derive(Debug, Clone)]
pub struct OcrOptions {
    /// Longest side after decoding. 1125 is 75% of a typical 1500px page:
    /// 1.5x faster than full size for a 6% loss in recognised characters.
    pub max_side: u32,
    /// BCP-47 tags, most likely first.
    pub languages: Vec<String>,
    /// Language correction doubles the time and only prettifies output;
    /// matching does not need it.
    pub language_correction: bool,
}

impl Default for OcrOptions {
    fn default() -> Self {
        Self {
            max_side: 1125,
            languages: vec!["ko-KR".into()],
            language_correction: false,
        }
    }
}

pub trait Ocr: Send + Sync {
    /// Recognise text in an encoded image (AVIF, WebP, PNG, JPEG...).
    fn recognize(&self, encoded: &[u8]) -> Result<Vec<Line>, OcrError>;

    fn name(&self) -> &'static str;
}

impl fmt::Debug for dyn Ocr {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Ocr({})", self.name())
    }
}

#[cfg(target_os = "macos")]
mod vision;
#[cfg(target_os = "macos")]
pub use vision::VisionOcr;

/// The OCR this platform provides, if any.
pub fn platform_ocr(options: OcrOptions) -> Option<Box<dyn Ocr>> {
    #[cfg(target_os = "macos")]
    {
        Some(Box::new(VisionOcr::new(options)))
    }
    #[cfg(not(target_os = "macos"))]
    {
        let _ = options;
        None
    }
}

/// Returns canned lines. Lets the pipeline be tested without an OS OCR.
#[derive(Debug, Clone, Default)]
pub struct MockOcr {
    pub lines: Vec<Line>,
    pub fail: bool,
}

impl MockOcr {
    pub fn saying(texts: &[&str]) -> Self {
        let lines = texts
            .iter()
            .enumerate()
            .map(|(i, t)| Line {
                text: (*t).to_string(),
                confidence: 1.0,
                x: 0.1,
                y: 0.1 * i as f32,
                width: 0.3,
                height: 0.05,
            })
            .collect();
        Self { lines, fail: false }
    }
}

impl Ocr for MockOcr {
    fn recognize(&self, _encoded: &[u8]) -> Result<Vec<Line>, OcrError> {
        if self.fail {
            return Err(OcrError::Recognize("mock failure".into()));
        }
        Ok(self.lines.clone())
    }

    fn name(&self) -> &'static str {
        "mock"
    }
}

/// Puts lines into an order that keeps each speech bubble together.
///
/// Sorting by row alone interleaves bubbles that happen to sit at the same
/// height, which splits phrases and defeats exact matching. Instead, lines
/// are clustered into bubbles by proximity and overlap, read top to bottom
/// inside a bubble, and bubbles are read top to bottom then right to left.
pub fn reading_order(lines: &mut Vec<Line>) {
    if lines.len() < 2 {
        return;
    }
    let mut by_y: Vec<Line> = std::mem::take(lines);
    by_y.sort_by(|a, b| a.y.partial_cmp(&b.y).unwrap_or(std::cmp::Ordering::Equal));

    let mut bubbles: Vec<Vec<Line>> = Vec::new();
    for line in by_y {
        let joined = bubbles.iter_mut().find(|bubble| belongs(bubble, &line));
        match joined {
            Some(bubble) => bubble.push(line),
            None => bubbles.push(vec![line]),
        }
    }

    // Bubbles: those whose tops are close count as one row, read right to left.
    bubbles.sort_by(|a, b| {
        let (ta, tb) = (a[0].y, b[0].y);
        let row_a = (ta / 0.12).floor();
        let row_b = (tb / 0.12).floor();
        row_a
            .partial_cmp(&row_b)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then_with(|| centre_x(b).partial_cmp(&centre_x(a)).unwrap_or(std::cmp::Ordering::Equal))
    });

    for mut bubble in bubbles {
        bubble.sort_by(|a, b| a.y.partial_cmp(&b.y).unwrap_or(std::cmp::Ordering::Equal));
        lines.extend(bubble);
    }
}

/// A line joins a bubble when it sits just below the bubble's last line and
/// overlaps it horizontally.
fn belongs(bubble: &[Line], line: &Line) -> bool {
    let last = bubble.last().expect("bubbles are never empty");
    let gap = line.y - (last.y + last.height);
    let tolerance = last.height.max(line.height) * 1.2;
    if gap > tolerance {
        return false;
    }
    let left = line.x.max(last.x);
    let right = (line.x + line.width).min(last.x + last.width);
    let overlap = (right - left).max(0.0);
    overlap >= 0.3 * line.width.min(last.width)
}

fn centre_x(bubble: &[Line]) -> f32 {
    let sum: f32 = bubble.iter().map(|l| l.x + l.width / 2.0).sum();
    sum / bubble.len() as f32
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mock_returns_its_lines() {
        let ocr = MockOcr::saying(&["안녕", "하세요"]);
        let lines = ocr.recognize(b"anything").unwrap();
        assert_eq!(lines.iter().map(|l| l.text.as_str()).collect::<Vec<_>>(), ["안녕", "하세요"]);
    }

    #[test]
    fn mock_can_fail() {
        let ocr = MockOcr { lines: vec![], fail: true };
        assert!(matches!(ocr.recognize(b"x"), Err(OcrError::Recognize(_))));
    }

    fn mk(t: &str, x: f32, y: f32) -> Line {
        Line { text: t.into(), confidence: 1.0, x, y, width: 0.2, height: 0.04 }
    }

    #[test]
    fn lines_in_one_bubble_stay_together_and_in_order() {
        // Two bubbles side by side at the same height. Row sorting would
        // interleave them; bubble clustering must not.
        let mut lines = vec![
            mk("부르는 게", 0.6, 0.15),
            mk("좋겠어요", 0.6, 0.20),
            mk("엉망이", 0.1, 0.12),
            mk("구급차라도", 0.6, 0.10),
            mk("됐네", 0.1, 0.17),
        ];
        reading_order(&mut lines);
        let got: Vec<&str> = lines.iter().map(|l| l.text.as_str()).collect();
        // Right bubble first (right to left), each bubble top to bottom.
        assert_eq!(got, ["구급차라도", "부르는 게", "좋겠어요", "엉망이", "됐네"]);
    }

    #[test]
    fn a_bubble_further_down_comes_later() {
        let mut lines = vec![mk("lower", 0.7, 0.60), mk("upper", 0.2, 0.10)];
        reading_order(&mut lines);
        assert_eq!(lines.iter().map(|l| l.text.as_str()).collect::<Vec<_>>(), ["upper", "lower"]);
    }

    #[test]
    fn a_big_vertical_gap_starts_a_new_bubble() {
        let mut lines = vec![mk("b", 0.2, 0.40), mk("a", 0.2, 0.10)];
        reading_order(&mut lines);
        assert_eq!(lines.iter().map(|l| l.text.as_str()).collect::<Vec<_>>(), ["a", "b"]);
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn platform_ocr_exists_on_macos() {
        assert!(platform_ocr(OcrOptions::default()).is_some());
    }
}
