import adapter from '@sveltejs/adapter-static';
import { sveltekit } from '@sveltejs/kit/vite';
import { defineConfig } from 'vitest/config';

// Note: as of @sveltejs/kit >=2.62, passing config directly to the
// `sveltekit()` Vite plugin causes svelte.config.js to be ignored entirely,
// so kit options (adapter, prerender) live here instead of a separate file.
/** Where `/v1` requests are forwarded; the e2e run points this at the mock API. */
const API_PROXY = process.env.VITE_API_PROXY ?? 'http://127.0.0.1:18090';

export default defineConfig({
	plugins: [
		sveltekit({
			compilerOptions: {
				// Force runes mode for the project, except for libraries. Can be removed in svelte 6.
				runes: ({ filename }) =>
					filename.split(/[/\\]/).includes('node_modules') ? undefined : true
			},

			adapter: adapter({ fallback: 'index.html', strict: false }),
			prerender: { entries: [] }
		})
	],
	server: {
		proxy: {
			'/v1': API_PROXY
		}
	},
	// `vite preview` does not inherit `server.proxy`, and the Playwright e2e run drives the
	// built app through it — so the same `/v1` proxy is declared for preview too.
	preview: {
		proxy: {
			'/v1': API_PROXY
		}
	},
	test: {
		include: ['tests/unit/**/*.test.ts'],
		environment: 'node'
	}
});
