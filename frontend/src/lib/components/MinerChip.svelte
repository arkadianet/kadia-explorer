<script lang="ts">
	import { minerColor } from '$lib/format/minerHue';
	import { truncateMiddle } from '$lib/format/hash';

	interface Props {
		minerPk: string;
	}

	let { minerPk }: Props = $props();
</script>

<span class="miner" title={minerPk}>
	<span class="chip" style:background={minerColor(minerPk)} aria-hidden="true"></span>
	<span class="mono">{truncateMiddle(minerPk)}</span>
</span>

<style>
	.miner {
		display: inline-flex;
		align-items: center;
		gap: var(--space-2);
		white-space: nowrap;
	}
	.chip {
		width: 8px;
		height: 8px;
		flex-shrink: 0;
	}
</style>
