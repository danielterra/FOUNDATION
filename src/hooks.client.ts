export async function init(): Promise<void> {
	if (!import.meta.env.DEV || typeof window === 'undefined') {
		return;
	}

	if (Reflect.get(window, '__TAURI_INTERNALS__') !== undefined) {
		return;
	}

	const { setupDevInvoke } = await import('tauri-plugin-dev-invoke-api');
	await setupDevInvoke({
		url: 'http://127.0.0.1:3030',
		window: 'main',
		debug: false
	});
}
