import { invoke } from '@tauri-apps/api/core';
import { listen, type UnlistenFn } from '@tauri-apps/api/event';
import type {
  PathInfo,
  QualityTier,
  ShrinkOutcome,
  ShrinkProgress,
  SystemColors
} from './types';

// Mirror of `commands::SHRINK_PROGRESS_EVENT` in src-tauri/src/commands.rs.
const SHRINK_PROGRESS_EVENT = 'shrinkpub://shrink-progress';

/** Typed wrapper around the Rust commands — one static method per command. */
export class TauriService {
  static async inspectPaths(paths: string[]): Promise<PathInfo[]> {
    return await invoke('inspect_paths', { paths });
  }

  static async shrinkEpubFile(
    jobId: string,
    path: string,
    quality: QualityTier
  ): Promise<ShrinkOutcome> {
    return await invoke('shrink_epub_file', { jobId, path, quality });
  }

  static async listenShrinkProgress(
    handler: (payload: ShrinkProgress) => void
  ): Promise<UnlistenFn> {
    return await listen<ShrinkProgress>(SHRINK_PROGRESS_EVENT, (event) => {
      handler(event.payload);
    });
  }

  static async getSystemColors(): Promise<SystemColors> {
    return await invoke('get_system_colors');
  }
}
