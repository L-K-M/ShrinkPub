#!/usr/bin/env bash
# Cuts a release: bumps the version, commits, tags "v<version>", and with --push
# pushes branch + tag — which triggers .github/workflows/release.yml to package the
# universal macOS app with @electron/packager, ad-hoc sign it, zip it, and publish
# the GitHub Release. The tag names the Release and the zip
# (ShrinkPub-<tag>-darwin-universal.zip), but @electron/packager stamps the .app
# bundle's version from the *committed* package.json — it does NOT derive it from the
# tag — so the two must agree, or you'd ship "v1.3.0" containing an app that reports
# 0.0.1. package.json is the only committed version location (no lockfile is
# committed, and the bump must not create one).
#
#   scripts/release.sh 1.3.0          # bump package.json + README, commit, tag v1.3.0
#   scripts/release.sh 1.3.0 --push   # …also push the commit + tag (CI then publishes)
#   scripts/release.sh                # tag the current version as-is
#
# Usage: scripts/release.sh [X.Y.Z] [--push]
# Shared engine: https://github.com/L-K-M/release-tool (this stub only sets config).
set -euo pipefail

export RELEASE_APP_NAME="ShrinkPub"
export RELEASE_KIND="npm"
export RELEASE_CI_NOTE="CI (release.yml) will now package + ad-hoc sign the universal macOS app and publish the GitHub Release for <tag>."
export RELEASE_INVOKED_AS="scripts/release.sh"

BIN="${LKM_RELEASE_BIN:-lkm-release}"
command -v "$BIN" >/dev/null 2>&1 || {
  echo "error: lkm-release not found — clone https://github.com/L-K-M/release-tool and run ./install.sh" >&2
  exit 1
}
exec "$BIN" "$@"
