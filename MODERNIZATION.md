# ShrinkPub Modernization

This document is the full audit of the legacy Electron app (as of commit `49175d3`)
and the plan for its replacement. The rebuild ships in the same repo; the legacy
sources it describes are removed by the rewrite, so file references below point at
the pre-rewrite tree.

## 1. What the app does (and must keep doing)

Drop `.epub` files onto the window, pick a quality tier, get a smaller `.epub`
written next to the original. EPUBs are zip containers; virtually all of their
weight is images, so "shrinking" means recompressing the JPEG/PNG payload and
repacking. The old app's contract worth preserving:

- The original file is never modified or deleted.
- Output lands next to the input, never overwriting an existing file.
- Quality tiers from "Very High" down to "Atrocious" (lower tiers also downscale
  oversized images).
- Per-file status in the UI: pending → working → success (with old/new size) or error.

## 2. Findings: why the old app is beyond repair

### 2.1 Platform rot — it no longer runs at all

- **Electron 9 (May 2020, EOL since late 2020).** Chromium 83 with years of
  unpatched CVEs. It doesn't launch on current macOS/Windows/Linux toolchains, and
  `npm install` of the pinned tree fails on modern Node.
- **`File.path` is gone.** The whole drop pipeline hinges on Electron's non-standard
  `File.path` property (`preload.js`, `renderer.js`). Chromium removed it; Electron
  ≥ 32 requires `webUtils.getPathForFile()`. Even a straight Electron upgrade would
  break the app's core mechanism.
- **Native-binary image deps are abandoned and unbuildable.** `imagemin-jpeg-recompress`
  (archived; its `jpeg-recompress-bin` ships x86 binaries that don't exist for Apple
  Silicon and segfault-or-404 on modern glibc), `imagemin-pngquant@8`, `decompress-zip`
  (last release 2016, known zip-slip class issues), `compressing@1.5`, `gulp@4` — all
  unmaintained, several with published advisories. The app's fate was tied to
  prebuilt 2019 binaries.
- **Undeclared runtime dependencies.** `preload.js` requires `pngcrush-bin`, `gulp`,
  `gulp-image`, `gulp-debug` at runtime, but `pngcrush-bin` is not in `package.json`
  at all and the gulp trio sit in `devDependencies` — a packaged build would crash on
  first `require`. (It was never packaged: the only script is `npm start`.)

### 2.2 Outright bugs

- **`main.js:30`** — `mainWindow.handleDropOnAppIcon(process.argv)` is called on the
  `BrowserWindow` object, but the function only ever existed on the renderer's
  `window`. `process.argv.length >= 2` is always true, so every launch throws a
  `TypeError` inside `app.whenReady()`.
- **`preload.js:26-36` (`handleDropOnAppIcon`)** — passes an `fs.Stats` object where
  a `File` is expected. `stat.name`, `stat.path`, `stat.type` don't exist, so
  open-with/drop-on-icon could never have worked.
- **`preload.js:144-150` (`crushPNG`)** — references `f`, which is not in scope →
  `ReferenceError` if ever called (it's only reachable from commented-out code).
- **`preload.js:281-288`** — `await fs.writeFile(...)` on the callback API: the
  `await` is a no-op, the write races the subsequent `imagemin` read, and `throw
  error` inside the callback is uncatchable.
- **`preload.js:307-323` (`compressJPG2`)** — uses the imagemin v5 call signature
  (`imagemin(files, dest, opts)`) against imagemin v7, so the "second attempt when
  the first grew the file" always throws; the catch above it silently eats it. Net
  effect: JPEGs that got *bigger* are shipped anyway (see below).
- **No "keep the smaller one" guarantee.** Recompressed images overwrite the
  extracted originals in place even when the result is larger than the input
  (aggressive recompress of already-optimized images routinely grows files).
- **`preload.js:271/303`** — `preSize` is shadowed/re-read after the file was already
  overwritten, so the logged "compressed from X to Y" numbers are wrong.
- **MIME-type gate is platform-dependent.** `f.file.type == "application/epub+zip"`
  (`preload.js:45`) relies on the OS MIME registry; on most Windows machines `.epub`
  reports an empty type and every valid EPUB is rejected with "I'm no Epub yet!".
- **`renderer.js:89` / `preload.js:70`** — sizes computed as `parseInt(size/1000)/1000`
  and labelled "Mb": wrong math (truncation), wrong unit (that's MB, decimal), and
  wrong by ~5% vs the MiB users expect.
- **`style.css:10`** — `overflow:scroll-y` is not a CSS property.

### 2.3 The temp-folder leak (the headline complaint)

`decompress()` extracts every EPUB to `<original>.epub_unzipped/` (plus `_unzipped_1`,
`_2`, … on repeats) **in the same directory as the source file** — typically the
user's Books/Downloads folder — and *nothing ever deletes them*. The README even
shrugs: "will [leave] temp folders behind that you should probably delete." Every
run scatters a full uncompressed copy of every book over the user's library. The
rebuild eliminates extraction entirely (§3): entries are recompressed in memory,
so there is nothing to clean up and no I/O ever happens outside the output file.

### 2.4 Broken EPUB output

The OCF spec requires the first zip entry to be the `mimetype` file, **stored
uncompressed**, so readers can sniff the container. `createEpub()` re-zips with
`compressing.zip.Stream` in `fs.readdir` order and deflates everything — the output
fails `epubcheck` and is rejected or mis-sniffed by stricter readers (Apple Books,
Kobo sideloading). It also races: entries are `addEntry`-ed asynchronously *after*
the stream has been piped. And the output name stutters: `Book.epub_compressed.epub`.

### 2.5 Architecture & security

- Written against Electron 9's insecure defaults: no `contextIsolation`, a preload
  that dumps the entire Node API surface (`fs`, `child_process`, …) onto `window`,
  renderer and "backend" logic tangled in one file. Modern Electron forbids most of
  this; porting means rewriting the process split anyway.
- The "IPC" is functions monkey-patched onto `window` from both sides — untestable
  by construction, and full of latent `undefined` calls depending on load order.
- CPU-bound image crunching runs on the renderer's main process, serially, one file
  at a time (`recursiveCompression` awaits every image in sequence); the UI thread
  and the compressor fight each other. A whole Chromium ships along to run ~300
  lines of glue.
- Quality tiers are two unrelated switch ladders (`compressPNG`, `compressJPG`) that
  drifted apart: three JPEG tiers ("Very High"/"High"/"Medium") differ only in PNG
  behavior, and GIF/WebP/SVG payloads are ignored entirely.

### 2.6 Missing project infrastructure

- No packaging, no installers, no releases (version 0.0.1 forever), no update path.
- No CI, no tests, no lint; `package-lock.json` is *gitignored*, so no reproducible
  install; the `.gitignore` is a generic Node boilerplate.
- No build or release scripts; none of the family conventions (version marker in
  README, `scripts/release.sh` → `lkm-release`, tag-triggered release workflow,
  `CICD.md`, `AGENTS.md`, `media-sources/` icon pipeline) exist here.

## 3. The rebuild

**Decision: rewrite as a Tauri 2 + Svelte 5 app** (matching Baegun and Lantenna),
with the compression engine as a plain Rust crate, UI from the family's system7-ui
component kit, and the standard family tooling (lkm-release stub, hardened CI,
tag-triggered releases). A rewrite, not a port: §2 shows every layer — runtime,
dependencies, IPC, file handling, output format — has to change; the only thing
worth carrying over is the product contract in §1 (plus the icons' spirit).

### 3.1 Architecture

```
crates/shrinkpub-core/  Pure-Rust engine: EPUB in → EPUB out. No Tauri types.
crates/shrinkpub-cli/   `shrinkpub` binary: batch-shrink from the terminal.
src-tauri/              Thin Tauri shell: commands + drag-drop, progress events.
src/                    Svelte 5 frontend: drop zone, quality picker, per-file rows.
```

- **`shrinkpub-core` (the engine).** Streams the source zip and writes the output
  zip **entirely in memory, entry by entry** — no extraction directory, no temp
  files, nothing left behind (fixes §2.3 by construction). JPEGs are re-encoded at
  the tier's quality, PNGs are palette-quantized (imagequant + lodepng — all pure
  Rust, no fragile prebuilt binaries), oversized images are downscaled at lower
  tiers, and **every recompressed entry is kept only if it is actually smaller** —
  otherwise the original bytes are copied through verbatim (fixes §2.2). Non-image
  entries are copied bit-for-bit.
  The output writes `mimetype` first, stored, per OCF (fixes §2.4), and is named
  `Book (shrunk).epub` (…`(shrunk 2)`, … on collision). EPUB detection is by
  container sniffing (zip magic + mimetype entry), not OS MIME guesses.
- **Tauri shell.** One async `shrink_epub` command per file, spawned on a worker
  thread via `spawn_blocking`; progress/log lines stream to the frontend as events
  keyed by file id. File paths come from Tauri's native drag-drop event (real paths,
  no `File.path` hacks) or an Open dialog. Strict CSP; no fs access exposed to the
  webview beyond the two commands.
- **Frontend.** Svelte 5 (runes), system7-ui components for the chrome, list,
  progress and quality select; the same drop-first interaction as the old app.
  Files process concurrently in Rust, with per-file progress bars, resulting sizes,
  and saved-percentage summary.

### 3.2 Quality tiers

One table instead of two switch ladders (all tiers strip metadata):

| Tier | JPEG quality | PNG quality (min–max) | Max width |
|---|---|---|---|
| Very High | 90 | 70–95 | — |
| High | 80 | 50–85 | — |
| Medium | 70 | 30–70 | — |
| Low | 55 | 15–55 | 1600 px |
| Very Low | 40 | 10–40 | 1200 px |
| Terrible | 25 | 5–25 | 900 px |
| Atrocious | 10 | 0–15 | 600 px |

The PNG *min* is a quality floor: when a (typically photographic) PNG cannot be
palette-quantized at least that well, it is kept as-is instead of butchered —
the same graceful fallback the keep-smaller rule provides for size.

### 3.3 Family tooling (per lkm-project-conventions)

- `scripts/release.sh` — stub over `lkm-release` with `RELEASE_KIND=tauri`
  (bumps `package.json` + lockfile, `tauri.conf.json`, workspace crate versions,
  `Cargo.lock`, README version marker; tags `vX.Y.Z`).
- `scripts/build.sh` — local production build orchestrator in the family house
  style (`npm install` → `npm run build` → `tauri build`), staging artifacts into
  `dist/`.
- `.github/workflows/ci.yml` — hardening trio (least-privilege permissions,
  ref-scoped concurrency, job timeouts); runs Rust fmt/clippy/tests and the
  frontend check/build on every push/PR.
- `.github/workflows/release.yml` — `v*` tag-triggered; gates on the test suite,
  then builds macOS (aarch64 + x86_64), Windows and Linux bundles via
  `tauri-action` and publishes them to the GitHub Release
  (`softprops`-equivalent flow; pre-release tags marked as such).
- `README.md` version marker, `CICD.md`, `AGENTS.md`, `media-sources/` with the
  icon master + regeneration script, family `.gitignore`.

### 3.4 Explicitly dropped

- The gulp/pngcrush "second pass" machinery (dead code; the keep-smaller rule
  makes it moot).
- The `epub_unzipped` naming scheme and everything that touched the source dir.
- The commented-out DevTools/debug scaffolding and `compressJPG2`.

## 4. Verification

- Engine test suite (`crates/shrinkpub-core/tests/shrink.rs`, fixtures generated
  in-test): output is a valid zip with `mimetype` first & stored (even from a
  non-compliant source), images shrank, non-images byte-identical, originals
  untouched, no stray files on disk (including after failures), larger results
  discarded byte-identically, unquantizable PNGs kept at high tiers, low tiers
  downscale, tier ordering of output sizes, collision suffixing, non-EPUB
  rejection, progress covering every entry — plus inline unit tests in
  `quality.rs`, `image_codec.rs`, `updates.rs` and the CLI.
- Gates run locally on this tree and enforced in CI: `cargo fmt --all --check`,
  `cargo clippy --workspace --all-targets -- -D warnings`,
  `cargo test --workspace`, `svelte-check`, `vite build`, and a release build of
  the desktop binary. Cross-platform bundles are produced by the tag-triggered
  release workflow.
- Real-world smoke test (release CLI, generated 8.6 MiB fixture book): Medium
  → 2.5 MiB (−72 %), Very High → 8.4 MiB (−3 %), Atrocious → 42 KiB; originals
  untouched, `(shrunk N)` collision naming, exit code 1 + clean message on
  missing/non-EPUB inputs.
