// PM2 ecosystem for the FOUNDATION dev server.
// Managed exclusively via the /server-start, /server-stop and /server-restart skills.
const path = require('path');

// On Windows PM2 cannot exec `npm` (resolves to NPM.CMD and feeds it to Node) —
// point at npm-cli.js next to the Node binary instead.
const npmCli = path.join(path.dirname(process.execPath), 'node_modules', 'npm', 'bin', 'npm-cli.js');

module.exports = {
	apps: [
		{
			name: 'foundation-dev',
			script: npmCli,
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
