# ShrinkPub — Agent/Contributor Notes

This document is the implementation handoff and must be kept in sync with the
code. When behavior changes, also update README.md (and MODERNIZATION.md when
the architecture shifts).

## Product Direction

ShrinkPub makes EPUB files smaller. Drop `.epub` files onto the window (or pick
them with the file dialog) and each gets a `Book (shrunk).epub` sibling next to
the original. Books start at the default quality tier (set in the Settings
panel); every row has its own quality selector, and changing it shrinks that
book again at the new tier. The tool is deliberately boring and safe:

- The original file is never modified or deleted.
- An existing file is never overwritten (collision-suffixed output names).
- A recompressed image is only used when it is *strictly smaller*; anything the
  engine can't improve (or can't decode) passes through byte-for-byte.
- No temp files, no extraction directories — the repack happens in memory.
  Output streams into a `<name>.epub.part` sibling renamed into place on
  success, so even a killed process never leaves a file that looks finished.
- The output container is OCF-compliant (`mimetype` first, stored) even when
  the source wasn't.

## Quality Gates

```bash
npm run verify   # svelte-check + cargo test --workspace — the pre-commit gate
npm run check    # svelte-kit sync + svelte-check only
npm test         # cargo test --workspace only
```

`.husky/pre-commit` runs `npm run verify`. `npm run build` chains `verify`
before `vite build`, so a production frontend build always passes the gates
first. CI (`.github/workflows/ci.yml`) additionally enforces
`cargo fmt --all --check` and `cargo clippy --workspace --all-targets -- -D warnings`;
run those locally before pushing. See CICD.md for the workflow details.

## Architecture

```
crates/shrinkpub-core/   Pure-Rust engine (no Tauri types): EPUB in → EPUB out
crates/shrinkpub-cli/    `shrinkpub` binary over the same engine
src-tauri/               Tauri 2 shell: thin commands, progress events, updates
src/                     SvelteKit (Svelte 5, static adapter, ssr=false) frontend
```

Pipeline inside the engine (`crates/shrinkpub-core/src/lib.rs`):
open zip → validate it's an EPUB (mimetype or META-INF/container.xml) →
claim a collision-free `<stem> (shrunk[ N]).epub` slot (the bytes go to a
`.part` sibling claimed atomically with `create_new`) → write canonical
`mimetype` first (stored) → stream every other entry: images
(`.jpg/.jpeg/.png` by name) are decoded/re-encoded in memory
(`image_codec.rs`) and used only if smaller, everything else is
`raw_copy_file`d bit-for-bit → rename the `.part` to the final name → on any
write error the partial file is deleted instead.

## Core API (`shrinkpub-core`)

- `shrink_epub(input: &Path, quality: Quality, progress: impl FnMut(Progress)) -> Result<ShrinkReport, ShrinkError>`
- `Quality` — 7 tiers (`veryhigh` … `atrocious`), each mapping to a JPEG
  quality, an imagequant (min, max) range, and an optional max width
  (`quality.rs`). `Quality::from_id`/`id()` are the stable IPC names.
- `ShrinkReport { output_path, input_bytes, output_bytes, images_recompressed, images_kept, entries_total }`
- `has_epub_extension(path)` — cheap pre-filter for drag-and-drop.
- Module map: `lib.rs` (pipeline + errors), `quality.rs` (tiers),
  `image_codec.rs` (JPEG re-encode, PNG quantization via imagequant+lodepng).

## CLI Contract (`crates/shrinkpub-cli`)

`shrinkpub [--quality <tier>] [--verbose] <EPUB>...` — shrinks each file,
prints one summary line per book. Exit codes: `0` all succeeded, `1` at least
one file failed, `2` bad arguments (clap).

## Desktop App Contract (`src-tauri` + `src`)

Commands (all `Result<T, String>` at the boundary, registered in `lib.rs`):
- `inspect_paths(paths: Vec<String>) -> Vec<PathInfo>` — name/size/is_dir/is_epub
  for dropped or picked paths.
- `shrink_epub_file(job_id, path, quality) -> ShrinkOutcome` — runs the engine
  on a blocking thread; several jobs may run concurrently.
- `get_system_colors() -> SystemColors` — macOS accent colors for theming.
- `updates::check_self_update` / `updates::open_release_url` — the family's
  drop-in GitHub release notice (see below).

Events: `shrinkpub://shrink-progress` with
`{ job_id, index, total, entry_name }`, throttled Rust-side to ~60 ms (plus
first/last entry). The event name constant lives in `commands.rs` and is
mirrored in `src/lib/tauri.ts`.

Frontend structure: all UI in `src/routes/+page.svelte` (Svelte 5 runes);
`src/lib/tauri.ts` (`TauriService`, one typed static method per command);
`src/lib/types.ts` (TS mirrors of `models.rs`, snake_case fields);
`src/lib/windowManager.ts` (custom title-bar window controls);
`src/lib/updateChecker.ts` + `components/UpdateNotice.svelte` (update notice);
`src/lib/util/` (notifications re-export, byte/percent formatting).
localStorage keys are namespaced `shrinkpub.*` (currently `shrinkpub.quality` —
the default tier applied to newly added books).

## system7-ui Integration

UI components come from `@lkmc/system7-ui` (npm registry, `^0.2.1` — same form
as the sibling apps). `+layout.svelte` imports `@lkmc/system7-ui/styles.css`;
the page wraps everything in `.window-frame.s7-root` themed via
`getSystem7WindowStyle(systemColors)`. Window chrome is the custom `TitleBar`
(the Tauri window has `decorations: false`); reference apps for the patterns:
`../Lantenna`, `../Baegun`.

## Updates

No Tauri updater plugin. `src-tauri/src/updates.rs` (OWNER/REPO consts) checks
GitHub's latest release once a day (frontend-throttled), shows a dismissible
notice, and only ever opens the release page in the browser. The three files
involved are designed to be copied between sibling apps unchanged.

## Testing and Validation

- Engine: `crates/shrinkpub-core/tests/shrink.rs` builds fixture EPUBs in-test
  (no binary fixtures) and asserts the §Product guarantees: no litter, byte-safe
  originals, OCF-compliant output, keep-if-smaller, collision naming, tier
  ordering, downscaling, EPUB rejection, progress coverage.
- Unit tests live inline (`#[cfg(test)] mod tests`) in `quality.rs`,
  `image_codec.rs`, `updates.rs`, and the CLI.
- Everything runs with one `cargo test --workspace` (frontend must be built
  once first — `npx vite build` — because `tauri::generate_context!` embeds
  `build/`).

## Releases

`scripts/release.sh X.Y.Z [--push]` — a stub over the shared `lkm-release`
engine (https://github.com/L-K-M/release-tool), `RELEASE_KIND=tauri` with
`RELEASE_CARGO_TOMLS="Cargo.toml"` (the workspace manifest is the Rust version
source; member crates inherit it). The version lives in `package.json`,
`src-tauri/tauri.conf.json`, the workspace `Cargo.toml`, and the README marker
— always bump through the script so they stay in lockstep. Pushing the `vX.Y.Z`
tag triggers `release.yml` (see CICD.md).

## Dependency release-age policy

`.npmrc` sets `min-release-age=10`: npm refuses packages published in the last
10 days (supply-chain guard). If an urgent fix is blocked by it, prefer waiting
or pinning an older version; only use `--min-release-age=0` deliberately and
never in CI.
