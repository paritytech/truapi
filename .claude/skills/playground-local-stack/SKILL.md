---
name: playground-local-stack
description: Boot the local TrUAPI dev stack (dotli host + playground) in a tmux `servers` window so the playground can be tested in a real browser. Auto-trigger on "run the playground locally", "test locally", "let me try it", "open the playground locally", "boot the stack", or any equivalent phrasing in this repo.
---

# Local playground stack

Brings up the two servers needed to load the playground inside a real TrUAPI host:

- **dotli host** at `http://localhost:5173` (out of `hosts/dotli/`).
- **playground** at `http://localhost:3000` (out of `playground/`).

The browser opens **`http://localhost:5173/localhost:3000`**, where dotli's
path-based router proxies the inner `localhost:3000` into its sandboxed iframe
and the playground detects the parent host and connects. Plain
`http://localhost:3000` renders the UI but stays disconnected.

## Prerequisites (always check, do not skip)

1. **Submodule populated.** If `hosts/dotli/package.json` does not exist, run
   from the repo root:

   ```bash
   git submodule update --init --recursive
   ```

2. **dotli deps installed.** If `hosts/dotli/node_modules/` does not exist:

   ```bash
   ( cd hosts/dotli && bun install --frozen-lockfile )
   ```

3. **playground snapshot fresh.** If you just regenerated the codegen or
   touched `js/packages/truapi/`, refresh the playground's frozen snapshot
   first (see the `refresh-playground-snapshot` skill).

Doing these in the foreground avoids cryptic failures inside tmux panes.

## Bring up the stack

Use one tmux window named `servers` in the `truapi` session, with two panes.
The single hard rule: **capture pane ids and always issue `cd` and the command
in the same `send-keys` call**. Numeric pane indexes vary by tmux
configuration, `tmux split-window -c <dir>` only sets a new pane's cwd, and a
freshly-created pane may swallow the first keys before its shell is ready.

```bash
# 0. Resolve the repo root so the commands work on any machine.
REPO_ROOT=$(git rev-parse --show-toplevel)

# 1. Create the servers window and capture the initial pane id.
DOTLI_PANE=$(
  tmux new-window -t truapi: -n servers -d -P -F '#{pane_id}' \
    -c "$REPO_ROOT"
)

# 2. Split horizontally for the playground pane and capture its pane id.
PLAYGROUND_PANE=$(
  tmux split-window -t "$DOTLI_PANE" -h -d -P -F '#{pane_id}' \
    -c "$REPO_ROOT/playground"
)

# 3. Launch each process WITH an explicit cd in the same send-keys.
tmux send-keys -t "$DOTLI_PANE" \
  "cd $REPO_ROOT/hosts/dotli && bun run preview" Enter
tmux send-keys -t "$PLAYGROUND_PANE" \
  "cd $REPO_ROOT/playground && yarn dev" Enter
```

Use `bun run preview:debugger` (i.e. `VITE_APP_DEBUG=true`) instead of
`bun run preview` when the user mentions investigating wire frames, debug
panel, or unknown-discriminant errors. The debug panel logs every
host↔product TrUAPI frame.

## Verify before reporting "ready"

```bash
tmux list-panes -t truapi:servers \
  -F '#P #{pane_current_command} #{pane_current_path}'
```

Expected: one pane running `bun` from `…/truapi/hosts/dotli`, and one pane
running `node` from `…/truapi/playground`. If either pane says `zsh`, the cd
or command did not take, fix it before declaring success.

Then peek at logs to confirm both servers are listening:

```bash
tmux capture-pane -t "$DOTLI_PANE" -p | tail
tmux capture-pane -t "$PLAYGROUND_PANE" -p | tail
```

dotli should print `Preview server on http://localhost:5173` (after a
`turbo run build` step that takes ~1–3 minutes on a cold cache). The
playground should print `Ready in <N>ms` and list `http://localhost:3000`.

## Hand off

Tell the user to open:

```
http://localhost:5173/localhost:3000
```

The connection chip should flip from **Offline** → **Handshaking** →
**Host Linked** within ~1s. Anything else points at a wire-table mismatch
or a stale playground snapshot, see the `e2e-dotli` skill's "Failure
modes" section.

## Stopping the stack

```bash
tmux kill-window -t truapi:servers
```

(or use the user-level `kill-servers` skill).

## Common failures

- **`error: Script not found "preview"`** → pane 1 is still in the truapi
  root, cd was missed. Fix: re-send with the explicit cd as above.
- **dotli's turbo build hangs on first run** → cold cache, expect 1–3 min.
  Do not retry until you've waited.
- **Connection chip stuck on Offline** → user opened plain
  `http://localhost:3000` instead of the dotli-framed URL.
- **Connection chip stuck on Handshaking** → wire-table mismatch between
  dotli's vendored `@parity/truapi` and the just-built one. Refresh the
  playground snapshot and rebuild dotli.
