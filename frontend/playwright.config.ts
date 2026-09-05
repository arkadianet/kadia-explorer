import { defineConfig, devices } from '@playwright/test';

/** Port the mock API listens on; `webServer` starts it and vite preview proxies `/v1` here. */
const MOCK_PORT = 18099;
/** Port `vite preview` serves the production build on. */
const APP_PORT = 4173;

export default defineConfig({
	testDir: 'tests/e2e',
	testMatch: '**/*.spec.ts',
	fullyParallel: true,
	forbidOnly: !!process.env.CI,
	retries: process.env.CI ? 1 : 0,
	workers: process.env.CI ? 1 : undefined,
	reporter: process.env.CI ? [['github'], ['list']] : 'list',
	use: {
		baseURL: `http://127.0.0.1:${APP_PORT}`,
		trace: 'on-first-retry'
	},
	projects: [{ name: 'chromium', use: { ...devices['Desktop Chrome'] } }],
	webServer: [
		{
			// Node ≥ 22.18 strips the types itself, so the mock runs straight from source.
			command: `node tests/e2e/mock/server.ts`,
			env: { MOCK_PORT: String(MOCK_PORT) },
			url: `http://127.0.0.1:${MOCK_PORT}/v1/status`,
			// Never reuse: a stale mock would serve a dataset built from older fixtures.
			reuseExistingServer: false,
			stdout: 'ignore',
			stderr: 'pipe'
		},
		{
			// The e2e run tests the built app (the same artefact the bundle budget measures),
			// not the dev server.
			// `--host 127.0.0.1`: vite preview otherwise binds `localhost`, which on this box
			// resolves to ::1 only, and Playwright's `url` check uses 127.0.0.1.
			command: `npm run build && npx vite preview --host 127.0.0.1 --port ${APP_PORT} --strictPort`,
			env: { VITE_API_PROXY: `http://127.0.0.1:${MOCK_PORT}` },
			url: `http://127.0.0.1:${APP_PORT}/`,
			// Never reuse: a stale preview would skip the build and test yesterday's bundle.
			reuseExistingServer: false,
			timeout: 180_000,
			stdout: 'ignore',
			stderr: 'pipe'
		}
	]
});
