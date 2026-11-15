export async function init(): Promise<void> {
	if (!import.meta.env.DEV || typeof window === 'undefined') {
		return;
	}

	if (Reflect.get(window, '__TAURI_INTERNALS__') !== undefined) {
		return;
	}

	const { setupDevInvoke } = await import('tauri-plugin-dev-invoke-api');
	await setupDevInvoke({
		url: `${window.location.origin}/__dev-invoke`,
		window: 'main',
		debug: false
	});
}
