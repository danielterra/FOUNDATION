#!/usr/bin/env node

const url = process.env.FOUNDATION_MCP_URL || "http://127.0.0.1:47178/mcp";

const stdin = process.stdin;
const stdout = process.stdout;
const stderr = process.stderr;

stdin.setEncoding("utf8");

function logError(msg) {
  stderr.write(`[foundation-bridge] ${msg}\n`);
}

function writeResponse(obj) {
  stdout.write(JSON.stringify(obj) + "\n");
}

function jsonRpcError(id, code, message) {
  return { jsonrpc: "2.0", id, error: { code, message } };
}

async function forward(line) {
  let req;
  try {
    req = JSON.parse(line);
  } catch (e) {
    logError(`invalid JSON from client: ${e.message}`);
    return;
  }

  const isNotification = req.id === undefined || req.id === null;

  let res;
  try {
    res = await fetch(url, {
      method: "POST",
      headers: {
        "Content-Type": "application/json",
        "Accept": "application/json"
      },
      body: line
    });
  } catch (e) {
    logError(`HTTP request failed: ${e.message}. Is the Foundation app running on ${url}?`);
    if (!isNotification) {
      writeResponse(jsonRpcError(
        req.id,
        -32000,
        `Foundation app não está acessível em ${url}. Abra o app Foundation e tente novamente.`
      ));
    }
    return;
  }

  if (isNotification) return;

  if (res.status === 202) {
    return;
  }

  const text = await res.text();
  if (!res.ok) {
    logError(`HTTP ${res.status}: ${text}`);
    writeResponse(jsonRpcError(req.id, -32000, `Foundation HTTP ${res.status}: ${text}`));
    return;
  }

  try {
    JSON.parse(text);
    stdout.write(text + "\n");
  } catch (e) {
    logError(`invalid JSON from server: ${e.message}`);
    writeResponse(jsonRpcError(req.id, -32603, `Foundation returned invalid JSON: ${e.message}`));
  }
}

let buffer = "";
stdin.on("data", (chunk) => {
  buffer += chunk;
  let idx;
  while ((idx = buffer.indexOf("\n")) >= 0) {
    const line = buffer.slice(0, idx).trim();
    buffer = buffer.slice(idx + 1);
    if (line) {
      forward(line).catch((e) => logError(`forward crashed: ${e.message}`));
    }
  }
});

stdin.on("end", () => {
  process.exit(0);
});

logError(`bridge started, forwarding to ${url}`);
