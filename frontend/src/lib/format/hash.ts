/** "aa44ef6a6c08d0b198d65b762abb0181e6ad995654116fbedaa1d3d3eb95a4d6" -> "aa44ef6a…95a4d6".
 * Strings no longer than `head + tail` are returned unchanged. */
export function truncateMiddle(s: string, head = 8, tail = 6): string {
	if (s.length <= head + tail) return s;
	return `${s.slice(0, head)}…${s.slice(s.length - tail)}`;
}
