//! Thin async bridge between the webview and `shrinkpub-core`.
//!
//! Commands stay small: validate the request, hand the real work to the core
//! crate on a blocking thread, stream progress back as events, and stringify
//! errors at the boundary (`Result<T, String>`), matching the sibling apps.

use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use shrinkpub_core::{has_epub_extension, shrink_epub, Quality};
use tauri::Emitter;

use crate::models::{PathInfo, ShrinkOutcome, ShrinkProgress, SystemColors};

/// Event carrying [`ShrinkProgress`] payloads while a job runs. Mirrored as
/// the same constant in `src/lib/tauri.ts`.
pub const SHRINK_PROGRESS_EVENT: &str = "shrinkpub://shrink-progress";

/// Entry-level progress can fire thousands of times per second on image-heavy
/// books; the UI only needs a smooth bar, so emissions are rate-limited.
const PROGRESS_INTERVAL: Duration = Duration::from_millis(60);

/// Describe a set of dropped/picked paths so the UI can build its file rows
/// (name, size, and whether it even looks like an EPUB) in one round-trip.
#[tauri::command]
pub fn inspect_paths(paths: Vec<String>) -> Vec<PathInfo> {
    paths
        .into_iter()
        .map(|raw| {
            let path = Path::new(&raw);
            let metadata = std::fs::metadata(path).ok();
            let is_dir = metadata.as_ref().is_some_and(|m| m.is_dir());
            PathInfo {
                name: path
                    .file_name()
                    .map(|n| n.to_string_lossy().into_owned())
                    .unwrap_or_else(|| raw.clone()),
                size_bytes: metadata.as_ref().map_or(0, |m| m.len()),
                is_dir,
                is_epub: !is_dir && has_epub_extension(path),
                path: raw,
            }
        })
        .collect()
}

/// Shrink one EPUB at the given quality tier. Runs on a blocking thread so
/// several files can crunch in parallel while the UI stays live; per-entry
/// progress is emitted as [`SHRINK_PROGRESS_EVENT`] tagged with `job_id`.
#[tauri::command]
pub async fn shrink_epub_file(
    app: tauri::AppHandle,
    job_id: String,
    path: String,
    quality: String,
) -> Result<ShrinkOutcome, String> {
    let tier =
        Quality::from_id(&quality).ok_or_else(|| format!("Unknown quality tier \"{quality}\""))?;
    let input = PathBuf::from(&path);

    tauri::async_runtime::spawn_blocking(move || {
        let mut last_emit: Option<Instant> = None;
        let report = shrink_epub(&input, tier, |progress| {
            let is_edge = progress.index == 0 || progress.index + 1 == progress.total;
            if is_edge || last_emit.is_none_or(|at| at.elapsed() >= PROGRESS_INTERVAL) {
                last_emit = Some(Instant::now());
                let _ = app.emit_to(
                    "main",
                    SHRINK_PROGRESS_EVENT,
                    ShrinkProgress {
                        job_id: job_id.clone(),
                        index: progress.index,
                        total: progress.total,
                        entry_name: progress.entry_name.to_string(),
                    },
                );
            }
        })
        .map_err(|error| error.to_string())?;

        Ok(ShrinkOutcome {
            output_path: report.output_path.to_string_lossy().into_owned(),
            input_bytes: report.input_bytes,
            output_bytes: report.output_bytes,
            images_recompressed: report.images_recompressed,
            images_kept: report.images_kept,
            entries_total: report.entries_total,
        })
    })
    .await
    .map_err(|error| error.to_string())?
}

/// OS accent/highlight colors for system7-ui window theming.
#[tauri::command]
pub fn get_system_colors() -> SystemColors {
    crate::system_colors::get_system_colors()
}
