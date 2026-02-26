# Releasing

Read when: preparing a new `tsq` release, forcing release workflow runs, or bumping schema/version values.

## GitHub Workflows

Release automation uses two workflows:

- `/.github/workflows/release-please.yml`
  - Trigger: push to `main` or manual `workflow_dispatch`.
  - Action: opens/updates the release PR from Conventional Commits.
- `/.github/workflows/release-from-package.yml`
  - Trigger: manual `workflow_dispatch`.
  - Action: validates `package.json` version, creates release tag `v<package.version>`, publishes GitHub Release.
- `/.github/workflows/release.yml`
  - Trigger: GitHub Release `published`.
  - Action: validates release tag/version sync, runs `bun run doctor`, builds release artifacts on Linux/macOS/Windows, uploads `dist/releases/*` to the release.

## Standard Release Path

1. Merge feature/fix PRs to `main` with Conventional Commit titles.
2. Wait for (or manually run) `Release Please`.
3. Review the generated release PR and merge it.
4. Confirm `Release` workflow succeeds for all matrix jobs.
5. Verify release assets and `SHA256SUMS.txt` on the published release.

## Seamless Manual Path

Use this when you want to release directly from the current `package.json` version:

1. Ensure target version is in `package.json`.
2. Open GitHub Actions and run `Release From Package`.
3. Optional input: `version` (must match `package.json` if provided).
4. Workflow creates and publishes tag `v<package.version>`.
5. Published release triggers `Release` build/upload workflow automatically.

## Version And Schema Bumps

Use the local helper:

```bash
bun run version:bump -- --bump patch
bun run version:bump -- --version 1.4.0
bun run version:bump -- --schema 2
bun run version:bump -- --bump minor --schema 2 --dry-run
bun run release:verify-version -- --tag v1.4.0
```

What it updates:

- `package.json` (`version`) when `--version` or `--bump` is provided.
- `src/types.ts` (`SCHEMA_VERSION`) when `--schema` is provided.
- schema-version examples in:
  - `README.md`
  - `SKILLS/tasque/references/machine-output-and-durability.md`

Validation helper:

- `bun run release:verify-version`
  - With `--tag <tag>`: fails when tag != `v<package.version>`.
  - With `--expected-version <semver>`: fails when provided version != `package.json`.

Use workflow_dispatch on npm-publish.yml:

  # dry run
  gh workflow run npm-publish.yml --ref main -f dry_run=true

  # actual publish
  gh workflow run npm-publish.yml --ref main -f dry_run=false

  Watch it:

  gh run list --workflow "npm-publish" --limit 5
  gh run watch $(gh run list --workflow "npm-publish" --limit 1 --json databaseId --jq '.[0].databaseId')

  You can also run against a tag ref (example):

  gh workflow run npm-publish.yml --ref v0.4.0 -f dry_run=false

## Verification Checklist

Before merging a release PR:

1. `bun run doctor`
2. `bun run build`
3. `bun run release` (local artifact sanity check)
4. Confirm `tsq --version` matches expected release version

## Rollback

1. Delete GitHub Release.
2. Delete git tag (`git push --delete origin v<version>`).
3. Revert bad commit on `main`.
4. Let Release Please open the next corrective release PR.
