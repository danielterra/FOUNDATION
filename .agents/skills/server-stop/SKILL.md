---
name: server-stop
description: "Stop the FOUNDATION dev server managed by PM2, including orphan child cleanup (FOUNDATION.exe / cargo). Use when the user asks to stop the server or Codex needs a clean shutdown."
---

# Server Stop

Stops the PM2 process `foundation-dev` and guarantees no orphan children survive.

## Steps

1. **Stop via PM2** — run `pm2 stop foundation-dev`, then `pm2 delete foundation-dev`. Ignore "process not found" (already stopped).
2. **Orphan sweep (MANDATORY)** — PM2 kills the `npm` root, but spawned `cargo`, `FOUNDATION`, or Vite children may survive:
   - On macOS/Linux use `pgrep -fl FOUNDATION`; on Windows use `Get-Process FOUNDATION -ErrorAction SilentlyContinue`. Terminate only the exact FOUNDATION process belonging to this repository.
   - Inspect leftover Cargo watchers (`pgrep -af cargo` on macOS/Linux; `Get-Process cargo` on Windows). Kill one only after verifying its command/path belongs to this repository — never kill unrelated Cargo builds.
3. **Verify** — `pm2 jlist` contains no `foundation-dev`, and the platform-specific FOUNDATION process check is empty.
4. **Report** — what was stopped and which orphans (if any) were cleaned.

## Rules

- ALWAYS sweep orphans after `pm2 stop` — a surviving FOUNDATION process holds the DB and blocks the next start.
- NEVER kill unrelated `node`/`cargo` processes — only the dev-server tree.
