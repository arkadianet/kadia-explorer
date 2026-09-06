<script lang="ts">
	import { minerColor } from '$lib/format/minerHue';
	import { relTime } from '$lib/format/time';
	import type { BlockDto } from '$lib/api/types';

	interface Props {
		blocks: BlockDto[];
	}

	let { blocks }: Props = $props();

	let now = $state(Date.now());

	$effect(() => {
		const id = setInterval(() => (now = Date.now()), 30_000);
		return () => clearInterval(id);
	});
</script>

<!-- The chain's most recent stretch, one tile per block, each capped by its miner's colour.
     Runs of one colour are runs of one pool — the single loud element on the page. -->
<div class="strip">
	{#each blocks as block (block.id)}
		<a class="tile" href={`/blocks/${block.height}`} title={`Miner ${block.miner_pk}`}>
			<span class="bar" style:background={minerColor(block.miner_pk)} aria-hidden="true"></span>
			<span class="height mono">{block.height}</span>
			<span class="meta">{block.tx_count} tx</span>
			<span class="meta">{relTime(block.timestamp, now)}</span>
		</a>
	{/each}
</div>

<style>
	.strip {
		display: flex;
		gap: var(--space-2);
		overflow-x: auto;
		padding-bottom: var(--space-1);
	}

	.tile {
		flex: 1 1 0;
		min-width: 86px;
		background: var(--surface-solid);
		border-radius: var(--radius-control);
		overflow: hidden;
		padding-bottom: var(--space-2);
		display: flex;
		flex-direction: column;
		gap: 2px;
	}

	.tile:hover,
	.tile:focus-visible {
		background: var(--surface-hover);
		color: inherit;
	}

	.bar {
		height: 4px;
		display: block;
	}

	.height {
		font-weight: 600;
		font-size: var(--fs-title);
		padding: var(--space-2) var(--space-2) var(--space-1);
	}

	.meta {
		padding: 0 var(--space-2);
		font-size: 11px;
		line-height: 1.35;
		color: var(--fg-muted);
		white-space: nowrap;
		overflow: hidden;
		text-overflow: ellipsis;
	}
</style>
