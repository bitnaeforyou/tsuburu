//! Windows backend: Windows.Media.Ocr reads the page, and the same imaging
//! stack decodes it.
//!
//! Both come with the operating system, as Vision does on macOS. AVIF is the
//! one gap: WIC only decodes it once the AV1 Video Extension is installed,
//! which is why a decode failure says so instead of blaming the page.

use crate::{Line, Ocr, OcrError, OcrOptions};
use windows::Globalization::Language;
use windows::Graphics::Imaging::{
    BitmapAlphaMode, BitmapDecoder, BitmapInterpolationMode, BitmapPixelFormat, BitmapTransform,
    ColorManagementMode, ExifOrientationMode, SoftwareBitmap,
};
use windows::Media::Ocr::OcrEngine;
use windows::Storage::Streams::{DataWriter, InMemoryRandomAccessStream};
use windows::core::HSTRING;

#[derive(Debug, Clone)]
pub struct WindowsOcr {
    options: OcrOptions,
}

impl WindowsOcr {
    pub fn new(options: OcrOptions) -> Self {
        Self { options }
    }

    /// The requested language if Windows has its model, else whatever the
    /// user's own languages give. Recognition is worse in the wrong
    /// language, but silence would be worse still.
    fn engine(&self) -> Result<OcrEngine, OcrError> {
        for tag in &self.options.languages {
            let Ok(language) = Language::CreateLanguage(&HSTRING::from(tag.as_str())) else {
                continue;
            };
            if let Ok(engine) = OcrEngine::TryCreateFromLanguage(&language) {
                return Ok(engine);
            }
        }
        OcrEngine::TryCreateFromUserProfileLanguages().map_err(|e| {
            OcrError::Recognize(format!(
                "Windows has no text recognition installed for {} ({e})",
                self.options.languages.join(", ")
            ))
        })
    }

    /// hitomi serves AVIF for almost every page, and WIC only reads one once
    /// the AV1 Video Extension is installed - a free download that is not
    /// there by default, and without which nothing on this machine could be
    /// read at all. Those are decoded here instead of being handed to
    /// Windows, so there is nothing to install.
    fn decode(&self, encoded: &[u8]) -> Result<SoftwareBitmap, OcrError> {
        if tsuburu_avif::is_avif(encoded) {
            return self.decode_avif(encoded);
        }
        self.decode_inner(encoded).map_err(|e| {
            tracing::debug!(detail = %e, "page could not be decoded");
            OcrError::Decode(crate::describe(encoded))
        })
    }

    /// The page as recognition wants it: one grey value per pixel, written out
    /// as the four-channel bitmap the engine takes.
    fn decode_avif(&self, encoded: &[u8]) -> Result<SoftwareBitmap, OcrError> {
        let page = tsuburu_avif::decode_luma(encoded, self.options.max_side)
            .map_err(|e| OcrError::Decode(e.to_string()))?;
        let mut bgra = Vec::with_capacity(page.pixels.len() * 4);
        for grey in page.pixels {
            bgra.extend_from_slice(&[grey, grey, grey, 0xff]);
        }
        self.bitmap_from_bgra(&bgra, page.width as i32, page.height as i32).map_err(|e| {
            tracing::debug!(detail = %e, "a decoded page could not be wrapped");
            OcrError::Decode("the page could not be handed to Windows".into())
        })
    }

    fn bitmap_from_bgra(
        &self,
        bgra: &[u8],
        width: i32,
        height: i32,
    ) -> windows::core::Result<SoftwareBitmap> {
        let writer = DataWriter::new()?;
        writer.WriteBytes(bgra)?;
        let buffer = writer.DetachBuffer()?;
        // Every pixel is opaque, so premultiplied and straight are the same
        // bytes; this is the format the engine is already given.
        SoftwareBitmap::CreateCopyFromBuffer(&buffer, BitmapPixelFormat::Bgra8, width, height)
    }

    fn decode_inner(&self, encoded: &[u8]) -> windows::core::Result<SoftwareBitmap> {
        let stream = InMemoryRandomAccessStream::new()?;
        let writer = DataWriter::CreateDataWriter(&stream)?;
        writer.WriteBytes(encoded)?;
        writer.StoreAsync()?.join()?;
        writer.FlushAsync()?.join()?;
        writer.DetachStream()?;
        stream.Seek(0)?;

        let decoder = BitmapDecoder::CreateAsync(&stream)?.join()?;
        let transform = BitmapTransform::new()?;
        let (width, height) = (decoder.PixelWidth()?, decoder.PixelHeight()?);
        let longest = width.max(height);
        if longest > self.options.max_side && longest > 0 {
            let scale = f64::from(self.options.max_side) / f64::from(longest);
            transform.SetScaledWidth((f64::from(width) * scale).round() as u32)?;
            transform.SetScaledHeight((f64::from(height) * scale).round() as u32)?;
            transform.SetInterpolationMode(BitmapInterpolationMode::Fant)?;
        }

        decoder
            .GetSoftwareBitmapTransformedAsync(
                BitmapPixelFormat::Bgra8,
                BitmapAlphaMode::Premultiplied,
                &transform,
                ExifOrientationMode::RespectExifOrientation,
                ColorManagementMode::DoNotColorManage,
            )?
            .join()
    }
}

impl Ocr for WindowsOcr {
    fn recognize(&self, encoded: &[u8]) -> Result<Vec<Line>, OcrError> {
        let bitmap = self.decode(encoded)?;
        let engine = self.engine()?;
        let recognize = |bitmap: &SoftwareBitmap| -> windows::core::Result<Vec<Line>> {
            let (width, height) = (bitmap.PixelWidth()? as f32, bitmap.PixelHeight()? as f32);
            let result = engine.RecognizeAsync(bitmap)?.join()?;
            let mut lines = Vec::new();
            for line in result.Lines()? {
                let text = line.Text()?.to_string_lossy();
                if text.trim().is_empty() {
                    continue;
                }
                // A line knows its words' boxes but not its own.
                let mut bounds: Option<(f32, f32, f32, f32)> = None;
                for word in line.Words()? {
                    let rect = word.BoundingRect()?;
                    let (left, top) = (rect.X, rect.Y);
                    let (right, bottom) = (rect.X + rect.Width, rect.Y + rect.Height);
                    bounds = Some(match bounds {
                        None => (left, top, right, bottom),
                        Some((l, t, r, b)) => {
                            (l.min(left), t.min(top), r.max(right), b.max(bottom))
                        }
                    });
                }
                let Some((left, top, right, bottom)) = bounds else { continue };
                if width <= 0.0 || height <= 0.0 {
                    continue;
                }
                lines.push(Line {
                    text,
                    // Windows reports no confidence, and the pipeline only
                    // ever compares it against a floor.
                    confidence: 1.0,
                    x: left / width,
                    y: top / height,
                    width: (right - left) / width,
                    height: (bottom - top) / height,
                });
            }
            Ok(lines)
        };
        recognize(&bitmap).map_err(|e| OcrError::Recognize(e.to_string()))
    }

    fn name(&self) -> &'static str {
        "windows"
    }
}
