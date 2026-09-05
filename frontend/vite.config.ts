import adapter from '@sveltejs/adapter-static';
import { sveltekit } from '@sveltejs/kit/vite';
import { defineConfig } from 'vitest/config';

// Note: as of @sveltejs/kit >=2.62, passing config directly to the
// `sveltekit()` Vite plugin causes svelte.config.js to be ignored entirely,
// so kit options (adapter, prerender) live here instead of a separate file.
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
			'/v1': process.env.VITE_API_PROXY ?? 'http://127.0.0.1:18090'
		}
	},
	test: {
		include: ['tests/unit/**/*.test.ts'],
		environment: 'node'
	}
});
