import type { PendingTxDetails, TxStatusDto } from '$lib/api/types';

type Counts = Pick<
	NonNullable<TxStatusDto['pending']>,
	'input_count' | 'data_input_count' | 'output_count'
>;
const id = (value: unknown): value is string =>
	typeof value === 'string' && /^[0-9a-f]{64}$/.test(value);
const record = (value: unknown): value is Record<string, unknown> =>
	!!value && typeof value === 'object' && !Array.isArray(value);
const count = (value: unknown, maximum: number): value is number =>
	Number.isSafeInteger(value) && Number(value) >= 0 && Number(value) <= maximum;
const amount = (value: unknown): value is string =>
	typeof value === 'string' &&
	/^[1-9][0-9]{0,18}$/.test(value) &&
	BigInt(value) <= 9223372036854775807n;

/** Validate the bounded projection before rendering IDs or parsing exact amounts.
 * Missing legacy details stay distinct from malformed new evidence. */
export function validatePendingDetails(value: unknown, counts: Counts): PendingTxDetails | null {
	if (
		!record(value) ||
		!count(counts.input_count, 512) ||
		!count(counts.data_input_count, 512) ||
		!count(counts.output_count, 4096)
	)
		return null;
	for (const [key, total, truncated] of [
		['inputs', counts.input_count, 'inputs_truncated'],
		['data_inputs', counts.data_input_count, 'data_inputs_truncated']
	] as const) {
		const ids = value[key];
		if (
			!Array.isArray(ids) ||
			ids.length > Math.min(32, total) ||
			!ids.every(id) ||
			new Set(ids).size !== ids.length ||
			value[truncated] !== ids.length < total
		)
			return null;
	}
	if (
		!Array.isArray(value.outputs) ||
		value.outputs.length > Math.min(32, counts.output_count) ||
		value.outputs_truncated !== value.outputs.length < counts.output_count
	)
		return null;
	const outputIds = new Set<string>();
	let complete =
		!value.inputs_truncated && !value.data_inputs_truncated && !value.outputs_truncated;
	for (const [index, output] of value.outputs.entries()) {
		if (
			!record(output) ||
			output.index !== index ||
			!amount(output.value) ||
			(output.id !== null && !id(output.id)) ||
			typeof output.ergo_tree_truncated !== 'boolean'
		)
			return null;
		if (output.id !== null) {
			if (outputIds.has(output.id)) return null;
			outputIds.add(output.id);
		}
		if (
			output.address !== undefined &&
			output.address !== null &&
			(typeof output.address !== 'string' || !/^[1-9A-HJ-NP-Za-km-z]{1,1024}$/.test(output.address))
		)
			return null;
		if (
			output.ergo_tree_truncated
				? output.ergo_tree !== null
				: typeof output.ergo_tree !== 'string' ||
					!/^(?:[0-9a-f]{2}){1,2048}$/.test(output.ergo_tree)
		)
			return null;
		if (
			!Array.isArray(output.tokens) ||
			output.tokens.length > 32 ||
			!(output.token_count === null || count(output.token_count, 16384))
		)
			return null;
		if (
			output.token_count === null
				? output.tokens.length !== 0 || output.tokens_truncated !== false
				: output.tokens.length > output.token_count ||
					output.tokens_truncated !== output.tokens.length < output.token_count
		)
			return null;
		const tokenIds = new Set<string>();
		for (const token of output.tokens) {
			if (!record(token) || !id(token.id) || !amount(token.amount) || tokenIds.has(token.id))
				return null;
			tokenIds.add(token.id);
		}
		complete &&=
			!output.ergo_tree_truncated && !output.tokens_truncated && output.token_count !== null;
	}
	if (
		value.complete !== complete ||
		new TextEncoder().encode(JSON.stringify(value)).byteLength > 32768
	)
		return null;
	return value as unknown as PendingTxDetails;
}
