// Tauri doesn't have a Node.js server to do proper SSR
// so we use adapter-static with a fallback to index.html to put the site in SPA mode
// See: https://svelte.dev/docs/kit/single-page-apps
// See: https://v2.tauri.app/start/frontend/sveltekit/ for more info
export const ssr = false;

import posthog from 'posthog-js';
import { PUBLIC_POSTHOG_KEY, PUBLIC_POSTHOG_HOST } from '$env/static/public';

export const load = () => {
  if (!PUBLIC_POSTHOG_KEY) return;
  posthog.init(PUBLIC_POSTHOG_KEY, {
    api_host: PUBLIC_POSTHOG_HOST || 'https://us.i.posthog.com',
    defaults: '2026-01-30',
    person_profiles: 'identified_only',
    capture_pageview: false,
    opt_out_capturing_by_default: true,
  });
};
