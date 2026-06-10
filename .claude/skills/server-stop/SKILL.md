---
name: server-stop
description: Stop the FOUNDATION dev server managed by PM2, including orphan child cleanup (FOUNDATION.exe / cargo). Use when the user asks to stop the server or Claude needs a clean shutdown.
disable-model-invocation: false
---

# Server Stop

Stops the PM2 process `foundation-dev` and guarantees no orphan children survive.

## Steps

1. **Stop via PM2** — `pm2 stop foundation-dev; pm2 delete foundation-dev`. Ignore "process not found" (already stopped).
2. **Orphan sweep (MANDATORY)** — PM2 kills the `npm` root; on Windows the spawned tree (`cargo.exe`, `FOUNDATION.exe`, vite `node` child) may survive:
   - `Get-Process FOUNDATION -ErrorAction SilentlyContinue` → `Stop-Process -Name FOUNDATION -Force` if present.
   - Check for a leftover `cargo` watcher: `Get-Process cargo -ErrorAction SilentlyContinue` → kill ONLY if its `Path` is under this repo's `target\` or it has no other plausible owner — never kill the user's unrelated cargo builds.
3. **Verify** — both `pm2 jlist` (no `foundation-dev`) and `Get-Process FOUNDATION` (empty) confirm shutdown.
4. **Report** — what was stopped and which orphans (if any) were cleaned.

## Rules

- ALWAYS sweep orphans after `pm2 stop` — a surviving `FOUNDATION.exe` holds the DB and blocks the next start.
- NEVER kill unrelated `node`/`cargo` processes — only the dev-server tree.
