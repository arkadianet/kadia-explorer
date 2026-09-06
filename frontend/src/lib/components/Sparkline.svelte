<script lang="ts">
	/**
	 * A bucketed series drawn as a line or as bars. Every series handed to this component is
	 * computed from data the API actually returned — see the `title` each caller passes, which
	 * states the formula and the bucket width.
	 */
	interface Props {
		values: number[];
		kind?: 'line' | 'bars';
		/** Explains what the shape is made of. Rendered as the element's tooltip. */
		title: string;
		height?: number;
		color?: string;
	}

	let { values, kind = 'line', title, height = 34, color = 'var(--accent)' }: Props = $props();

	const W = 120;
	const H = 34;

	const max = $derived(Math.max(1, ...values));

	const points = $derived.by(() => {
		if (values.length === 0) return '';
		const step = values.length === 1 ? 0 : W / (values.length - 1);
		return values
			.map((v, i) => `${(i * step).toFixed(1)},${(H - 2 - (v / max) * (H - 5)).toFixed(1)}`)
			.join(' ');
	});

	const bars = $derived.by(() => {
		const n = values.length;
		if (n === 0) return [];
		const slot = W / n;
		const w = Math.max(1.5, slot - 1.6);
		return values.map((v, i) => ({
			x: i * slot,
			w,
			h: Math.max(1.5, (v / max) * (H - 4)),
			y: H - Math.max(1.5, (v / max) * (H - 4))
		}));
	});
</script>

{#if values.length > 0}
	<svg
		class="spark"
		viewBox="0 0 {W} {H}"
		preserveAspectRatio="none"
		style:height="{height}px"
		role="img"
		aria-label={title}
	>
		<title>{title}</title>
		{#if kind === 'bars'}
			{#each bars as b, i (i)}
				<rect x={b.x} y={b.y} width={b.w} height={b.h} rx="1" fill={color} opacity="0.85" />
			{/each}
		{:else}
			<polyline
				{points}
				fill="none"
				stroke={color}
				stroke-width="1.8"
				stroke-linecap="round"
				stroke-linejoin="round"
				vector-effect="non-scaling-stroke"
			/>
		{/if}
	</svg>
{/if}

<style>
	.spark {
		display: block;
		width: 100%;
		overflow: visible;
	}
</style>
