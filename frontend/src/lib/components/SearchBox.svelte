<script lang="ts">
	import { goto } from '$app/navigation';

	interface Props {
		/** DOM id for the input (and its label's `for`). Must be unique per instance. */
		id?: string;
		/** Whether this instance owns the page-wide "/" focus shortcut. Exactly one instance
		 * should — the header's — so a second box on the page (e.g. /search's) neither steals
		 * focus nor registers a duplicate window listener. */
		globalShortcut?: boolean;
	}

	let { id = 'global-search', globalShortcut = true }: Props = $props();

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
	<input
		{id}
		type="search"
		bind:value={q}
		bind:this={inputEl}
		placeholder="Height, block, tx, box or address"
		autocomplete="off"
		spellcheck="false"
		onkeydown={onInputKeydown}
	/>
	<button type="submit" class="submit-btn" aria-label="Search">Go</button>
</form>

<style>
	.search {
		display: flex;
		gap: var(--space-2);
		max-width: 640px;
	}

	.search input {
		flex: 1;
		width: 100%;
		min-width: 0;
		padding: var(--space-1) var(--space-3);
		height: 30px;
		background: var(--bg-elev);
		color: var(--fg);
		border: var(--rule);
		border-radius: var(--radius);
		font: var(--fs-body) / 1.4 var(--font-sans);
	}

	.search input::placeholder {
		color: var(--fg-muted);
	}

	.search input:focus {
		outline: 2px solid var(--accent);
		outline-offset: -1px;
	}

	.submit-btn {
		flex-shrink: 0;
		height: 30px;
		padding: 0 var(--space-4);
		background: var(--accent);
		color: var(--accent-fg);
		border: 1px solid var(--accent);
		border-radius: var(--radius);
		font: 600 var(--fs-body) / 1 var(--font-sans);
		cursor: pointer;
	}

	.submit-btn:hover {
		filter: brightness(1.08);
	}

	.submit-btn:focus-visible {
		outline: 2px solid var(--accent);
		outline-offset: 2px;
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
