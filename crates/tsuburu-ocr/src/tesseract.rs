//! Unix backend: a system converter decodes the page and tesseract reads it.
//!
//! Neither is bundled. Linux has no OCR of its own, and hitomi serves AVIF,
//! which leptonica - and so tesseract - cannot read, so a decoder has to sit
//! in front of it. Calling tools the distribution already packages is the
//! same arrangement `import-meta` uses for `sqlite3`, and it keeps the
//! binary free of a C toolchain.

use crate::{Line, Ocr, OcrError, OcrOptions};
use std::io::Write;
use std::process::{Command, Stdio};

/// How a converter hands the page back: on its stdout, or at a second path.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Style {
    Stdout,
    Files,
}

#[derive(Debug, Clone, Copy)]
struct Converter {
    program: &'static str,
    style: Style,
}

/// Tried in order.
const CONVERTERS: [Converter; 4] = [
    Converter { program: "ffmpeg", style: Style::Stdout },
    Converter { program: "magick", style: Style::Stdout },
    Converter { program: "convert", style: Style::Stdout },
    Converter { program: "avifdec", style: Style::Files },
];

fn on_path(program: &str) -> bool {
    Command::new(program)
        .arg("--version")
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .is_ok()
}

/// Names of the tools this platform is missing, in the order to install them.
pub fn missing() -> Vec<&'static str> {
    let mut out = Vec::new();
    if !on_path("tesseract") {
        out.push("tesseract");
    }
    if !CONVERTERS.iter().any(|c| on_path(c.program)) {
        out.push("ffmpeg");
    }
    out
}

/// A path that removes itself. The converters need real files.
struct Scratch {
    path: std::path::PathBuf,
}

impl Scratch {
    fn new(extension: &str) -> Self {
        static COUNTER: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
        let serial = COUNTER.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        let name = format!("tsuburu-{}-{serial}.{extension}", std::process::id());
        Self { path: std::env::temp_dir().join(name) }
    }
}

impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.path);
    }
}

#[derive(Debug, Clone)]
pub struct TesseractOcr {
    options: OcrOptions,
    converter: Converter,
}

impl TesseractOcr {
    /// `None` when tesseract or every converter is absent.
    pub fn new(options: OcrOptions) -> Option<Self> {
        if !missing().is_empty() {
            return None;
        }
        let converter = CONVERTERS.iter().copied().find(|c| on_path(c.program))?;
        tracing::debug!(decoder = converter.program, "text recognition through tesseract");
        Some(Self { options, converter })
    }

    /// AVIF is an ISOBMFF container and its decoders seek inside it, so the
    /// page has to arrive as a file; a pipe truncates it.
    fn to_png(&self, encoded: &[u8]) -> Result<Vec<u8>, OcrError> {
        let program = self.converter.program;
        let input = Scratch::new("page");
        std::fs::write(&input.path, encoded).map_err(|e| decode(program, &e.to_string()))?;
        let source = input.path.to_string_lossy().to_string();
        let max = self.options.max_side;

        match self.converter.style {
            Style::Stdout if program == "ffmpeg" => capture(
                program,
                &[
                    "-v",
                    "error",
                    "-i",
                    &source,
                    "-vf",
                    // The quotes are ffmpeg's own: they keep the comma out of
                    // the filtergraph parser. -2 keeps the aspect ratio.
                    &format!("scale=w='min({max},iw)':h=-2"),
                    "-frames:v",
                    "1",
                    "-f",
                    "image2",
                    "-c:v",
                    "png",
                    "pipe:1",
                ],
            ),
            Style::Stdout => {
                capture(program, &[&source, "-resize", &format!("{max}x{max}>"), "png:-"])
            }
            Style::Files => {
                let output = Scratch::new("png");
                let target = output.path.to_string_lossy().to_string();
                capture(program, &[&source, &target])?;
                std::fs::read(&output.path).map_err(|e| decode(program, &e.to_string()))
            }
        }
    }
}

impl Ocr for TesseractOcr {
    fn recognize(&self, encoded: &[u8]) -> Result<Vec<Line>, OcrError> {
        let png = self.to_png(encoded)?;
        let (width, height) = png_size(&png).ok_or(OcrError::Decode)?;
        let languages = tesseract_languages(&self.options.languages);
        // Sparse text (11) beats a uniform block on speech bubbles, and the
        // page is not enlarged: measured against Vision's reading of a Korean
        // page, 1125px with psm 11 found 7 of 8 words, psm 6 found four, and
        // both fell off above 2000px.
        let tsv =
            run("tesseract", &["stdin", "stdout", "-l", &languages, "--psm", "11", "tsv"], &png)?;
        let tsv = String::from_utf8_lossy(&tsv);
        Ok(parse_tsv(&tsv, width as f32, height as f32))
    }

    fn name(&self) -> &'static str {
        "tesseract"
    }
}

fn decode(program: &str, detail: &str) -> OcrError {
    tracing::debug!(program, detail, "page could not be decoded");
    OcrError::Decode
}

fn run(program: &str, args: &[&str], input: &[u8]) -> Result<Vec<u8>, OcrError> {
    let mut child = Command::new(program)
        .args(args)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| OcrError::Recognize(format!("{program} could not be run: {e}")))?;

    // The child writes while we write, so feeding it from this thread would
    // deadlock on a page of any size.
    let mut stdin = child
        .stdin
        .take()
        .ok_or_else(|| OcrError::Recognize(format!("{program} closed its input")))?;
    let bytes = input.to_vec();
    let feeder = std::thread::spawn(move || stdin.write_all(&bytes));

    let output = child
        .wait_with_output()
        .map_err(|e| OcrError::Recognize(format!("{program} failed: {e}")))?;
    let _ = feeder.join();

    if !output.status.success() {
        let detail = String::from_utf8_lossy(&output.stderr);
        let detail = detail.lines().last().unwrap_or("no output").to_string();
        return Err(if program == "tesseract" {
            OcrError::Recognize(format!("tesseract failed: {detail}"))
        } else {
            decode(program, &detail)
        });
    }
    Ok(output.stdout)
}

/// Runs a converter over paths and returns whatever it put on stdout.
fn capture(program: &str, args: &[&str]) -> Result<Vec<u8>, OcrError> {
    let output = Command::new(program)
        .args(args)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output()
        .map_err(|e| OcrError::Recognize(format!("{program} could not be run: {e}")))?;
    if !output.status.success() {
        let detail = String::from_utf8_lossy(&output.stderr);
        return Err(decode(program, detail.lines().last().unwrap_or("no output")));
    }
    Ok(output.stdout)
}

/// Width and height out of a PNG's IHDR, which is always the first chunk.
fn png_size(png: &[u8]) -> Option<(u32, u32)> {
    if png.len() < 24 || &png[..8] != b"\x89PNG\r\n\x1a\n" || &png[12..16] != b"IHDR" {
        return None;
    }
    let read =
        |at: usize| -> u32 { u32::from_be_bytes([png[at], png[at + 1], png[at + 2], png[at + 3]]) };
    Some((read(16), read(20)))
}

/// Tesseract names its models after ISO 639-2, not BCP-47.
fn tesseract_languages(tags: &[String]) -> String {
    let mut out: Vec<&str> = Vec::new();
    for tag in tags {
        let name = match tag.to_lowercase().as_str() {
            "ko-kr" | "ko" => "kor",
            "ja-jp" | "ja" => "jpn",
            "zh-hans" | "zh-cn" => "chi_sim",
            "zh-hant" | "zh-tw" => "chi_tra",
            "es-es" | "es" => "spa",
            "fr-fr" | "fr" => "fra",
            "de-de" | "de" => "deu",
            "it-it" | "it" => "ita",
            "pt-br" | "pt" => "por",
            "ru-ru" | "ru" => "rus",
            "th-th" | "th" => "tha",
            "vi-vt" | "vi-vn" | "vi" => "vie",
            "id-id" | "id" => "ind",
            "pl-pl" | "pl" => "pol",
            "tr-tr" | "tr" => "tur",
            "cs-cz" | "cs" => "ces",
            "nl-nl" | "nl" => "nld",
            "uk-ua" | "uk" => "ukr",
            _ => "eng",
        };
        if !out.contains(&name) {
            out.push(name);
        }
    }
    if out.is_empty() {
        out.push("eng");
    }
    out.join("+")
}

/// Words out of tesseract's TSV, gathered back into the lines they came from.
///
/// Every row carries the block, paragraph and line it belongs to, so the
/// grouping is given rather than guessed; the box is the union of the words'.
fn parse_tsv(tsv: &str, width: f32, height: f32) -> Vec<Line> {
    if width <= 0.0 || height <= 0.0 {
        return Vec::new();
    }
    struct Pending {
        words: Vec<String>,
        confidence: f32,
        left: f32,
        top: f32,
        right: f32,
        bottom: f32,
    }

    let mut order: Vec<(u32, u32, u32)> = Vec::new();
    let mut lines: std::collections::HashMap<(u32, u32, u32), Pending> = Default::default();

    for row in tsv.lines().skip(1) {
        let field: Vec<&str> = row.split('\t').collect();
        if field.len() < 12 || field[0] != "5" {
            continue;
        }
        let text = field[11].trim();
        if text.is_empty() {
            continue;
        }
        let number = |at: usize| field[at].parse::<f32>().ok();
        let (Some(block), Some(paragraph), Some(line)) = (
            field[2].parse::<u32>().ok(),
            field[3].parse::<u32>().ok(),
            field[4].parse::<u32>().ok(),
        ) else {
            continue;
        };
        let (Some(left), Some(top), Some(w), Some(h), Some(confidence)) =
            (number(6), number(7), number(8), number(9), number(10))
        else {
            continue;
        };
        if confidence < 0.0 {
            continue;
        }

        let key = (block, paragraph, line);
        match lines.get_mut(&key) {
            Some(pending) => {
                pending.words.push(text.to_string());
                pending.confidence += confidence;
                pending.left = pending.left.min(left);
                pending.top = pending.top.min(top);
                pending.right = pending.right.max(left + w);
                pending.bottom = pending.bottom.max(top + h);
            }
            None => {
                order.push(key);
                lines.insert(
                    key,
                    Pending {
                        words: vec![text.to_string()],
                        confidence,
                        left,
                        top,
                        right: left + w,
                        bottom: top + h,
                    },
                );
            }
        }
    }

    order
        .into_iter()
        .filter_map(|key| lines.remove(&key))
        .map(|pending| Line {
            confidence: pending.confidence / pending.words.len() as f32 / 100.0,
            text: pending.words.join(" "),
            x: pending.left / width,
            y: pending.top / height,
            width: (pending.right - pending.left) / width,
            height: (pending.bottom - pending.top) / height,
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn language_tags_become_model_names() {
        assert_eq!(tesseract_languages(&["ko-KR".into()]), "kor");
        assert_eq!(tesseract_languages(&["zh-Hans".into(), "zh-Hant".into()]), "chi_sim+chi_tra");
        // Nothing is refused; an unknown tag reads as Latin.
        assert_eq!(tesseract_languages(&["kli-KL".into()]), "eng");
        assert_eq!(tesseract_languages(&[]), "eng");
    }

    #[test]
    fn duplicate_tags_are_asked_for_once() {
        assert_eq!(tesseract_languages(&["ja-JP".into(), "ja".into()]), "jpn");
    }

    #[test]
    fn missing_only_ever_names_the_two_tools() {
        // Whichever machine runs this, the answer is a subset of these.
        for name in missing() {
            assert!(["tesseract", "ffmpeg"].contains(&name), "unexpected: {name}");
        }
    }

    #[test]
    fn png_dimensions_come_from_the_header() {
        let mut png = b"\x89PNG\r\n\x1a\n".to_vec();
        png.extend_from_slice(&13u32.to_be_bytes());
        png.extend_from_slice(b"IHDR");
        png.extend_from_slice(&1125u32.to_be_bytes());
        png.extend_from_slice(&1600u32.to_be_bytes());
        assert_eq!(png_size(&png), Some((1125, 1600)));
        assert_eq!(png_size(b"not a png"), None);
    }

    #[test]
    fn words_are_gathered_into_the_lines_they_came_from() {
        let tsv = "level\tpage_num\tblock_num\tpar_num\tline_num\tword_num\tleft\ttop\twidth\theight\tconf\ttext\n\
             5\t1\t1\t1\t1\t1\t10\t20\t30\t10\t90\t나\n\
             5\t1\t1\t1\t1\t2\t50\t20\t40\t12\t80\t역시\n\
             4\t1\t1\t1\t1\t0\t0\t0\t0\t0\t-1\t\n\
             5\t1\t2\t1\t1\t1\t10\t60\t20\t10\t70\t여보\n";
        let lines = parse_tsv(tsv, 100.0, 100.0);
        assert_eq!(lines.len(), 2);
        assert_eq!(lines[0].text, "나 역시");
        assert!((lines[0].confidence - 0.85).abs() < 1e-5);
        // The box spans both words: left 10 to right 90, top 20 to bottom 32.
        assert!((lines[0].x - 0.10).abs() < 1e-5);
        assert!((lines[0].width - 0.80).abs() < 1e-5);
        assert!((lines[0].height - 0.12).abs() < 1e-5);
        assert_eq!(lines[1].text, "여보");
    }

    /// Needs the tools and a real page:
    /// `TSUBURU_OCR_SAMPLE=/path/to/page.avif cargo test -- --ignored`.
    #[test]
    #[ignore = "requires tesseract, a converter and a sample image"]
    fn reads_korean_text_from_a_real_page() {
        let path = std::env::var("TSUBURU_OCR_SAMPLE").expect("TSUBURU_OCR_SAMPLE");
        let bytes = std::fs::read(path).unwrap();
        let ocr = TesseractOcr::new(OcrOptions::default()).expect("tesseract and a converter");
        let lines = ocr.recognize(&bytes).unwrap();
        assert!(!lines.is_empty());
        assert!(lines.iter().any(|l| l.text.chars().any(|c| ('가'..='힣').contains(&c))));
    }

    #[test]
    #[ignore = "requires tesseract and a converter"]
    fn garbage_is_a_decode_error() {
        let ocr = TesseractOcr::new(OcrOptions::default()).expect("tesseract and a converter");
        assert!(matches!(ocr.recognize(b"not an image").unwrap_err(), OcrError::Decode));
    }

    #[test]
    fn a_page_with_no_words_reads_as_nothing() {
        assert!(parse_tsv("level\tpage\n", 100.0, 100.0).is_empty());
        assert!(parse_tsv("", 0.0, 0.0).is_empty());
    }
}
