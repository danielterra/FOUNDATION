---
name: server-start
description: Start the FOUNDATION dev server (npm run tauri dev) under PM2 management. Use when the app is not running and Claude needs it up (MCP, QA, visual validation) or the user asks to start the server.
disable-model-invocation: false
---

# Server Start

Starts the dev server as the PM2 process `foundation-dev` (defined in `ecosystem.config.cjs` at the repo root).

## Steps

1. **Check current state** — `pm2 describe foundation-dev | Select-String "status"` (NOT `pm2 jlist | ConvertFrom-Json` — PS 5.1 chokes on duplicate env keys). If status is `online`, report "already running" and STOP — never start a second instance. If `stopped`/`errored`, run `pm2 delete foundation-dev` first. Other PM2 apps (e.g. `lightapps`) belong to the user — never touch them.
2. **Check for orphans** — `Get-Process FOUNDATION -ErrorAction SilentlyContinue`. An orphan `FOUNDATION.exe` from a previous unmanaged session blocks the new instance (port/DB). If found, kill it (`Stop-Process -Name FOUNDATION -Force`) — this is Claude's responsibility now, not the user's.
3. **Start** — `pm2 start ecosystem.config.cjs` (run from repo root, or pass the absolute path).
4. **Wait for readiness** — poll the backend log (`$env:LOCALAPPDATA\org.w3id.foundation\application.log`) for a `[DB] Read pool ready` or `[STARTUP] executor_ready` line with timestamp AFTER the start. Poll every ~15s. Budget: up to 5 min when Rust needs recompiling (watch `pm2 logs foundation-dev --lines 30 --nostream` for cargo build progress), ~60s otherwise.
5. **Report** — PID, readiness lines found, and any startup warnings. If the log stalls at `MCP server listening` with no `[STARTUP]` lines and `pm2 logs` shows `transport invoke timed out` (Vite SSR zombie), STOP and run `/server-restart` with cache clean instead.

## Rules

- ALWAYS use PM2 — never spawn `npm run tauri dev` directly (unmanaged orphan).
- NEVER start when status is already `online`.
- The first boot after schema migrations can legitimately take minutes (e.g. one-shot VACUUM) — check the log for `[STARTUP]` progress lines before declaring a hang.
- No console window should ever appear: in debug the exe hides its own orphan console (`hide_orphan_console` in `src-tauri/src/main.rs`). A visible blank console window = regression of that guard (and the user closing it kills the server).
