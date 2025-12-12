import { invoke } from '@tauri-apps/api/core';

// Store original console methods
const originalConsole = {
  log: console.log,
  warn: console.warn,
  error: console.error,
  info: console.info,
  debug: console.debug
};

/**
 * Send log to Tauri backend
 */
async function sendLogToBackend(level, args) {
  try {
    // Convert all arguments to strings
    const message = args.map(arg => {
      if (typeof arg === 'object') {
        try {
          return JSON.stringify(arg, null, 2);
        } catch (e) {
          return String(arg);
        }
      }
      return String(arg);
    }).join(' ');

    await invoke('log_frontend', { level, message });
  } catch (err) {
    // Silently fail if logging fails - don't want to break the app
    originalConsole.error('Failed to send log to backend:', err);
  }
}

/**
 * Create a wrapped console method that logs both to console and backend
 */
function createWrappedMethod(level, originalMethod) {
  return function(...args) {
    // Call original console method first
    originalMethod.apply(console, args);

    // Send to backend (async, non-blocking)
    sendLogToBackend(level, args);
  };
}

/**
 * Initialize logging interception
 */
export function initializeLogging() {
  // Set up console wrappers - they will fail silently if not in Tauri
  console.log = createWrappedMethod('log', originalConsole.log);
  console.warn = createWrappedMethod('warn', originalConsole.warn);
  console.error = createWrappedMethod('error', originalConsole.error);
  console.info = createWrappedMethod('info', originalConsole.info);
  console.debug = createWrappedMethod('debug', originalConsole.debug);

  originalConsole.log('📝 Frontend logging initialized - all console output will be saved to file');
}

/**
 * Get the log file path
 */
export async function getLogFilePath() {
  try {
    return await invoke('get_log_file_path_command');
  } catch (err) {
    originalConsole.error('Failed to get log file path:', err);
    return null;
  }
}

/**
 * Clear all logs
 */
export async function clearLogs() {
  try {
    await invoke('clear_logs');
    originalConsole.log('✅ Logs cleared');
  } catch (err) {
    originalConsole.error('Failed to clear logs:', err);
  }
}
