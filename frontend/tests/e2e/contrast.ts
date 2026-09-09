/** WCAG sRGB contrast for the opaque computed colours used by these controls. */
export function contrast(foreground: string, background: string): number {
	const luminance = (css: string) => {
		const channels = css
			.match(/[\d.]+/g)!
			.slice(0, 3)
			.map(Number)
			.map((value) => {
				const channel = value / 255;
				return channel <= 0.04045 ? channel / 12.92 : ((channel + 0.055) / 1.055) ** 2.4;
			});
		return channels[0] * 0.2126 + channels[1] * 0.7152 + channels[2] * 0.0722;
	};
	const a = luminance(foreground),
		b = luminance(background);
	return (Math.max(a, b) + 0.05) / (Math.min(a, b) + 0.05);
}
