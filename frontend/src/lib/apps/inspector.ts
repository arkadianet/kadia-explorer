import type { BoxDto, TxDto } from '$lib/api/types';

export interface ApplicationSelector {
	kind: 'tx' | 'box';
	id: string;
	block?: string;
}
export type ApplicationEvidence = { kind: 'tx'; value: TxDto } | { kind: 'box'; value: BoxDto };
export interface InspectorState {
	loading: boolean;
	evidence: ApplicationEvidence | null;
	error: string;
}
const hash = /^[a-fA-F0-9]{64}$/;
function readableBox(value: BoxDto): boolean {
	return (
		!!value &&
		typeof value.id === 'string' &&
		hash.test(value.id) &&
		typeof value.tx_id === 'string' &&
		hash.test(value.tx_id) &&
		typeof value.value === 'string' &&
		/^(0|[1-9][0-9]{0,79})$/.test(value.value) &&
		(value.ergo_tree === null || typeof value.ergo_tree === 'string') &&
		Array.isArray(value.tokens) &&
		value.tokens.length <= 10000 &&
		value.tokens.every(
			(token) =>
				token &&
				typeof token.id === 'string' &&
				hash.test(token.id) &&
				typeof token.amount === 'string' &&
				/^(0|[1-9][0-9]{0,79})$/.test(token.amount)
		)
	);
}
export function applicationSelector(
	kind: string,
	id: string,
	block?: string | null
): ApplicationSelector {
	if (kind !== 'tx' && kind !== 'box') throw new Error('Choose a transaction or a box.');
	if (!hash.test(id.trim())) throw new Error('Enter a 64-character hexadecimal ID.');
	if (block && (kind !== 'tx' || !hash.test(block)))
		throw new Error('A pinned block must be a transaction block ID.');
	return { kind, id: id.trim().toLowerCase(), ...(block ? { block: block.toLowerCase() } : {}) };
}
export function applicationLink(selector: ApplicationSelector) {
	const validated = applicationSelector(selector.kind, selector.id, selector.block);
	return '/applications?' + new URLSearchParams({ ...validated });
}
export function createApplicationInspector(options: {
	getTx: (id: string) => Promise<TxDto>;
	getBox: (id: string) => Promise<BoxDto>;
	onChange: (state: InspectorState) => void;
}) {
	let generation = 0,
		stopped = false;
	function reset() {
		generation++;
		options.onChange({ loading: false, evidence: null, error: '' });
	}
	async function load(selector: ApplicationSelector) {
		if (stopped) return;
		const own = ++generation;
		options.onChange({ loading: true, evidence: null, error: '' });
		try {
			const valid = applicationSelector(selector.kind, selector.id, selector.block);
			let evidence: ApplicationEvidence;
			if (valid.kind === 'tx') {
				const value = await options.getTx(valid.id);
				if (own !== generation || stopped) return;
				if (
					!value ||
					value.id !== valid.id ||
					!Array.isArray(value.inputs) ||
					!Array.isArray(value.outputs)
				)
					throw new Error('Transaction evidence does not match this request.');
				if (value.inputs.length + value.outputs.length > 100)
					throw new Error(
						'Application discovery supports up to 100 input/output references. Open the transaction details for this larger transaction.'
					);
				if (
					!value.inputs.every(
						(input) =>
							input &&
							typeof input.id === 'string' &&
							hash.test(input.id) &&
							(input.box === null || (readableBox(input.box) && input.box.id === input.id))
					) ||
					!value.outputs.every(
						(box, index) => readableBox(box) && box.tx_id === value.id && box.index === index
					) ||
					new Set(value.inputs.map((input) => input.id)).size !== value.inputs.length ||
					new Set(value.outputs.map((box) => box.id)).size !== value.outputs.length
				)
					throw new Error('Transaction box evidence is malformed.');
				if (
					[
						...value.inputs.flatMap((input) => (input.box ? [input.box] : [])),
						...value.outputs
					].reduce((sum, box) => sum + box.tokens.length, 0) > 10000
				)
					throw new Error('Application evidence exceeds the token-entry limit.');
				if (valid.block && value.block_id !== valid.block)
					throw new Error(
						'The pinned inclusion changed or is unavailable. Clear the block pin and inspect again.'
					);
				evidence = { kind: 'tx', value };
			} else {
				const value = await options.getBox(valid.id);
				if (own !== generation || stopped) return;
				if (!readableBox(value) || value.id !== valid.id)
					throw new Error('Box evidence does not match this request.');
				evidence = { kind: 'box', value };
			}
			options.onChange({ loading: false, evidence, error: '' });
		} catch (cause) {
			if (own === generation && !stopped)
				options.onChange({
					loading: false,
					evidence: null,
					error: cause instanceof Error ? cause.message : 'Application evidence is unavailable.'
				});
		}
	}
	return {
		load,
		reset,
		stop() {
			stopped = true;
			generation++;
		}
	};
}
