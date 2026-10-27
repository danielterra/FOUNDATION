// PM2 ecosystem for the FOUNDATION dev server.
// Managed exclusively via the /server-start, /server-stop and /server-restart skills.
const npmCommand = process.platform === 'win32' ? 'npm.cmd' : 'npm';

module.exports = {
	apps: [
		{
			name: 'foundation-dev',
			script: npmCommand,
			interpreter: 'none',
			args: 'run tauri dev',
			cwd: __dirname,
			windowsHide: true,
			// A compile error in dev would otherwise cause a restart storm —
			// restarts are always an explicit decision via /server-restart.
			autorestart: false,
			kill_timeout: 15000,
			max_restarts: 0,
		},
	],
};
