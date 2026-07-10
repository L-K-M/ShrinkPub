//! Shrink EPUB files by recompressing the images inside them.
//!
//! An EPUB is a zip container whose weight is almost entirely JPEG/PNG
//! payload. [`shrink_epub`] streams the container entry-by-entry, re-encodes
//! the images at the requested [`Quality`] tier, and writes a new EPUB next
//! to the original. Guarantees, in order of importance:
//!
//! - **The source file is never touched** and nothing is written anywhere
//!   except the single output file — no extraction directory, no temp files.
//!   Everything happens in memory, one entry at a time.
//! - **A failed shrink leaves no trace**: the partial output is removed.
//! - **Recompression can only help**: an image is replaced only when the
//!   re-encoded version is strictly smaller; otherwise the original bytes are
//!   copied through raw. Non-image entries are always copied bit-for-bit.
//! - **The output is spec-compliant**: the `mimetype` entry comes first and
//!   is stored uncompressed, as OCF requires — even when the source got that
//!   wrong.

mod image_codec;
mod quality;

pub use quality::Quality;

use std::fs::{self, File};
use std::io::{BufReader, BufWriter, Read, Write};
use std::path::{Path, PathBuf};

use zip::result::ZipError;
use zip::write::SimpleFileOptions;
use zip::{CompressionMethod, ZipArchive, ZipWriter};

/// Images larger than this are passed through without a recompression
/// attempt, so a pathological entry cannot balloon memory.
const MAX_IMAGE_BYTES: u64 = 256 * 1024 * 1024;

const EPUB_MIMETYPE: &str = "application/epub+zip";

/// Emitted once per zip entry as it is about to be processed.
#[derive(Debug)]
pub struct Progress<'a> {
    pub index: usize,
    pub total: usize,
    pub entry_name: &'a str,
}

/// What a completed shrink did.
#[derive(Debug, Clone)]
pub struct ShrinkReport {
    pub output_path: PathBuf,
    pub input_bytes: u64,
    pub output_bytes: u64,
    /// Images replaced by a smaller re-encoded version.
    pub images_recompressed: usize,
    /// Images kept as-is (re-encoding failed or was not smaller).
    pub images_kept: usize,
    pub entries_total: usize,
}

#[derive(Debug, thiserror::Error)]
pub enum ShrinkError {
    #[error("cannot read {}: {source}", path.display())]
    Read {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
    #[error("{} is not an EPUB: {reason}", path.display())]
    NotAnEpub { path: PathBuf, reason: String },
    #[error("cannot create output file {}: {source}", path.display())]
    CreateOutput {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
    #[error("error while writing {}: {source}", path.display())]
    Write {
        path: PathBuf,
        #[source]
        source: ZipError,
    },
}

/// True when `path` has the `.epub` extension (any case). Cheap pre-filter
/// for drag-and-drop; [`shrink_epub`] verifies the actual container anyway.
pub fn has_epub_extension(path: &Path) -> bool {
    path.extension()
        .is_some_and(|ext| ext.eq_ignore_ascii_case("epub"))
}

/// Shrink `input` at the given tier, writing `<name> (shrunk).epub` (or
/// `(shrunk 2)`, `(shrunk 3)`, … on collision) next to it. `progress` fires
/// once per container entry.
pub fn shrink_epub(
    input: &Path,
    quality: Quality,
    mut progress: impl FnMut(Progress<'_>),
) -> Result<ShrinkReport, ShrinkError> {
    let read_err = |source| ShrinkError::Read {
        path: input.to_path_buf(),
        source,
    };

    let input_bytes = fs::metadata(input).map_err(read_err)?.len();
    let file = File::open(input).map_err(read_err)?;
    let mut archive =
        ZipArchive::new(BufReader::new(file)).map_err(|e| ShrinkError::NotAnEpub {
            path: input.to_path_buf(),
            reason: match e {
                ZipError::Io(io) => return read_err(io),
                other => format!("not a zip container ({other})"),
            },
        })?;
    validate_epub(&mut archive, input)?;

    let (output_path, out_file) = create_output(input)?;
    let result =
        write_shrunk(&mut archive, out_file, quality, &mut progress).map_err(|source| {
            // Never leave a half-written book behind.
            let _ = fs::remove_file(&output_path);
            ShrinkError::Write {
                path: output_path.clone(),
                source,
            }
        })?;

    let output_bytes = fs::metadata(&output_path)
        .map_err(|source| ShrinkError::CreateOutput {
            path: output_path.clone(),
            source,
        })?
        .len();

    Ok(ShrinkReport {
        output_path,
        input_bytes,
        output_bytes,
        images_recompressed: result.images_recompressed,
        images_kept: result.images_kept,
        entries_total: archive.len(),
    })
}

/// An EPUB must either declare the OCF mimetype or at least carry the OCF
/// container manifest. Anything else is rejected before any output exists.
fn validate_epub<R: Read + std::io::Seek>(
    archive: &mut ZipArchive<R>,
    input: &Path,
) -> Result<(), ShrinkError> {
    let mimetype_ok = archive.by_name("mimetype").is_ok_and(|mut entry| {
        let mut declared = String::new();
        entry.read_to_string(&mut declared).is_ok() && declared.trim() == EPUB_MIMETYPE
    });
    if mimetype_ok || archive.by_name("META-INF/container.xml").is_ok() {
        return Ok(());
    }
    Err(ShrinkError::NotAnEpub {
        path: input.to_path_buf(),
        reason: "no EPUB mimetype or META-INF/container.xml entry".into(),
    })
}

/// Claim a collision-free sibling output file. `create_new` makes the
/// probe-and-claim atomic, so concurrent shrinks cannot race each other.
fn create_output(input: &Path) -> Result<(PathBuf, File), ShrinkError> {
    let dir = input.parent().unwrap_or_else(|| Path::new("."));
    let stem = input
        .file_stem()
        .and_then(|s| s.to_str())
        .filter(|s| !s.is_empty())
        .unwrap_or("book");

    for attempt in 1..10_000u32 {
        let name = if attempt == 1 {
            format!("{stem} (shrunk).epub")
        } else {
            format!("{stem} (shrunk {attempt}).epub")
        };
        let candidate = dir.join(name);
        match File::create_new(&candidate) {
            Ok(file) => return Ok((candidate, file)),
            Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => continue,
            Err(source) => {
                return Err(ShrinkError::CreateOutput {
                    path: candidate,
                    source,
                })
            }
        }
    }
    Err(ShrinkError::CreateOutput {
        path: dir.join(format!("{stem} (shrunk).epub")),
        source: std::io::Error::new(
            std::io::ErrorKind::AlreadyExists,
            "10000 shrunk copies already exist",
        ),
    })
}

struct WriteOutcome {
    images_recompressed: usize,
    images_kept: usize,
}

fn write_shrunk<R: Read + std::io::Seek>(
    archive: &mut ZipArchive<R>,
    out_file: File,
    quality: Quality,
    progress: &mut impl FnMut(Progress<'_>),
) -> Result<WriteOutcome, ZipError> {
    let mut writer = ZipWriter::new(BufWriter::new(out_file));
    let mut outcome = WriteOutcome {
        images_recompressed: 0,
        images_kept: 0,
    };

    // OCF 3.3 §3.3: first entry, named "mimetype", stored, no extra field.
    writer.start_file(
        "mimetype",
        SimpleFileOptions::default().compression_method(CompressionMethod::Stored),
    )?;
    writer.write_all(EPUB_MIMETYPE.as_bytes())?;

    let total = archive.len();
    for index in 0..total {
        let (name, is_image_file) = {
            let raw = archive.by_index_raw(index)?;
            (
                raw.name().to_string(),
                !raw.is_dir() && image_codec::image_kind(raw.name()).is_some(),
            )
        };
        progress(Progress {
            index,
            total,
            entry_name: &name,
        });

        if name == "mimetype" {
            continue; // already written, canonically
        }
        if !is_image_file {
            let raw = archive.by_index_raw(index)?;
            writer.raw_copy_file(raw)?;
            continue;
        }

        match read_entry(archive, index) {
            Some(original) => {
                let kind = image_codec::image_kind(&name).expect("checked above");
                let smaller = image_codec::recompress(kind, &original, quality)
                    .filter(|re| re.len() < original.len());
                match smaller {
                    Some(recompressed) => {
                        // Already-compressed payload: deflating again wastes
                        // CPU for ~0 gain, so store it.
                        writer.start_file(
                            &*name,
                            SimpleFileOptions::default()
                                .compression_method(CompressionMethod::Stored),
                        )?;
                        writer.write_all(&recompressed)?;
                        outcome.images_recompressed += 1;
                    }
                    None => {
                        let raw = archive.by_index_raw(index)?;
                        writer.raw_copy_file(raw)?;
                        outcome.images_kept += 1;
                    }
                }
            }
            // Unreadable (e.g. unsupported compression method) or oversized:
            // pass the raw bytes through untouched.
            None => {
                let raw = archive.by_index_raw(index)?;
                writer.raw_copy_file(raw)?;
                outcome.images_kept += 1;
            }
        }
    }

    writer.finish()?.flush()?;
    Ok(outcome)
}

fn read_entry<R: Read + std::io::Seek>(
    archive: &mut ZipArchive<R>,
    index: usize,
) -> Option<Vec<u8>> {
    let mut entry = archive.by_index(index).ok()?;
    if entry.size() > MAX_IMAGE_BYTES {
        return None;
    }
    let mut buf = Vec::with_capacity(entry.size() as usize);
    entry.read_to_end(&mut buf).ok()?;
    Some(buf)
}
