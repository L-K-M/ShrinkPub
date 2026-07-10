//! End-to-end tests over the public API, driven by fixture EPUBs that are
//! generated in-test (no binary fixtures in the repo).

use std::fs;
use std::io::{Cursor, Read, Write};
use std::path::{Path, PathBuf};

use image::{DynamicImage, ImageFormat, RgbImage, RgbaImage};
use shrinkpub_core::{has_epub_extension, shrink_epub, Progress, Quality, ShrinkError};
use zip::write::SimpleFileOptions;
use zip::{CompressionMethod, ZipArchive, ZipWriter};

const CONTAINER_XML: &str = r#"<?xml version="1.0"?>
<container version="1.0" xmlns="urn:oasis:names:tc:opendocument:xmlns:container">
  <rootfiles><rootfile full-path="OEBPS/content.opf" media-type="application/oebps-package+xml"/></rootfiles>
</container>"#;

const CHAPTER_XHTML: &str = r#"<?xml version="1.0" encoding="utf-8"?>
<html xmlns="http://www.w3.org/1999/xhtml"><body><p>Bring out your EPUBs!</p></body></html>"#;

/// Deterministic photo-ish content: smooth gradients plus LCG noise, so JPEG
/// quality levels actually change the encoded size.
fn photo_like(width: u32, height: u32) -> RgbImage {
    let mut seed = 0x2545_f491u32;
    RgbImage::from_fn(width, height, |x, y| {
        seed = seed.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
        let noise = (seed >> 24) as u8;
        image::Rgb([
            ((x * 255) / width.max(1)) as u8,
            ((y * 255) / height.max(1)) as u8,
            noise,
        ])
    })
}

fn jpeg_bytes(img: &RgbImage, quality: u8) -> Vec<u8> {
    let mut out = Vec::new();
    image::codecs::jpeg::JpegEncoder::new_with_quality(&mut out, quality)
        .encode_image(img)
        .unwrap();
    out
}

/// Flat-color "chart" artwork — the typical PNG payload inside an EPUB, and
/// happily quantizable (~100 distinct colors).
fn chart_png_bytes(width: u32, height: u32) -> Vec<u8> {
    let img = RgbaImage::from_fn(width, height, |x, y| {
        if x % 80 < 2 || y % 80 < 2 {
            image::Rgba([0, 0, 0, 255])
        } else {
            let cell = (x / 80) + (y / 80) * 10;
            image::Rgba([
                ((cell * 37) % 200 + 30) as u8,
                ((cell * 73) % 200 + 30) as u8,
                ((cell * 151) % 200 + 30) as u8,
                255,
            ])
        }
    });
    let mut out = Vec::new();
    DynamicImage::ImageRgba8(img)
        .write_to(&mut Cursor::new(&mut out), ImageFormat::Png)
        .unwrap();
    out
}

/// Per-pixel RGB noise: impossible to palette-quantize at a high quality
/// floor, so high tiers must keep it as-is.
fn noise_png_bytes(width: u32, height: u32) -> Vec<u8> {
    let mut seed = 0x1234_5678u32;
    let img = RgbaImage::from_fn(width, height, |_, _| {
        seed = seed.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
        image::Rgba([
            (seed >> 8) as u8,
            (seed >> 16) as u8,
            (seed >> 24) as u8,
            255,
        ])
    });
    let mut out = Vec::new();
    DynamicImage::ImageRgba8(img)
        .write_to(&mut Cursor::new(&mut out), ImageFormat::Png)
        .unwrap();
    out
}

struct FixtureEntry {
    name: &'static str,
    data: Vec<u8>,
    method: CompressionMethod,
}

fn write_fixture(path: &Path, entries: &[FixtureEntry]) {
    let file = fs::File::create(path).unwrap();
    let mut zip = ZipWriter::new(file);
    for entry in entries {
        zip.start_file(
            entry.name,
            SimpleFileOptions::default().compression_method(entry.method),
        )
        .unwrap();
        zip.write_all(&entry.data).unwrap();
    }
    zip.finish().unwrap();
}

/// A well-formed EPUB: proper mimetype first, a chapter, a large JPEG and PNG.
fn standard_fixture(dir: &Path, file_name: &str) -> PathBuf {
    let path = dir.join(file_name);
    write_fixture(
        &path,
        &[
            FixtureEntry {
                name: "mimetype",
                data: b"application/epub+zip".to_vec(),
                method: CompressionMethod::Stored,
            },
            FixtureEntry {
                name: "META-INF/container.xml",
                data: CONTAINER_XML.as_bytes().to_vec(),
                method: CompressionMethod::Deflated,
            },
            FixtureEntry {
                name: "OEBPS/chapter1.xhtml",
                data: CHAPTER_XHTML.as_bytes().to_vec(),
                method: CompressionMethod::Deflated,
            },
            FixtureEntry {
                name: "OEBPS/images/photo.jpg",
                data: jpeg_bytes(&photo_like(1800, 1200), 95),
                method: CompressionMethod::Stored,
            },
            FixtureEntry {
                name: "OEBPS/images/chart.png",
                data: chart_png_bytes(800, 800),
                method: CompressionMethod::Stored,
            },
        ],
    );
    path
}

fn entry_bytes(epub: &Path, entry_name: &str) -> Vec<u8> {
    let mut archive = ZipArchive::new(fs::File::open(epub).unwrap()).unwrap();
    let mut entry = archive.by_name(entry_name).unwrap();
    let mut buf = Vec::new();
    entry.read_to_end(&mut buf).unwrap();
    buf
}

fn dir_entries(dir: &Path) -> Vec<String> {
    let mut names: Vec<String> = fs::read_dir(dir)
        .unwrap()
        .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
        .collect();
    names.sort();
    names
}

#[test]
fn shrinks_a_fixture_epub_without_leaving_anything_behind() {
    let dir = tempfile::tempdir().unwrap();
    let input = standard_fixture(dir.path(), "Fixture.epub");
    let input_before = fs::read(&input).unwrap();
    let dir_before = dir_entries(dir.path());

    let report = shrink_epub(&input, Quality::Medium, |_| {}).unwrap();

    // Exactly one new file appeared, named the friendly way.
    assert_eq!(report.output_path, dir.path().join("Fixture (shrunk).epub"));
    let mut expected = dir_before.clone();
    expected.push("Fixture (shrunk).epub".into());
    expected.sort();
    assert_eq!(dir_entries(dir.path()), expected, "no temp litter allowed");

    // The original is byte-for-byte untouched.
    assert_eq!(fs::read(&input).unwrap(), input_before);

    // Both images got smaller; the whole book got smaller.
    assert_eq!(report.images_recompressed, 2);
    assert_eq!(report.images_kept, 0);
    assert!(report.output_bytes < report.input_bytes);
    assert!(
        entry_bytes(&report.output_path, "OEBPS/images/photo.jpg").len()
            < entry_bytes(&input, "OEBPS/images/photo.jpg").len()
    );
    assert!(
        entry_bytes(&report.output_path, "OEBPS/images/chart.png").len()
            < entry_bytes(&input, "OEBPS/images/chart.png").len()
    );

    // Non-image entries are copied bit-for-bit.
    assert_eq!(
        entry_bytes(&report.output_path, "OEBPS/chapter1.xhtml"),
        CHAPTER_XHTML.as_bytes()
    );
    assert_eq!(
        entry_bytes(&report.output_path, "META-INF/container.xml"),
        CONTAINER_XML.as_bytes()
    );
}

#[test]
fn output_is_ocf_compliant_even_when_the_source_is_not() {
    let dir = tempfile::tempdir().unwrap();
    // Broken source: mimetype is deflated, padded, and not the first entry.
    let input = dir.path().join("Sloppy.epub");
    write_fixture(
        &input,
        &[
            FixtureEntry {
                name: "OEBPS/chapter1.xhtml",
                data: CHAPTER_XHTML.as_bytes().to_vec(),
                method: CompressionMethod::Deflated,
            },
            FixtureEntry {
                name: "mimetype",
                data: b"\napplication/epub+zip  \n".to_vec(),
                method: CompressionMethod::Deflated,
            },
        ],
    );

    let report = shrink_epub(&input, Quality::Medium, |_| {}).unwrap();

    let mut archive = ZipArchive::new(fs::File::open(&report.output_path).unwrap()).unwrap();
    let first = archive.by_index(0).unwrap();
    assert_eq!(first.name(), "mimetype");
    assert_eq!(first.compression(), CompressionMethod::Stored);
    drop(first);
    assert_eq!(
        entry_bytes(&report.output_path, "mimetype"),
        b"application/epub+zip"
    );
    // And only one mimetype entry exists.
    let names: Vec<String> = (0..archive.len())
        .map(|i| archive.by_index(i).unwrap().name().to_string())
        .collect();
    assert_eq!(names.iter().filter(|n| n.as_str() == "mimetype").count(), 1);
}

#[test]
fn images_that_would_grow_are_kept_byte_identical() {
    let dir = tempfile::tempdir().unwrap();
    let tiny = jpeg_bytes(&photo_like(4, 4), 5);
    let input = dir.path().join("Tiny.epub");
    write_fixture(
        &input,
        &[
            FixtureEntry {
                name: "mimetype",
                data: b"application/epub+zip".to_vec(),
                method: CompressionMethod::Stored,
            },
            FixtureEntry {
                name: "OEBPS/tiny.jpg",
                data: tiny.clone(),
                method: CompressionMethod::Stored,
            },
        ],
    );

    let report = shrink_epub(&input, Quality::VeryHigh, |_| {}).unwrap();

    assert_eq!(report.images_recompressed, 0);
    assert_eq!(report.images_kept, 1);
    assert_eq!(entry_bytes(&report.output_path, "OEBPS/tiny.jpg"), tiny);
}

#[test]
fn unquantizable_pngs_are_kept_at_high_tiers() {
    let dir = tempfile::tempdir().unwrap();
    let noise = noise_png_bytes(300, 300);
    let input = dir.path().join("Photo.epub");
    write_fixture(
        &input,
        &[
            FixtureEntry {
                name: "mimetype",
                data: b"application/epub+zip".to_vec(),
                method: CompressionMethod::Stored,
            },
            FixtureEntry {
                name: "OEBPS/photo.png",
                data: noise.clone(),
                method: CompressionMethod::Stored,
            },
        ],
    );

    let report = shrink_epub(&input, Quality::VeryHigh, |_| {}).unwrap();

    // Quantizing noise can't reach Very High's quality floor, so the engine
    // must keep the original bytes rather than butcher the image.
    assert_eq!(report.images_recompressed, 0);
    assert_eq!(report.images_kept, 1);
    assert_eq!(entry_bytes(&report.output_path, "OEBPS/photo.png"), noise);
}

#[test]
fn collisions_get_numbered_suffixes() {
    let dir = tempfile::tempdir().unwrap();
    let input = standard_fixture(dir.path(), "Book.epub");

    let first = shrink_epub(&input, Quality::Atrocious, |_| {}).unwrap();
    let second = shrink_epub(&input, Quality::Atrocious, |_| {}).unwrap();
    let third = shrink_epub(&input, Quality::Atrocious, |_| {}).unwrap();

    assert_eq!(first.output_path, dir.path().join("Book (shrunk).epub"));
    assert_eq!(second.output_path, dir.path().join("Book (shrunk 2).epub"));
    assert_eq!(third.output_path, dir.path().join("Book (shrunk 3).epub"));
}

#[test]
fn lower_tiers_produce_smaller_files() {
    let dir = tempfile::tempdir().unwrap();
    let mut sizes = Vec::new();
    for quality in [Quality::VeryHigh, Quality::Medium, Quality::Atrocious] {
        let sub = dir.path().join(quality.id());
        fs::create_dir(&sub).unwrap();
        let input = standard_fixture(&sub, "Book.epub");
        let report = shrink_epub(&input, quality, |_| {}).unwrap();
        sizes.push(report.output_bytes);
    }
    assert!(
        sizes[0] > sizes[1],
        "Medium must beat Very High ({sizes:?})"
    );
    assert!(
        sizes[1] > sizes[2],
        "Atrocious must beat Medium ({sizes:?})"
    );
}

#[test]
fn low_tiers_downscale_oversized_images() {
    let dir = tempfile::tempdir().unwrap();
    let input = dir.path().join("Wide.epub");
    write_fixture(
        &input,
        &[
            FixtureEntry {
                name: "mimetype",
                data: b"application/epub+zip".to_vec(),
                method: CompressionMethod::Stored,
            },
            FixtureEntry {
                name: "OEBPS/wide.jpg",
                data: jpeg_bytes(&photo_like(2000, 500), 95),
                method: CompressionMethod::Stored,
            },
        ],
    );

    let report = shrink_epub(&input, Quality::Low, |_| {}).unwrap();

    let out = image::load_from_memory(&entry_bytes(&report.output_path, "OEBPS/wide.jpg")).unwrap();
    assert_eq!(out.width(), 1600);
    assert_eq!(out.height(), 400, "aspect ratio must be preserved");

    // Medium and above never downscale.
    let untouched = shrink_epub(&input, Quality::Medium, |_| {}).unwrap();
    let out =
        image::load_from_memory(&entry_bytes(&untouched.output_path, "OEBPS/wide.jpg")).unwrap();
    assert_eq!(out.width(), 2000);
}

#[test]
fn rejects_non_epubs_and_creates_nothing() {
    let dir = tempfile::tempdir().unwrap();

    let not_zip = dir.path().join("NotAZip.epub");
    fs::write(&not_zip, "certainly not a zip archive").unwrap();
    let plain_zip = dir.path().join("PlainZip.epub");
    write_fixture(
        &plain_zip,
        &[FixtureEntry {
            name: "readme.txt",
            data: b"just a zip".to_vec(),
            method: CompressionMethod::Deflated,
        }],
    );
    let before = dir_entries(dir.path());

    for input in [&not_zip, &plain_zip] {
        match shrink_epub(input, Quality::Medium, |_| {}) {
            Err(ShrinkError::NotAnEpub { .. }) => {}
            other => panic!("expected NotAnEpub for {input:?}, got {other:?}"),
        }
    }
    assert_eq!(
        dir_entries(dir.path()),
        before,
        "rejection must not create files"
    );
}

#[test]
fn missing_input_reports_a_read_error() {
    match shrink_epub(Path::new("/nonexistent/Book.epub"), Quality::Medium, |_| {}) {
        Err(ShrinkError::Read { .. }) => {}
        other => panic!("expected Read error, got {other:?}"),
    }
}

#[test]
fn progress_covers_every_entry() {
    let dir = tempfile::tempdir().unwrap();
    let input = standard_fixture(dir.path(), "Book.epub");

    let mut seen: Vec<(usize, usize, String)> = Vec::new();
    let report = shrink_epub(&input, Quality::Medium, |p: Progress<'_>| {
        seen.push((p.index, p.total, p.entry_name.to_string()));
    })
    .unwrap();

    assert_eq!(seen.len(), report.entries_total);
    assert_eq!(seen.len(), 5);
    for (i, (index, total, _)) in seen.iter().enumerate() {
        assert_eq!(*index, i);
        assert_eq!(*total, 5);
    }
    assert!(seen
        .iter()
        .any(|(_, _, name)| name == "OEBPS/images/photo.jpg"));
}

#[test]
fn epub_extension_helper() {
    assert!(has_epub_extension(Path::new("a/b/Book.epub")));
    assert!(has_epub_extension(Path::new("BOOK.EPUB")));
    assert!(!has_epub_extension(Path::new("archive.zip")));
    assert!(!has_epub_extension(Path::new("epub")));
}
