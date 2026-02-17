# LEARNINGS

- 2026-02-17: Chose JSONL-first V1 architecture (`.tasque/events.jsonl` source of truth, `state.json` derived cache). Explicitly no sqlite/dolt/backend abstraction in V1.
- 2026-02-17: Beads-inspired plan finalized in `docs/learned/tasque-v1-implementation-plan.md`. Chosen path: Bun+TypeScript V1, JSONL source-of-truth + derived state cache + single-machine lockfile protocol for <=5 concurrent agents.
- 2026-02-17: Locked V1 defaults: Bun runtime; fixed 6-char hash IDs + append-only child IDs; strict CAS claim; no generic link API in V1; supersede without auto-rewire; dep/supersede cycle rejection; state cache gitignored; JSON output includes schema_version=1.
- 2026-02-17: Updated V1 spec to Beads-compatible supersede/link behavior: generic `link add/remove` in V1; `supersede` closes source task and sets `superseded_by` without dependency rewiring.
- 2026-02-17: Locked durability decisions: bidirectional relates_to, canonical supersede command (close source + superseded_by), universal JSON envelope with schema_version=1, ULID event IDs, fail-safe stale-lock cleanup, and snapshot-based compaction with events.jsonl as canonical source.
- 2026-02-17: Docs synced to shipped V1 behavior (including `doctor`, `--exact-id`, lock policy, JSON envelope, and storage layout). Pitfall fixed: plan/spec references diverged from implementation defaults (`snapshot_every=200`, local-only snapshots via `.tasque/.gitignore`).
- 2026-02-17: Corrected earlier draft mismatch: V1 does include generic `link add/remove` plus dedicated `supersede` workflow command.
- 2026-02-17: Added release packaging flow: `bun run build` compiles single binary; `bun run release` emits platform artifact + `SHA256SUMS.txt` in `dist/releases/`.
