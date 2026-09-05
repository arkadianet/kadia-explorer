/**
 * A deterministic colour per miner.
 *
 * Ergo blocks carry the miner's public key but no pool name, so there is nothing to label a
 * block with. A stable hue derived from the key gives the same pool the same colour every
 * time it appears, which is enough to *see* that six of the last twelve blocks came from one
 * miner without naming anybody. Saturation and lightness are fixed so no colour ever shouts
 * louder than another, and so the same value stays legible on both themes.
 */

const SATURATION = 58;
const LIGHTNESS = 56;

/** FNV-1a (32-bit). Small, dependency-free, and well spread for short hex strings. */
function fnv1a(s: string): number {
	let h = 0x811c9dc5;
	for (let i = 0; i < s.length; i++) {
		h ^= s.charCodeAt(i);
		// h *= 16777619, kept in 32-bit range without BigInt.
		h = (h + ((h << 1) + (h << 4) + (h << 7) + (h << 8) + (h << 24))) >>> 0;
	}
	return h >>> 0;
}

/** Hue in [0, 360) for a miner public key. Stable across reloads and processes. */
export function minerHue(minerPk: string): number {
	return fnv1a(minerPk) % 360;
}

/** The miner's colour, ready for CSS. */
export function minerColor(minerPk: string): string {
	return `hsl(${minerHue(minerPk)} ${SATURATION}% ${LIGHTNESS}%)`;
}
