# Tasque OpenTUI (Read-Only)

OpenTUI React app for Tasque with tabbed views:
- `Tasks`
- `Epics`
- `Board`

The app is read-only. It does not mutate Tasque data.

## Data sources (fallback order)
1. `tsq-rs list --json`
2. `tsq list --json`
3. `.tasque/state.json`
4. `.tasque/events.jsonl`

## Spec state
Every row/card/details pane includes spec state:
- `attached`
- `missing`
- `invalid`

When metadata exists, spec state is validated against file existence and fingerprint.

## Run
```bash
cd tui-opentui
bun install
bun run start
```

## Keyboard
- `q` or `Esc`: quit (uses `renderer.destroy()`)
- `Ctrl+C`: quit (uses `renderer.destroy()`)
- `Tab` / `Left` / `Right`: switch tabs
- `Up` / `Down` or `j` / `k`: move selection
- `h` / `l`: switch board lane (Board tab)
- `r`: refresh now
- `p`: pause/resume auto-refresh
