<script lang="ts">
	import { goto } from '$app/navigation';

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

	function onKeydown(e: KeyboardEvent) {
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
			return;
		}
		// Escape only clears/blurs when the search input itself is focused, so it never
		// hijacks Escape while the user is elsewhere on the page (e.g. closing a dialog).
		if (e.key === 'Escape' && document.activeElement === inputEl) {
			q = '';
			inputEl?.blur();
		}
	}

	$effect(() => {
		window.addEventListener('keydown', onKeydown);
		return () => window.removeEventListener('keydown', onKeydown);
	});
</script>

<form role="search" class="search" onsubmit={submit}>
	<label class="visually-hidden" for="global-search">Search</label>
	<input
		id="global-search"
		type="search"
		bind:value={q}
		bind:this={inputEl}
		placeholder="Height, block, tx, box or address"
		autocomplete="off"
		spellcheck="false"
	/>
	<button type="submit" class="submit-btn" aria-label="Search">Go</button>
</form>

<style>
	.search {
		display: flex;
		gap: var(--space-2);
		max-width: 480px;
	}

	.search input {
		flex: 1;
		width: 100%;
		padding: var(--space-2) var(--space-3);
		background: var(--bg);
		color: var(--fg);
		border: 1px solid var(--border);
		border-radius: var(--radius);
		font: inherit;
	}

	.search input:focus {
		outline: 2px solid var(--accent);
		outline-offset: -1px;
	}

	.submit-btn {
		flex-shrink: 0;
		padding: var(--space-2) var(--space-3);
		background: var(--bg-elev);
		color: var(--fg);
		border: 1px solid var(--border);
		border-radius: var(--radius);
		font: inherit;
		cursor: pointer;
	}

	.submit-btn:hover {
		background: var(--bg-hover);
	}

	.submit-btn:focus-visible {
		outline: 2px solid var(--accent);
		outline-offset: -1px;
	}

	.visually-hidden {
		position: absolute;
		width: 1px;
		height: 1px;
		padding: 0;
		margin: -1px;
		overflow: hidden;
		clip: rect(0, 0, 0, 0);
		white-space: nowrap;
		border: 0;
	}
</style>
