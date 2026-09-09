import { defineConfig } from '@playwright/test';

// Explicit local-only mainnet audit. The regular suite remains deterministic and offline.
export default defineConfig({
	testDir: 'tests/live',
	workers: 4,
	timeout: 60_000,
	use: { baseURL: 'http://127.0.0.1:5188' },
	webServer: {
		command: 'npx vite preview --host 127.0.0.1 --port 5188 --strictPort',
		env: { VITE_API_PROXY: 'http://127.0.0.1:18091' },
		url: 'http://127.0.0.1:5188',
		reuseExistingServer: false
	}
});
