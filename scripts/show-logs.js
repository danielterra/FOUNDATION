#!/usr/bin/env node
import fs from 'fs';
import os from 'os';
import path from 'path';
import readline from 'readline';

let lines = 100;
let watch = false;

for (const arg of process.argv.slice(2)) {
  if (arg === '--watch') watch = true;
  else if (/^\d+$/.test(arg)) lines = parseInt(arg, 10);
}

const LOG_PATHS = {
  darwin: path.join(os.homedir(), 'Library', 'Application Support', 'org.w3id.foundation', 'application.log'),
  linux:  path.join(os.homedir(), '.local', 'share', 'org.w3id.foundation', 'application.log'),
  win32:  path.join(process.env.LOCALAPPDATA ?? '', 'org.w3id.foundation', 'application.log'),
};

const logFile = LOG_PATHS[os.platform()];
if (!logFile) {
  console.error(`Plataforma não suportada: ${os.platform()}`);
  process.exit(1);
}

if (!fs.existsSync(logFile)) {
  console.error(`Arquivo de log não encontrado: ${logFile}`);
  process.exit(1);
}

function readLastLines(filePath, n) {
  const content = fs.readFileSync(filePath, 'utf8');
  const allLines = content.split('\n');
  return allLines.slice(-n - 1).join('\n');
}

const now = new Date().toISOString().replace('T', ' ').slice(0, 19);
console.log('=== FOUNDATION Application Logs ===');
console.log(`Current date/time: ${now}`);
console.log(watch ? `Showing last ${lines} lines (watching for changes...)` : `Showing last ${lines} lines`);
console.log('===================================\n');

console.log(readLastLines(logFile, lines));

if (watch) {
  let lastSize = fs.statSync(logFile).size;
  fs.watchFile(logFile, { interval: 500 }, (curr) => {
    if (curr.size > lastSize) {
      const stream = fs.createReadStream(logFile, { start: lastSize, end: curr.size });
      stream.pipe(process.stdout);
      lastSize = curr.size;
    }
  });
}
