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
		if (e.key === '/' && !isTypingTarget(e.target)) {
			e.preventDefault();
			inputEl?.focus();
			return;
		}
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
</form>

<style>
	.search {
		max-width: 480px;
	}

	.search input {
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
