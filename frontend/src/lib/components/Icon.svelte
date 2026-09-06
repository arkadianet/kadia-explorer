<script lang="ts" module>
	/**
	 * The explorer's whole icon set: hand-drawn 24 × 24 stroke paths, no library.
	 *
	 * They are all built from the same three moves — a box seen in isometric, a horizontal
	 * ledger line, a circle for a party — so a block, a box and an address read as relatives
	 * rather than as picks from three different sets.
	 */
	export const ICONS = {
		home: 'M3.6 10.4 12 4l8.4 6.4V19a1.4 1.4 0 0 1-1.4 1.4h-3.6v-5.6H8.6v5.6H5A1.4 1.4 0 0 1 3.6 19Z',
		blocks: 'M12 3.2 20 7.6v8.8L12 20.8 4 16.4V7.6Z M4 7.6l8 4.4 8-4.4 M12 12v8.8',
		txs: 'M4 8.4h13 M13.6 5 17 8.4 13.6 11.8 M20 15.6H7 M10.4 12.2 7 15.6l3.4 3.4',
		addresses:
			'M12 3.6a4.2 4.2 0 1 1 0 8.4 4.2 4.2 0 0 1 0-8.4Z M4.4 20.4c0-3.4 3.4-5.6 7.6-5.6s7.6 2.2 7.6 5.6',
		rent: 'M12 3.6v16.8 M15.8 7.2c-.8-1.3-2.2-2-3.8-2-2.2 0-3.8 1.2-3.8 3s1.6 2.6 3.8 3.1 3.8 1.3 3.8 3.1-1.6 3-3.8 3c-1.7 0-3.1-.7-3.9-2.1',
		richlist: 'M4 20h16 M6.4 20v-6.2 M11.2 20V8.4 M16 20v-9 M20.8 20V4.8',
		status: 'M3.2 12.6h4l2.4-6.4 3.6 12 2.4-8 1.8 4h3.4',
		search: 'M11 4.2a6.8 6.8 0 1 1 0 13.6 6.8 6.8 0 0 1 0-13.6Z M15.9 15.9 20.4 20.4',
		'arrow-right': 'M4.4 12h14.4 M13.6 6.8 18.8 12l-5.2 5.2',
		'chevron-right': 'M9.6 5.6 16 12l-6.4 6.4',
		copy: 'M9 9V5.6A1.6 1.6 0 0 1 10.6 4h8A1.6 1.6 0 0 1 20.2 5.6v8a1.6 1.6 0 0 1-1.6 1.6H15 M5.4 8.8h8a1.6 1.6 0 0 1 1.6 1.6v8a1.6 1.6 0 0 1-1.6 1.6h-8a1.6 1.6 0 0 1-1.6-1.6v-8a1.6 1.6 0 0 1 1.6-1.6Z',
		box: 'M12 3.2 20 7.6v8.8L12 20.8 4 16.4V7.6Z M4 7.6l8 4.4 8-4.4 M12 12v8.8',
		contract: 'M6.4 3.6h7.2l4.4 4.4v12.4H6.4Z M13.4 3.6V8h4.4 M9.2 12.8h5.6 M9.2 16.4h5.6',
		'rent-coin':
			'M12 4a8 8 0 1 1 0 16 8 8 0 0 1 0-16Z M12 7.6v8.8 M14.2 10c-.5-.8-1.3-1.2-2.2-1.2-1.3 0-2.2.7-2.2 1.7s.9 1.5 2.2 1.8 2.2.8 2.2 1.8-.9 1.7-2.2 1.7c-1 0-1.8-.4-2.3-1.2',
		trace: 'M5.2 5.2h4v4h-4Z M14.8 14.8h4v4h-4Z M9.2 7.2h3.4a2.2 2.2 0 0 1 2.2 2.2v5.4',
		layers: 'M12 3.6 3.6 8 12 12.4 20.4 8Z M3.6 12.4 12 16.8l8.4-4.4 M3.6 16.8 12 21.2l8.4-4.4',
		clock: 'M12 4a8 8 0 1 1 0 16 8 8 0 0 1 0-16Z M12 7.6V12l3 1.8',
		sun: 'M12 8.2a3.8 3.8 0 1 1 0 7.6 3.8 3.8 0 0 1 0-7.6Z M12 2.8v2 M12 19.2v2 M2.8 12h2 M19.2 12h2 M5.5 5.5l1.4 1.4 M17.1 17.1l1.4 1.4 M18.5 5.5l-1.4 1.4 M6.9 17.1l-1.4 1.4',
		moon: 'M20 14.4A8.4 8.4 0 0 1 9.6 4 8.4 8.4 0 1 0 20 14.4Z',
		spark: 'M12 3.2 13.9 9l6.1.2-4.9 3.7 1.8 5.9-4.9-3.5-4.9 3.5 1.8-5.9L4 9.2 10.1 9Z',
		menu: 'M4 7h16 M4 12h16 M4 17h16'
	} as const;

	export type IconName = keyof typeof ICONS;
</script>

<script lang="ts">
	interface Props {
		name: IconName;
		size?: number;
		/** Stroke weight; small icons want a touch more so they do not disappear. */
		weight?: number;
		class?: string;
	}

	let { name, size = 20, weight = 1.6, class: cls = '' }: Props = $props();
</script>

<svg
	class="icon {cls}"
	width={size}
	height={size}
	viewBox="0 0 24 24"
	fill="none"
	stroke="currentColor"
	stroke-width={weight}
	stroke-linecap="round"
	stroke-linejoin="round"
	aria-hidden="true"
	focusable="false"
>
	<path d={ICONS[name]} />
</svg>

<style>
	.icon {
		display: block;
		flex: none;
	}
</style>
