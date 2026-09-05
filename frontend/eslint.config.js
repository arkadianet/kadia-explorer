import js from '@eslint/js';
import svelte from 'eslint-plugin-svelte';
import globals from 'globals';
import ts from 'typescript-eslint';

export default ts.config(
	js.configs.recommended,
	...ts.configs.recommended,
	...svelte.configs.recommended,
	{
		languageOptions: {
			globals: { ...globals.browser, ...globals.node }
		}
	},
	{
		files: ['**/*.svelte', '**/*.svelte.ts'],
		languageOptions: {
			parserOptions: {
				parser: ts.parser,
				extraFileExtensions: ['.svelte']
			}
		}
	},
	{
		rules: {
			// Opts into SvelteKit's typed-router `resolve()` wrapper for every
			// href/goto — out of scope for this task's plain nav links.
			'svelte/no-navigation-without-resolve': 'off'
		}
	},
	{
		ignores: ['build/', '.svelte-kit/', 'dist/']
	}
);
