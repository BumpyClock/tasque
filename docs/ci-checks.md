# Reproducible checks and opt-in hooks

Rust is pinned to 1.97.1 in the root toolchain and both binary-release workflows.
This is the concrete stable toolchain exercised by the secret-free Linux
baseline, not a claim that native release targets passed. CI Clippy, tests and
CLI builds now use `--locked`. Existing frozen Bun setup and the dependency of
TUI contract tests on the actual built CLI remain intact.

The baseline Clippy `question_mark` diagnostic in event-record parsing was fixed
with equivalent `?` propagation. No lint was disabled or error path removed.

Prepare Rust 1.97.1 with rustfmt/clippy, run `cargo fetch --locked`, and install
existing TUI dependencies with `cd tui-opentui && bun install --frozen-lockfile`.
Enable the tracked pre-push explicitly with `bash scripts/install-git-hooks.sh`.
It refuses unrelated active hooks/config and can be rerun. Repository-local
`.githooks` also resolves correctly in linked worktrees; repository-local Git
config is shared across those worktrees. No automatic installation occurs.

The hook requires a clean checkout of the pushed commit. All updated refs must
resolve to HEAD; deletion-only pushes skip source checks. Unknown history, new
branches, empty input, shared configuration, source renames/deletions and
multiple refs run conservative full Rust format/locked offline Clippy and full
TUI typecheck. No source mutation, dependency installation, `tsq` state writes,
contract test execution, service launch or publishing happens in the hook.
Missing prepared dependencies/toolchains fail with setup instructions.

`python3 scripts/test-git-hooks.py` exercises real hook/installer executables in
throwaway fixtures with stubbed tools. No outbound push occurs. These fixtures
prove ref and failure handling, not native release acceptance.

Compatible Ubuntu-latest jobs and Linux matrix legs now use Boat. The release
matrix retains `matrix.os` identities/job names while selecting Boat through a
separate Linux runner field. Existing native Windows/macOS legs, platform artifact
names and trust/publishing guards remain unchanged. Tokenless npm publishing stays
on `ubuntu-latest`, required by npm's GitHub-hosted trusted-publisher contract;
OIDC and provenance remain intact without a new token. Source edits dispatch no CI.
Owner-only public policy is configured separately; outsiders/bots remain blocked.

Native qualification of all six npm targets and cargo-zigbuild version/cache
benchmarking remain separate release gates. No cache optimization or speedup is
claimed from runner-label changes.
