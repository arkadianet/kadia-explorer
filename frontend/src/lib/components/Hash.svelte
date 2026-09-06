<script lang="ts">
	import { truncateMiddle } from '$lib/format/hash';

	interface Props {
		value: string;
		href?: string;
		head?: number;
		tail?: number;
		copy?: boolean;
	}

	let { value, href, head = 8, tail = 6, copy = true }: Props = $props();

	const short = $derived(truncateMiddle(value, head, tail));

	let copied = $state(false);
	let timer: ReturnType<typeof setTimeout> | undefined;

	async function doCopy() {
		try {
			await navigator.clipboard.writeText(value);
		} catch {
			// Clipboard may be unavailable (insecure context, denied permission).
			return;
		}
		copied = true;
		clearTimeout(timer);
		timer = setTimeout(() => (copied = false), 1200);
	}

	$effect(() => () => clearTimeout(timer));
</script>

<span class="hash">
	{#if href}
		<a class="mono" {href} title={value}>{short}</a>
	{:else}
		<span class="mono" title={value}>{short}</span>
	{/if}
	{#if copy}
		<button type="button" class="copy" aria-label="Copy" onclick={doCopy}>
			{copied ? '✓' : '⧉'}
		</button>
		{#if copied}<span class="copied" role="status">Copied</span>{/if}
	{/if}
</span>

<style>
	.hash {
		display: inline-flex;
		align-items: center;
		gap: var(--space-1);
	}
	.hash a.mono:hover,
	.hash a.mono:focus-visible {
		color: var(--accent-ink);
	}
	.copy {
		background: none;
		border: 0;
		padding: 0 var(--space-1);
		color: var(--fg-muted);
		cursor: pointer;
		border-radius: var(--radius-control);
		line-height: 1;
	}
	.copy:hover {
		color: var(--fg);
		background: var(--surface-hover);
	}
	.copied {
		color: var(--ok-ink);
		font-size: 11px;
	}
</style>
