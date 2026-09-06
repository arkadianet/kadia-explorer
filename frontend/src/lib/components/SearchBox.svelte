<script lang="ts">
	import { goto } from '$app/navigation';
	import Icon from '$lib/components/Icon.svelte';

	interface Props {
		/** DOM id for the input (and its label's `for`). Must be unique per instance. */
		id?: string;
		/** Whether this instance owns the page-wide "/" focus shortcut. Exactly one instance
		 * should — the header's — so a second box on the page (e.g. /search's) neither steals
		 * focus nor registers a duplicate window listener. */
		globalShortcut?: boolean;
		/** Sits under the field as a one-line reminder of what can be pasted in. */
		hint?: string;
	}

	let { id = 'global-search', globalShortcut = true, hint }: Props = $props();

	let q = $state('');
	let inputEl: HTMLInputElement | undefined;

	function submit(e: SubmitEvent) {
		e.preventDefault();
		const trimmed = q.trim();
		if (!trimmed) return;
		goto(`/search?q=${encodeURIComponent(trimmed)}`);
	}

	function isTypingTarget(target: EventTarget | null): boolean {
		if (!(target instanceof HTMLElement)) return false;
		const tag = target.tagName;
		return tag === 'INPUT' || tag === 'TEXTAREA' || target.isContentEditable;
	}

	/** Page-wide "/" shortcut; only the instance that owns `globalShortcut` installs it. */
	function onWindowKeydown(e: KeyboardEvent) {
		if (
			e.key === '/' &&
			!e.ctrlKey &&
			!e.metaKey &&
			!e.altKey &&
			!isTypingTarget(e.target) &&
			document.activeElement !== inputEl
		) {
			e.preventDefault();
			inputEl?.focus();
		}
	}

	// Escape is bound to the input rather than the window, so it only ever clears this box
	// while it has focus and never hijacks Escape elsewhere on the page (e.g. a dialog).
	function onInputKeydown(e: KeyboardEvent) {
		if (e.key === 'Escape') {
			q = '';
			inputEl?.blur();
		}
	}

	$effect(() => {
		if (!globalShortcut) return;
		window.addEventListener('keydown', onWindowKeydown);
		return () => window.removeEventListener('keydown', onWindowKeydown);
	});
</script>

<form role="search" class="search" onsubmit={submit}>
	<label class="visually-hidden" for={id}>Search</label>
	<div class="field glass">
		<span class="lead"><Icon name="search" size={18} /></span>
		<input
			{id}
			type="search"
			bind:value={q}
			bind:this={inputEl}
			placeholder="Search anything…"
			autocomplete="off"
			spellcheck="false"
			onkeydown={onInputKeydown}
		/>
		{#if globalShortcut}
			<!-- The shortcut is shown where it is used, rather than hidden in a help page. -->
			<kbd title="Press / to jump to the search box">/</kbd>
		{:else}
			<button type="submit" class="go" aria-label="Search"
				><Icon name="arrow-right" size={18} /></button
			>
		{/if}
	</div>
	{#if hint}<p class="hint">{hint}</p>{/if}
</form>

<style>
	.search {
		min-width: 0;
	}

	.field {
		display: flex;
		align-items: center;
		gap: var(--space-3);
		height: 46px;
		padding: 0 var(--space-2) 0 var(--space-4);
		border-radius: var(--radius-pill);
	}

	.field:focus-within {
		border-color: var(--accent);
		box-shadow:
			var(--shadow-lift),
			0 0 0 3px var(--accent-wash);
	}

	.lead {
		color: var(--fg-muted);
	}

	input {
		flex: 1;
		min-width: 0;
		background: none;
		border: 0;
		padding: 0;
		color: var(--fg);
		font: 400 var(--fs-body) / 1.4 var(--font-sans);
	}

	input::placeholder {
		color: var(--fg-muted);
	}

	input:focus {
		outline: none;
	}

	/* Chrome draws its own clear affordance inside type=search; it fights the kbd badge. */
	input::-webkit-search-cancel-button {
		display: none;
	}

	kbd {
		display: grid;
		place-items: center;
		width: 26px;
		height: 26px;
		margin-right: 6px;
		border-radius: 8px;
		border: var(--rule);
		background: var(--bg);
		color: var(--fg-muted);
		font: 600 var(--fs-micro) / 1 var(--font-sans);
	}

	.go {
		display: grid;
		place-items: center;
		width: 34px;
		height: 34px;
		border: 0;
		border-radius: 50%;
		background: var(--btn-fill);
		color: var(--btn-fill-fg);
		cursor: pointer;
	}

	.go:hover {
		background: var(--btn-hover);
		color: var(--btn-hover-fg);
	}

	.hint {
		margin-top: var(--space-2);
		padding-left: var(--space-4);
		font-size: var(--fs-micro);
		color: var(--fg-muted);
	}
</style>
