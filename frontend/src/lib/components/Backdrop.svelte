<script lang="ts">
	/**
	 * The landscape behind the hero and the "Go deeper" band.
	 *
	 * There is no photography in this repository, and a stock photo would be one more asset to
	 * ship and license, so the mood is drawn: five ridgelines at falling contrast, a warm light
	 * source high on the right, haze pooling in each valley, and a film grain over the whole
	 * thing. Depth comes from the ridges overlapping and from the haze washing each one paler
	 * than the one in front — not from a gradient with a blur on it.
	 *
	 * `hero` is dawn over a valley, light enough to carry near-black display type on the left.
	 * `band` is the same terrain after dusk, dark enough to carry white type and dark glass.
	 */
	import { theme } from '$lib/theme/theme.svelte';

	interface Props {
		variant?: 'hero' | 'band';
		/** Distinguishes the gradient ids when both variants are on the page at once. */
		uid?: string;
	}

	let { variant = 'hero', uid = variant }: Props = $props();

	const p = (n: string) => `${uid}-${n}`;

	// Two lights over one terrain. The hero is dawn while the page is light and dusk once the
	// page turns dark, so the display type keeps its contrast either way; the band is always
	// dusk, because it carries white type and dark glass in both themes.
	const dawn = $derived(variant === 'hero' && theme.current !== 'dark');

	const sky = $derived(dawn ? ['#fdf6e9', '#e8f0ea'] : ['#12211a', '#0a100d']);
	const glow = $derived(dawn ? '#ffe6b0' : '#2e5f45');
	// Back to front: each ridge darker and more saturated than the one behind it.
	const ridges = $derived(
		dawn
			? ['#c6d6cb', '#9fbaab', '#6e9080', '#3f6350', '#1e3628']
			: ['#27452f', '#1f3826', '#182c1e', '#112015', '#0a130d']
	);
	const hazeStop = $derived(dawn ? 'rgba(253,246,233,0.85)' : 'rgba(24,44,33,0.6)');
</script>

<svg
	class="backdrop {variant}"
	viewBox="0 0 1440 540"
	preserveAspectRatio="xMidYMid slice"
	aria-hidden="true"
	focusable="false"
>
	<defs>
		<linearGradient id={p('sky')} x1="0" y1="0" x2="0.35" y2="1">
			<stop offset="0" stop-color={sky[0]} />
			<stop offset="1" stop-color={sky[1]} />
		</linearGradient>
		<radialGradient id={p('sun')} cx="0.6" cy="0.04" r="0.66">
			<stop offset="0" stop-color={glow} stop-opacity="0.7" />
			<stop offset="0.45" stop-color={glow} stop-opacity="0.2" />
			<stop offset="1" stop-color={glow} stop-opacity="0" />
		</radialGradient>
		<linearGradient id={p('haze')} x1="0" y1="0" x2="0" y2="1">
			<stop offset="0" stop-color={hazeStop} />
			<stop offset="1" stop-color={hazeStop} stop-opacity="0" />
		</linearGradient>
		<!-- Grain: one turbulence tile, desaturated and held at a low alpha, so the flat vector
		     fills stop looking like vector fills. -->
		<filter id={p('grain')} x="0" y="0" width="100%" height="100%">
			<feTurbulence type="fractalNoise" baseFrequency="0.85" numOctaves="3" stitchTiles="stitch" />
			<feColorMatrix type="saturate" values="0" />
		</filter>
	</defs>

	<rect width="1440" height="540" fill="url(#{p('sky')})" />
	<rect width="1440" height="540" fill="url(#{p('sun')})" />

	<!-- Ridge 1 — the far horizon, barely separated from the sky. -->
	<path
		fill={ridges[0]}
		opacity="0.62"
		d="M0 268 C 170 232 320 262 470 238 S 770 204 920 232 S 1230 198 1440 226 L1440 540 L0 540 Z"
	/>
	<rect y="220" width="1440" height="120" fill="url(#{p('haze')})" opacity="0.45" />

	<!-- Ridge 2 -->
	<path
		fill={ridges[1]}
		opacity="0.78"
		d="M0 344 C 120 322 200 300 286 246 C 352 204 404 232 470 288 C 560 364 660 300 760 272 C 860 244 940 300 1040 318 C 1160 340 1280 296 1440 312 L1440 540 L0 540 Z"
	/>
	<rect y="286" width="1440" height="120" fill="url(#{p('haze')})" opacity="0.34" />

	<!-- Ridge 3 — the valley floor reads between this one and the next. -->
	<path
		fill={ridges[2]}
		opacity="1"
		d="M0 392 C 160 360 350 402 550 368 S 870 330 1070 372 S 1310 336 1440 366 L1440 540 L0 540 Z"
	/>

	<!-- Ridge 4 -->
	<path
		fill={ridges[3]}
		d="M0 466 C 220 438 390 474 630 450 S 950 418 1150 452 S 1340 424 1440 442 L1440 540 L0 540 Z"
	/>

	<!-- The near bluff: the land climbing out of frame on the right, which is what gives the
	     scene a foreground and the floating tip card something dark to sit against. -->
	<path
		fill={ridges[4]}
		opacity="0.9"
		d="M1440 236 C 1372 262 1322 306 1274 372 C 1226 436 1152 484 1024 512 C 800 552 372 516 0 530 L0 540 L1440 540 Z"
	/>
	<path
		fill={ridges[4]}
		opacity="0.55"
		d="M0 520 C 260 496 520 526 780 512 C 1030 498 1230 520 1440 500 L1440 540 L0 540 Z"
	/>

	<rect
		width="1440"
		height="540"
		filter="url(#{p('grain')})"
		opacity={variant === 'hero' ? 0.055 : 0.09}
		style="mix-blend-mode: overlay"
	/>
</svg>

<style>
	.backdrop {
		position: absolute;
		inset: 0;
		width: 100%;
		height: 100%;
		display: block;
		/* The backdrop is scenery: it must never take a click meant for the glass above it. */
		pointer-events: none;
	}
</style>
