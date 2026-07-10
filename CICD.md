# CI/CD

This repository uses GitHub Actions for continuous integration and tag-driven
releases. ShrinkPub is an **Electron** app (`"main": "main.js"`,
`"start": "electron ."`, Electron `^9.0.0` as a devDependency). There is no
electron-builder / electron-packager config committed to the repo, and these
workflows deliberately add none — the macOS bundle is produced on demand with
`@electron/packager` via `npx`.

## Workflows

| Workflow | Trigger | Purpose |
| --- | --- | --- |
| `ci.yml` | `pull_request`, push to `master` | Install dependencies and syntax-check the top-level scripts. |
| `release.yml` | push of a `v*` tag | Package a universal macOS `.app`, ad-hoc sign it, zip it, and attach it to a GitHub Release. |

## Continuous integration (`ci.yml`)

Runs on every pull request and on pushes to `master`:

1. **Checkout** the repository.
2. **Set up Node.js 20** (`actions/setup-node`).
3. **Install dependencies** — `npm install`. No lockfile
   (`package-lock.json` / `yarn.lock`) is committed, so `npm ci` is not used
   (it requires a lockfile).
4. **Syntax check** — `node --check` on each top-level script: the main entry
   `main.js` plus `preload.js` and `renderer.js`. `node --check` parses a file
   and reports syntax errors without running it.

The project defines no lint or test scripts, so CI intentionally stays light —
no invented `npm run lint` / `npm test` steps. Add real steps here if such
scripts are introduced. `index.html` / `style.css` are markup/style and are not
syntax-checked.

### Running locally

```bash
npm install
node --check main.js
node --check preload.js
node --check renderer.js

# Launch the app:
npm start
```

## Releases (`release.yml`)

Cut a release by pushing a tag:

```
git tag v1.2.3
git push origin v1.2.3
```

The workflow runs on `macos-latest` and:

1. Installs dependencies (`npm install`).
2. Packages a **universal** (Intel + Apple Silicon) macOS app **without adding
   any config files**:
   ```
   npx --yes @electron/packager . ShrinkPub --platform=darwin --arch=universal --out=dist --overwrite
   ```
   `@electron/packager` picks up the Electron version to bundle from the
   project's **devDependencies** (`electron ^9.0.0` is present), so no extra
   configuration is needed.
3. **Ad-hoc signs** the bundle: `codesign --force --deep --sign -
   dist/ShrinkPub-darwin-universal/ShrinkPub.app`. The `-` identity needs no
   certificate or keychain, but ad-hoc signing **is required** for the app to
   launch on Apple Silicon.
4. Zips the `.app` with `ditto -c -k --keepParent` (preserves macOS metadata
   and the bundle structure).
5. Attaches the zip to an auto-generated GitHub Release.

**Artifacts**

- `ShrinkPub-<tag>-darwin-universal.zip` — the universal macOS `.app` bundle.

**Caveats**

- The app is **ad-hoc signed only** — not signed with a Developer ID and not
  notarized. macOS Gatekeeper will warn on first launch. To open it: right-click
  → **Open** → **Open**, or run
  `xattr -dr com.apple.quarantine /Applications/ShrinkPub.app`.
- Only a macOS artifact is produced. No Windows/Linux builds are configured.

## Secrets

**None required.** Both workflows run with no secrets — the release uses the
automatic `GITHUB_TOKEN` (via `permissions: contents: write`) to create the
Release. Ad-hoc signing (`--sign -`) needs no certificate.

### Future option: electron-builder (DMG + real signing)

For a polished, notarized distribution you would switch to
[`electron-builder`](https://www.electron.build/). That path **requires a
config file** added to the repo (e.g. a `build` block in `package.json` or an
`electron-builder.yml`) and, for a trusted install, **secrets**: an Apple
Developer ID Application certificate (imported into a CI keychain) plus an
App Store Connect API key (or Apple ID + app-specific password) for
notarization. With those in place electron-builder can produce a signed,
notarized `.dmg` that opens without Gatekeeper warnings. That is intentionally
out of scope for the current secret-free setup.
