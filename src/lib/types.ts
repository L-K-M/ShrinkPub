// TypeScript mirrors of the Rust serde structs in src-tauri/src/models.rs.
// Field names stay snake_case — that is how they serialize.

export interface SystemColors {
  accent_color: string | null;
  accent_text_color: string | null;
  highlight_color: string | null;
  highlight_text_color: string | null;
}

export interface PathInfo {
  path: string;
  name: string;
  size_bytes: number;
  is_dir: boolean;
  is_epub: boolean;
}

export interface ShrinkProgress {
  job_id: string;
  index: number;
  total: number;
  entry_name: string;
}

export interface ShrinkOutcome {
  output_path: string;
  input_bytes: number;
  output_bytes: number;
  images_recompressed: number;
  images_kept: number;
  entries_total: number;
}

/** Mirror of `shrinkpub_core::Quality::id()`. */
export type QualityTier =
  | 'veryhigh'
  | 'high'
  | 'medium'
  | 'low'
  | 'verylow'
  | 'terrible'
  | 'atrocious';

/** One row in the UI's file list (frontend-only state). */
export interface ShrinkJob {
  id: string;
  path: string;
  name: string;
  sizeBytes: number;
  status: 'working' | 'done' | 'error';
  progress: ShrinkProgress | null;
  outcome: ShrinkOutcome | null;
  error: string | null;
}
