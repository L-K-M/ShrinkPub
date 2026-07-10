//! Re-encoding of the image payloads found inside an EPUB.
//!
//! Everything here is fallible-by-design: any image that cannot be decoded,
//! quantized, or re-encoded is simply kept as-is by the caller. A broken
//! image must never abort the shrink of a whole book.

use image::codecs::jpeg::JpegEncoder;
use image::imageops::FilterType;
use image::{DynamicImage, ImageFormat, ImageReader, Limits};
use std::io::Cursor;

use crate::Quality;

/// Image payloads we know how to recompress, detected from the entry name.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum ImageKind {
    Jpeg,
    Png,
}

pub(crate) fn image_kind(entry_name: &str) -> Option<ImageKind> {
    let ext = entry_name.rsplit('.').next()?;
    match ext.to_ascii_lowercase().as_str() {
        "jpg" | "jpeg" => Some(ImageKind::Jpeg),
        "png" => Some(ImageKind::Png),
        _ => None,
    }
}

/// Re-encode `data` at the given tier. Returns `None` whenever the original
/// bytes should be kept instead: undecodable data, a format that does not
/// match the entry's extension (a PNG masquerading as `.jpg` would corrupt
/// the book's manifest media-types if transcoded), or any encode failure.
/// The caller additionally discards results that are not strictly smaller.
pub(crate) fn recompress(kind: ImageKind, data: &[u8], quality: Quality) -> Option<Vec<u8>> {
    let mut reader = ImageReader::new(Cursor::new(data))
        .with_guessed_format()
        .ok()?;
    let expected = match kind {
        ImageKind::Jpeg => ImageFormat::Jpeg,
        ImageKind::Png => ImageFormat::Png,
    };
    if reader.format() != Some(expected) {
        return None;
    }

    // A malformed dimension header must not be able to OOM the app.
    let mut limits = Limits::default();
    limits.max_image_width = Some(30_000);
    limits.max_image_height = Some(30_000);
    reader.limits(limits);

    let mut img = reader.decode().ok()?;
    if let Some(max_width) = quality.max_width() {
        if img.width() > max_width {
            img = img.resize(max_width, u32::MAX, FilterType::Lanczos3);
        }
    }

    match kind {
        ImageKind::Jpeg => encode_jpeg(&img, quality.jpeg_quality()),
        ImageKind::Png => encode_quantized_png(&img, quality.png_quality()),
    }
}

fn encode_jpeg(img: &DynamicImage, jpeg_quality: u8) -> Option<Vec<u8>> {
    // JPEG has no alpha; to_rgb8 drops it (EPUB JPEGs never carry meaningful
    // alpha anyway). Re-encoding also strips metadata, which is intended.
    let rgb = img.to_rgb8();
    let mut out = Vec::new();
    JpegEncoder::new_with_quality(&mut out, jpeg_quality)
        .encode_image(&rgb)
        .ok()?;
    Some(out)
}

fn encode_quantized_png(img: &DynamicImage, (q_min, q_max): (u8, u8)) -> Option<Vec<u8>> {
    let rgba = img.to_rgba8();
    let (width, height) = (rgba.width() as usize, rgba.height() as usize);
    let pixels: Vec<imagequant::RGBA> = rgba
        .as_raw()
        .chunks_exact(4)
        .map(|p| imagequant::RGBA::new(p[0], p[1], p[2], p[3]))
        .collect();

    // A target below 100 makes imagequant *degrade toward the target*, and it
    // tightens the q_min floor 3× for images that already fit a 256-color
    // palette — so charts/line art (the common EPUB PNG) can fail the floor
    // even though they'd convert to an indexed palette losslessly. Retrying
    // at target 100 turns those into the optimal palette conversion (also the
    // smaller file: fewer colors mean more dithering noise, not fewer bytes),
    // while photos that truly can't meet the floor still fail both attempts
    // and are kept as-is by the caller.
    let (palette, indexed) = quantize(&pixels, width, height, q_min, q_max)
        .or_else(|| quantize(&pixels, width, height, q_min, 100))?;

    let mut encoder = lodepng::Encoder::new();
    encoder.set_palette(&palette).ok()?;
    encoder.encode(&indexed, width, height).ok()
}

type Quantized = (Vec<imagequant::RGBA>, Vec<u8>);

fn quantize(
    pixels: &[imagequant::RGBA],
    width: usize,
    height: usize,
    q_min: u8,
    q_max: u8,
) -> Option<Quantized> {
    let mut attr = imagequant::new();
    attr.set_quality(q_min, q_max).ok()?;
    let mut liq_img = attr.new_image(pixels, width, height, 0.0).ok()?;
    let mut quantized = attr.quantize(&mut liq_img).ok()?;
    quantized.set_dithering_level(1.0).ok()?;
    quantized.remapped(&mut liq_img).ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn kind_is_detected_from_extension_case_insensitively() {
        assert_eq!(image_kind("OEBPS/img/a.JPG"), Some(ImageKind::Jpeg));
        assert_eq!(image_kind("a.jpeg"), Some(ImageKind::Jpeg));
        assert_eq!(image_kind("cover.PNG"), Some(ImageKind::Png));
        assert_eq!(image_kind("chapter1.xhtml"), None);
        assert_eq!(image_kind("noextension"), None);
    }

    #[test]
    fn mismatched_extension_is_left_alone() {
        // PNG bytes in a file the manifest calls a JPEG: must not transcode.
        let png = {
            let img = DynamicImage::new_rgb8(8, 8);
            let mut out = Vec::new();
            img.write_to(&mut Cursor::new(&mut out), ImageFormat::Png)
                .unwrap();
            out
        };
        assert!(recompress(ImageKind::Jpeg, &png, Quality::Medium).is_none());
        assert!(recompress(ImageKind::Png, &png, Quality::Medium).is_some());
    }

    #[test]
    fn garbage_is_left_alone() {
        assert!(recompress(ImageKind::Jpeg, b"not an image", Quality::Medium).is_none());
        assert!(recompress(ImageKind::Png, &[], Quality::Medium).is_none());
    }
}
