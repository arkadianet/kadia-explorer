<script lang="ts">
	import Icon from './Icon.svelte';
	import { isNftKind, tokenKindLabel } from '$lib/token/kind';
	import type { TokenKind } from '$lib/api/types';

	interface Props {
		kind: TokenKind;
		/** Longer explanation on hover, e.g. the raw R7 type tag the backend read. */
		title?: string;
	}

	let { kind, title }: Props = $props();

	const label = $derived(tokenKindLabel(kind));
	const media = $derived(isNftKind(kind));
</script>

<!-- The kind pill: Badge's shape and type scale, with one distinction Badge cannot make —
     an NFT is a single thing that holds media, so the three media kinds carry the frame mark
     and the accent wash, while a fungible token and a membership stay quiet. -->
<span class="kind" class:media {title}>
	{#if media}<Icon name="image" size={12} weight={1.8} />{/if}
	{label}
</span>

<style>
	.kind {
		display: inline-flex;
		align-items: center;
		gap: 5px;
		padding: 0 var(--space-2);
		border-radius: var(--radius-pill);
		font-size: 12px;
		line-height: 18px;
		white-space: nowrap;
		color: var(--fg-muted);
		background: var(--surface-hover);
	}

	.kind.media {
		color: var(--accent-ink);
		background: var(--accent-wash);
	}
</style>
