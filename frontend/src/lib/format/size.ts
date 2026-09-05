/** Byte size as kibibytes with one decimal, e.g. 1536 -> "1.5 KB". Sizes here are block and
 * transaction sizes from the API, which are plain `number` byte counts. */
export function formatKb(bytes: number): string {
	return `${(bytes / 1024).toFixed(1)} KB`;
}
