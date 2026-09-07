//! macOS backend: ImageIO decodes (and downsizes) the page, Vision reads it.
//!
//! Decoding through `CGImageSource` with a thumbnail size means the AVIF is
//! never expanded to full resolution, which is cheaper than decoding and
//! resizing separately.

use crate::{Line, Ocr, OcrError, OcrOptions};
use objc2::AnyThread;
use objc2::rc::Retained;
use objc2_core_foundation::{CFBoolean, CFData, CFDictionary, CFNumber, CFRetained, CFString, CFType};
use objc2_core_graphics::CGImage;
use objc2_foundation::{NSArray, NSDictionary, NSString};
use objc2_image_io::{
    CGImageSource, kCGImageSourceCreateThumbnailFromImageAlways, kCGImageSourceThumbnailMaxPixelSize,
};
use objc2_vision::{
    VNImageRequestHandler, VNRecognizeTextRequest, VNRequest, VNRequestTextRecognitionLevel,
};

#[derive(Debug, Clone)]
pub struct VisionOcr {
    options: OcrOptions,
}

impl VisionOcr {
    pub fn new(options: OcrOptions) -> Self {
        Self { options }
    }

    fn decode(&self, encoded: &[u8]) -> Result<CFRetained<CGImage>, OcrError> {
        let data = CFData::from_bytes(encoded);
        let source = unsafe { CGImageSource::with_data(&data, None) }.ok_or(OcrError::Decode)?;

        let keys: [&CFString; 2] = unsafe {
            [kCGImageSourceThumbnailMaxPixelSize, kCGImageSourceCreateThumbnailFromImageAlways]
        };
        let size = CFNumber::new_i32(self.options.max_side as i32);
        let always = CFBoolean::new(true);
        let values: [&CFType; 2] = [&size, always.as_ref()];
        let dictionary = CFDictionary::from_slices(&keys, &values);

        unsafe { source.thumbnail_at_index(0, Some(dictionary.as_opaque())) }.ok_or(OcrError::Decode)
    }
}

impl Ocr for VisionOcr {
    fn recognize(&self, encoded: &[u8]) -> Result<Vec<Line>, OcrError> {
        let image = self.decode(encoded)?;

        let request = VNRecognizeTextRequest::new();
        request.setRecognitionLevel(VNRequestTextRecognitionLevel::Accurate);
        request.setUsesLanguageCorrection(self.options.language_correction);
        let languages: Vec<Retained<NSString>> =
            self.options.languages.iter().map(|l| NSString::from_str(l)).collect();
        request.setRecognitionLanguages(&NSArray::from_retained_slice(&languages));

        let handler_options: Retained<NSDictionary<_, _>> = NSDictionary::new();
        let handler = unsafe {
            VNImageRequestHandler::initWithCGImage_options(
                VNImageRequestHandler::alloc(),
                &image,
                &handler_options,
            )
        };
        let requests: Retained<NSArray<VNRequest>> = NSArray::from_slice(&[&**request]);
        handler
            .performRequests_error(&requests)
            .map_err(|err| OcrError::Recognize(err.localizedDescription().to_string()))?;

        let mut lines = Vec::new();
        if let Some(results) = request.results() {
            for observation in results.iter() {
                let Some(best) = observation.topCandidates(1).iter().next() else { continue };
                let rect = unsafe { observation.boundingBox() };
                // Vision's origin is bottom-left; flip so y grows downwards.
                lines.push(Line {
                    text: best.string().to_string(),
                    confidence: best.confidence(),
                    x: rect.origin.x as f32,
                    y: (1.0 - rect.origin.y - rect.size.height) as f32,
                    width: rect.size.width as f32,
                    height: rect.size.height as f32,
                });
            }
        }
        Ok(lines)
    }

    fn name(&self) -> &'static str {
        "vision"
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Needs a real page: `TSUBURU_OCR_SAMPLE=/path/to/page.avif cargo test -- --ignored`.
    #[test]
    #[ignore = "requires a sample image on disk"]
    fn reads_korean_text_from_a_real_page() {
        let path = std::env::var("TSUBURU_OCR_SAMPLE").expect("TSUBURU_OCR_SAMPLE");
        let bytes = std::fs::read(path).unwrap();
        let lines = VisionOcr::new(OcrOptions::default()).recognize(&bytes).unwrap();
        assert!(!lines.is_empty());
        assert!(lines.iter().any(|l| l.text.chars().any(|c| ('가'..='힣').contains(&c))));
    }

    #[test]
    fn garbage_is_a_decode_error() {
        let err = VisionOcr::new(OcrOptions::default()).recognize(b"not an image").unwrap_err();
        assert!(matches!(err, OcrError::Decode));
    }
}
