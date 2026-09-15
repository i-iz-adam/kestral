---
name: deploy-github-release
description: Procedure for deploying a GitHub release: version bump across manifests, Cargo sync, build verification, git tagging, and GitHub Actions release tracking for Kestrel.
triggers: deploy release, github release, publish release, release version, bump version
origin: project
---

# Deploying a GitHub Release for Kestrel

Use this skill when preparing, bumping version numbers, and deploying a new software release (e.g. `v1.0.2`) to GitHub for this repository.

## Overview & Workflow

Releases are automatically built and published by GitHub Actions (`.github/workflows/release.yml`) whenever a git tag matching `v*.*.*` is pushed to `origin`.

---

## Step-by-Step Procedure

### 1. Merge pending feature/fix PRs
Ensure all feature or bug fix branches are merged into `main` and locally checked out:
```bash
git checkout main
git pull origin main
```

### 2. Bump version in manifest & configuration files
Update the version string `X.Y.Z` (e.g. `1.0.2`) in the following files:
- `package.json`: `"version": "X.Y.Z"`
- `src-tauri/Cargo.toml`: `version = "X.Y.Z"`
- `src-tauri/tauri.conf.json`: `"package": { "version": "X.Y.Z" }`
- `src/app/About.tsx`: `const currentVersion = updateResult?.current_version || "X.Y.Z";`

### 3. Sync Cargo.lock
Run `cargo check` to update `src-tauri/Cargo.lock` with the new version:
```bash
cargo check --manifest-path src-tauri/Cargo.toml
```

### 4. Verify build and test suite
Ensure both backend tests and frontend bundles pass:
```bash
cargo test --manifest-path src-tauri/Cargo.toml
npm run build
```

### 5. Commit, Tag, and Push
Stage the bumped files, create the git commit, create the release tag, and push to GitHub:
```bash
git add package.json src-tauri/Cargo.toml src-tauri/Cargo.lock src-tauri/tauri.conf.json src/app/About.tsx
git commit -m "Bump version to X.Y.Z"
git tag vX.Y.Z
git push origin main --tags
```

### 6. Monitor GitHub Actions Release Pipeline
Check that the release workflow was triggered by the tag push:
```bash
gh run list --limit 3
```
GitHub Actions will automatically build the Tauri app across Ubuntu, macOS, and Windows runners and publish the binaries to the GitHub Release page for `vX.Y.Z`.
