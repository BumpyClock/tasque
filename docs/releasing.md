---
summary: Release process, Cargo/npm version sync, GitHub workflows, and verification checklist.
read_when:
  - Preparing a new tsq release, forcing release workflow runs, or bumping release versions.
  - Changing release, npm publish, platform artifact, or version bump automation.
---

# Releasing

## Source Of Version Truth

- `Cargo.toml` is the release version source.
- `npm/package.json` and every `npm/platforms/*/package.json` must match it.
- `npm/scripts/bump-version.js` updates `Cargo.toml`, `Cargo.lock`, root npm package, platform packages, and optional dependency pins.

```bash
node npm/scripts/bump-version.js patch
node npm/scripts/bump-version.js minor
node npm/scripts/bump-version.js major
node npm/scripts/bump-version.js 0.5.0
node npm/scripts/bump-version.js patch --dry-run
```

Package aliases:

```bash
cd npm
npm run bump:patch
npm run bump:minor
npm run bump:major
npm run bump:set -- 0.5.0
```

## GitHub Workflows

Release automation uses these workflows:

- `/.github/workflows/release-please.yml`
  - Trigger: push to `main` or manual `workflow_dispatch`.
  - Action: runs Rust quality checks, then opens/updates the release PR from Conventional Commits using Rust release type.
- `/.github/workflows/release-from-package.yml`
  - Trigger: manual `workflow_dispatch`.
  - Action: validates optional `version` input against `Cargo.toml`, creates tag `v<Cargo.toml version>`, publishes GitHub Release.
- `/.github/workflows/release.yml`
  - Trigger: GitHub Release `published`.
  - Action: runs Rust format/lint/tests, builds release artifacts on Linux/macOS/Windows, bundles `SKILLS`, uploads `dist/releases/*`.
- `/.github/workflows/npm-publish.yml`
  - Trigger: GitHub Release `published` or manual `workflow_dispatch`.
  - Auth: npm Trusted Publishing/OIDC from GitHub Actions; no `NPM_TOKEN` is required for publish.
  - Action: builds target binaries, runs `scripts/build-npm.sh`, publishes platform packages, then publishes `@bumpyclock/tasque`.

## Standard Release Path

1. Merge feature/fix PRs to `main` with Conventional Commit titles.
2. Wait for (or manually run) `Release Please`.
3. Review the generated release PR and merge it.
4. Confirm `Release` succeeds for all matrix jobs.
5. Confirm `npm-publish` succeeds after the GitHub Release is published.
6. Verify release assets, `SHA256SUMS.txt`, and npm packages.

## Manual Release Path

Use this when releasing directly from the current `Cargo.toml` version:

1. Ensure target version is in `Cargo.toml`, `Cargo.lock`, and npm package files.
2. Open GitHub Actions and run `Release From Cargo`.
3. Optional input: `version` (must match `Cargo.toml` if provided).
4. Optional input: `target` (branch or commit SHA; default `main`).
5. Workflow creates and publishes tag `v<Cargo.toml version>`.
6. Published release triggers `Release` and `npm-publish` automatically.

## npm Package Flow

`npm-publish.yml` builds platform packages declared by the root package `optionalDependencies` and `npm/platforms/*/package.json` manifests.

Then it publishes root package `@bumpyclock/tasque`, whose optional dependencies point at those platform packages.

npm publishing uses Trusted Publishing. Each npm package must trust GitHub Actions for repository `BumpyClock/tasque` and workflow filename `npm-publish.yml`. The workflow grants `id-token: write` only to the publish job so npm can exchange the GitHub OIDC identity for short-lived publish credentials.

Trusted Publisher settings for all seven npm packages:

- Provider: GitHub Actions
- Repository: `BumpyClock/tasque`
- Workflow filename: `npm-publish.yml`
- Environment: blank (the workflow intentionally has no `environment:` gate so standard releases publish automatically)

Do not add `NODE_AUTH_TOKEN` or `NPM_TOKEN` to the publish steps. If publishing fails with `ENEEDAUTH`, verify the npm Trusted Publisher settings before adding token fallback.

`scripts/build-npm.sh`:

- reads version from `Cargo.toml`
- patches root + platform `package.json` versions
- copies `SKILLS/` into npm packages
- copies built binaries from `artifacts/<rust-target>/`

### npm Postinstall Skill Refresh

The npm package bundles `SKILLS/` alongside the binary. On `npm install`, the
postinstall script:

1. Resolves or downloads the `tsq` binary for the current platform.
2. Runs `tsq skills refresh --json` with the environment variable
   `TSQ_SKILLS_DIR` set to the package's `SKILLS/` directory path.
3. This refreshes only existing managed default `tasque` skill installs. It does
   not create missing installs.
4. Refresh failures warn but do not fail the install.
5. Set `TSQ_SKIP_SKILL_REFRESH=1` to opt out entirely.

Manual npm dry run:

```bash
gh workflow run npm-publish.yml --ref main -f dry_run=true
gh run list --workflow "npm-publish" --limit 5
gh run watch "$(gh run list --workflow "npm-publish" --limit 1 --json databaseId --jq '.[0].databaseId')"
```

Manual npm publish:

```bash
gh workflow run npm-publish.yml --ref v<version> -f dry_run=false
```

## Verification Checklist

Before merging a release PR:

1. `cargo fmt --check`
2. `cargo clippy --all-targets --all-features -- -D warnings`
3. `cargo test --quiet`
4. `npm run test:postinstall --prefix npm`
5. `cargo build --release --locked`
6. Confirm `target/release/tsq --version` matches `Cargo.toml`
7. Confirm npm package versions match `Cargo.toml`

## Rollback

1. Delete GitHub Release.
2. Delete git tag (`git push --delete origin v<version>`).
3. Revert bad commit on `main`.
4. Let Release Please open the next corrective release PR.
