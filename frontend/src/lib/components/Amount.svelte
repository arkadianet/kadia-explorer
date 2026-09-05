<script lang="ts">
	import { formatErg, formatNano } from '$lib/format/amount';

	interface Props {
		nano: string;
		unit?: 'ERG' | 'nanoERG';
		maxFrac?: number;
	}

	let { nano, unit = 'ERG', maxFrac }: Props = $props();

	const text = $derived(unit === 'ERG' ? formatErg(nano, { maxFrac }) : formatNano(nano));
</script>

<span class="mono amount" title={`${nano} nanoERG`}>{text}<span class="unit">{unit}</span></span>

<style>
	.amount {
		white-space: nowrap;
	}
	.unit {
		color: var(--fg-muted);
		font-size: 11px;
		margin-left: var(--space-1);
	}
</style>
