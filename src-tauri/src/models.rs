//! Serde structs shared with the frontend. Field names serialize with the
//! default snake_case and are mirrored 1:1 in `src/lib/types.ts`.

use serde::Serialize;

/// OS accent/highlight colors for system7-ui theming (macOS only; every field
/// is `None` on other platforms and the UI falls back to its defaults).
#[derive(Serialize, Clone)]
pub struct SystemColors {
    pub accent_color: Option<String>,
    pub accent_text_color: Option<String>,
    pub highlight_color: Option<String>,
    pub highlight_text_color: Option<String>,
}

/// What the UI needs to show for a dropped/picked path before shrinking it.
#[derive(Serialize, Clone)]
pub struct PathInfo {
    pub path: String,
    pub name: String,
    pub size_bytes: u64,
    pub is_dir: bool,
    pub is_epub: bool,
}

/// Per-entry progress of a running shrink job, keyed by the frontend's job id.
#[derive(Serialize, Clone)]
pub struct ShrinkProgress {
    pub job_id: String,
    pub index: usize,
    pub total: usize,
    pub entry_name: String,
}

/// Result of a completed shrink job (mirror of `shrinkpub_core::ShrinkReport`).
#[derive(Serialize, Clone)]
pub struct ShrinkOutcome {
    pub output_path: String,
    pub input_bytes: u64,
    pub output_bytes: u64,
    pub images_recompressed: usize,
    pub images_kept: usize,
    pub entries_total: usize,
}
