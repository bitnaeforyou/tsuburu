//! Reading an AVIF page without asking the operating system to.
//!
//! hitomi serves almost every page as AVIF. macOS decodes one out of the box
//! and Windows does not: WIC hands it to the AV1 Video Extension, which is a
//! free download that is not there by default, and without it every page fails
//! to decode and nothing can be read at all.
//!
//! Only the luma plane comes back. Recognition reads grey, so the chroma
//! planes are work nobody uses - and a page is one still frame, so the decoder
//! is opened, asked once, and closed.

use std::ptr::NonNull;

use rav1d::include::dav1d::data::Dav1dData;
use rav1d::include::dav1d::dav1d::{Dav1dContext, Dav1dSettings};
use rav1d::include::dav1d::picture::Dav1dPicture;
use rav1d::src::lib::{
    dav1d_close, dav1d_data_create, dav1d_default_settings, dav1d_get_picture, dav1d_open,
    dav1d_picture_unref, dav1d_send_data,
};

#[derive(Debug, thiserror::Error, PartialEq)]
pub enum AvifError {
    #[error("not an AVIF file")]
    NotAvif,
    #[error("the AVIF container could not be read: {0}")]
    Container(String),
    #[error("the AV1 image could not be decoded: {0}")]
    Decode(String),
    #[error("the image has no size")]
    Empty,
}

/// One page, as the grey an OCR reads.
#[derive(Debug, Clone, PartialEq)]
pub struct Grey {
    pub width: usize,
    pub height: usize,
    /// `width * height` bytes, row by row, no padding.
    pub pixels: Vec<u8>,
}

/// Whether these bytes are an AVIF, by the brand in the file's `ftyp` box.
pub fn is_avif(encoded: &[u8]) -> bool {
    encoded.len() >= 12 && &encoded[4..8] == b"ftyp" && matches!(&encoded[8..12], b"avif" | b"avis")
}

/// Decodes the page and shrinks it so its longest side is at most `max_side`.
///
/// Shrinking here rather than after recognition is the same bargain the other
/// backends make: a page is around 1400x2000, and the text is still legible at
/// a fraction of that while the recognition costs a fraction as much.
pub fn decode_luma(encoded: &[u8], max_side: u32) -> Result<Grey, AvifError> {
    if !is_avif(encoded) {
        return Err(AvifError::NotAvif);
    }
    let avif = avif_parse::read_avif(&mut &encoded[..])
        .map_err(|e| AvifError::Container(e.to_string()))?;
    let full = decode_av1(avif.primary_item.as_ref())?;
    Ok(shrink(full, max_side))
}

fn decode_av1(payload: &[u8]) -> Result<Grey, AvifError> {
    if payload.is_empty() {
        return Err(AvifError::Decode("the file carries no image".into()));
    }
    // SAFETY: every pointer below is to a local this function owns for the
    // whole call, and each rav1d handle is released before returning. The
    // buffer handed to `dav1d_send_data` is one rav1d allocated itself, so
    // there is no free callback to get wrong.
    unsafe {
        let mut settings: Dav1dSettings = std::mem::zeroed();
        dav1d_default_settings(NonNull::from(&mut settings));
        // A still frame, decoded once: threads and frame delay would only buy
        // latency on a sequence there isn't one of.
        settings.n_threads = 1;
        settings.max_frame_delay = 1;

        let mut context: Option<Dav1dContext> = None;
        if dav1d_open(NonNull::new(&mut context), Some(NonNull::from(&mut settings))).0 < 0 {
            return Err(AvifError::Decode("the decoder would not open".into()));
        }
        let decoded = decode_frame(context, payload);
        dav1d_close(NonNull::new(&mut context));
        decoded
    }
}

/// # Safety
///
/// `context` must be an open handle from `dav1d_open` that the caller closes.
unsafe fn decode_frame(context: Option<Dav1dContext>, payload: &[u8]) -> Result<Grey, AvifError> {
    unsafe {
        let mut data: Dav1dData = std::mem::zeroed();
        let room = dav1d_data_create(NonNull::new(&mut data), payload.len());
        if room.is_null() {
            return Err(AvifError::Decode("no room for the image".into()));
        }
        std::ptr::copy_nonoverlapping(payload.as_ptr(), room, payload.len());
        if dav1d_send_data(context, NonNull::new(&mut data)).0 < 0 {
            return Err(AvifError::Decode("the image was refused".into()));
        }

        let mut picture: Dav1dPicture = std::mem::zeroed();
        if dav1d_get_picture(context, NonNull::new(&mut picture)).0 < 0 {
            return Err(AvifError::Decode("no frame came back".into()));
        }
        let grey = read_luma(&picture);
        dav1d_picture_unref(NonNull::new(&mut picture));
        grey
    }
}

/// # Safety
///
/// `picture` must be a frame rav1d has filled in and not yet unreferenced.
unsafe fn read_luma(picture: &Dav1dPicture) -> Result<Grey, AvifError> {
    let (width, height) = (picture.p.w as usize, picture.p.h as usize);
    if width == 0 || height == 0 {
        return Err(AvifError::Empty);
    }
    // Ten-bit pages exist; this build of rav1d decodes eight.
    if picture.p.bpc != 8 {
        return Err(AvifError::Decode(format!("{} bits per channel", picture.p.bpc)));
    }
    let Some(plane) = picture.data[0] else {
        return Err(AvifError::Decode("the frame has no luma".into()));
    };
    let stride = picture.stride[0] as usize;
    let base = plane.as_ptr() as *const u8;
    let mut pixels = Vec::with_capacity(width * height);
    for row in 0..height {
        // SAFETY: rav1d guarantees `height` rows of at least `width` bytes,
        // `stride` apart.
        pixels.extend_from_slice(unsafe {
            std::slice::from_raw_parts(base.add(row * stride), width)
        });
    }
    Ok(Grey { width, height, pixels })
}

/// Averages whole blocks rather than sampling one pixel from each, because
/// dropping pixels out of a page of small text drops the strokes that make one
/// letter a different letter.
fn shrink(image: Grey, max_side: u32) -> Grey {
    let longest = image.width.max(image.height);
    let max_side = max_side as usize;
    if max_side == 0 || longest <= max_side {
        return image;
    }
    let width = (image.width * max_side / longest).max(1);
    let height = (image.height * max_side / longest).max(1);
    let mut pixels = Vec::with_capacity(width * height);
    for row in 0..height {
        let from_y = row * image.height / height;
        let to_y = (((row + 1) * image.height).div_ceil(height)).min(image.height).max(from_y + 1);
        for column in 0..width {
            let from_x = column * image.width / width;
            let to_x =
                (((column + 1) * image.width).div_ceil(width)).min(image.width).max(from_x + 1);
            let mut total = 0u32;
            let mut counted = 0u32;
            for y in from_y..to_y {
                let line = &image.pixels[y * image.width..][from_x..to_x];
                total += line.iter().map(|&p| u32::from(p)).sum::<u32>();
                counted += line.len() as u32;
            }
            pixels.push((total / counted.max(1)) as u8);
        }
    }
    Grey { width, height, pixels }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ftyp(brand: &[u8; 4]) -> Vec<u8> {
        let mut bytes = vec![0, 0, 0, 0x20];
        bytes.extend_from_slice(b"ftyp");
        bytes.extend_from_slice(brand);
        bytes.resize(64, 0);
        bytes
    }

    #[test]
    fn knows_an_avif_from_its_brand() {
        assert!(is_avif(&ftyp(b"avif")));
        assert!(is_avif(&ftyp(b"avis")));
        assert!(!is_avif(&ftyp(b"mif1")));
        assert!(!is_avif(b"\x89PNG\r\n\x1a\n"));
        assert!(!is_avif(b""));
    }

    #[test]
    fn a_page_that_is_not_one_is_turned_away_before_the_decoder() {
        assert_eq!(decode_luma(b"not a page", 1125), Err(AvifError::NotAvif));
    }

    /// The brand is right and the boxes are not: the container says so rather
    /// than the decoder being handed nothing.
    #[test]
    fn a_truncated_container_says_which_half_failed() {
        let err = decode_luma(&ftyp(b"avif"), 1125).unwrap_err();
        assert!(matches!(err, AvifError::Container(_)), "{err:?}");
    }

    fn ramp(width: usize, height: usize) -> Grey {
        let pixels = (0..width * height).map(|i| (i % 256) as u8).collect();
        Grey { width, height, pixels }
    }

    #[test]
    fn a_page_small_enough_already_is_left_alone() {
        let image = ramp(80, 100);
        assert_eq!(shrink(image.clone(), 1125), image);
    }

    #[test]
    fn shrinking_keeps_the_shape_of_the_page() {
        let small = shrink(ramp(1414, 2000), 1125);
        assert_eq!(small.height, 1125);
        assert_eq!(small.width, 795);
        assert_eq!(small.pixels.len(), 795 * 1125);
    }

    /// Averaging, not sampling: a black line one pixel wide has to leave a mark
    /// on the smaller page instead of falling between two samples.
    #[test]
    fn a_thin_line_survives_being_shrunk() {
        let mut image = ramp(100, 100);
        image.pixels.iter_mut().for_each(|p| *p = 255);
        for y in 0..100 {
            image.pixels[y * 100 + 49] = 0;
        }
        let small = shrink(image, 10);
        let darkest = small.pixels.iter().copied().min().unwrap();
        assert!(darkest < 255, "the line is gone");
    }

    /// Needs a real page: `TSUBURU_AVIF_SAMPLE=/path/to/page.avif cargo test -- --ignored`.
    #[test]
    #[ignore = "requires a sample page on disk"]
    fn reads_a_real_page() {
        let path = std::env::var("TSUBURU_AVIF_SAMPLE").expect("TSUBURU_AVIF_SAMPLE");
        let bytes = std::fs::read(path).unwrap();
        let page = decode_luma(&bytes, 1125).unwrap();
        assert!(page.width > 0 && page.height > 0);
        assert_eq!(page.pixels.len(), page.width * page.height);
        assert!(page.width.max(page.height) <= 1125);
        // A page of comic art is neither all black nor all white.
        let darkest = page.pixels.iter().copied().min().unwrap();
        let lightest = page.pixels.iter().copied().max().unwrap();
        assert!(lightest - darkest > 64, "{darkest}..{lightest} is not a page");
    }
}
