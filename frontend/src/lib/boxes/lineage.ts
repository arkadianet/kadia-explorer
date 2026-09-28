import { ApiError } from '$lib/api/client';
import type { BoxDto, TxDto } from '$lib/api/types';

export const INITIAL_LINEAGE_ROWS = 4;
export const MAX_VISIBLE_LINEAGE_ROWS = 20;
export const MAX_LINEAGE_REFERENCES = 10_000;
export type LineageSide = 'producer' | 'spender';
export interface BoxReference {
	id: string;
	box: BoxDto | null;
}
export interface LineageBranch {
	tx: TxDto;
	inputs: BoxReference[];
	outputs: BoxReference[];
	missingInputs: number;
}
export interface BranchState {
	status: 'idle' | 'loading' | 'ready' | 'budget' | 'unavailable' | 'mismatch';
	data: LineageBranch | null;
	message: string | null;
}
export type LineageState = Record<LineageSide, BranchState>;
export const hasProducer = (box: BoxDto) => !/^0{64}$/.test(box.tx_id);
export const initialLineage = (): LineageState => ({
	producer: { status: 'idle', data: null, message: null },
	spender: { status: 'idle', data: null, message: null }
});
const hash = (value: unknown): value is string =>
	typeof value === 'string' && /^[a-f0-9]{64}$/i.test(value);
const count = (value: unknown): value is number =>
	Number.isSafeInteger(value) && Number(value) >= 0;
const amount = (value: unknown): value is string =>
	typeof value === 'string' &&
	/^\d{1,20}$/.test(value) &&
	BigInt(value) <= 18_446_744_073_709_551_615n;

export class LineageMismatch extends Error {}
export class LineageBudget extends Error {}

function validBox(box: BoxDto): boolean {
	return (
		!!box &&
		hash(box.id) &&
		hash(box.tx_id) &&
		count(box.index) &&
		amount(box.value) &&
		Array.isArray(box.tokens) &&
		box.tokens.length <= MAX_LINEAGE_REFERENCES &&
		box.tokens
			.slice(0, 2)
			.every(
				(token) =>
					hash(token.id) &&
					amount(token.amount) &&
					(token.name === null || typeof token.name === 'string') &&
					(token.decimals === null || (count(token.decimals) && token.decimals <= 255))
			)
	);
}

/** Checks only indexed membership. An input/output relationship never establishes which
 * payment or token movement connects particular boxes. Creation height is not inclusion. */
export function inspectLineage(box: BoxDto, side: LineageSide, tx: TxDto): LineageBranch {
	const expected = side === 'producer' ? box.tx_id : box.spent_by;
	if (
		!tx ||
		tx.id !== expected ||
		!hash(tx.id) ||
		!count(tx.height) ||
		(tx.block_id !== undefined && !hash(tx.block_id)) ||
		(tx.indexed_height != null && (!count(tx.indexed_height) || tx.indexed_height < tx.height)) ||
		!Array.isArray(tx.inputs) ||
		!Array.isArray(tx.outputs)
	) {
		throw new LineageMismatch(
			'The returned transaction does not match this box reference. Refresh box details before retrying.'
		);
	}
	if (tx.inputs.length + tx.outputs.length > MAX_LINEAGE_REFERENCES) {
		throw new LineageBudget(
			'This transaction exceeds the lineage reference limit. Open its transaction page to inspect available details.'
		);
	}
	if (
		tx.inputs.some(
			(input) =>
				!hash(input.id) ||
				(input.box !== null && (!validBox(input.box) || input.box.id !== input.id))
		) ||
		tx.outputs.some(
			(output, index) => !validBox(output) || output.tx_id !== tx.id || output.index !== index
		) ||
		new Set(tx.inputs.map((input) => input.id)).size !== tx.inputs.length ||
		new Set(tx.outputs.map((output) => output.id)).size !== tx.outputs.length
	) {
		throw new LineageMismatch(
			'Transaction box references are inconsistent. Refresh box details before retrying.'
		);
	}
	if (side === 'producer') {
		const focused = tx.outputs.find((output) => output.id === box.id);
		if (!focused || focused.index !== box.index || focused.value !== box.value) {
			throw new LineageMismatch(
				'This transaction does not contain the selected box at its recorded output. Refresh box details.'
			);
		}
	} else {
		const focused = tx.inputs.find((input) => input.id === box.id);
		if (
			!focused ||
			(focused.box !== null &&
				(focused.box.value !== box.value ||
					focused.box.index !== box.index ||
					focused.box.tx_id !== box.tx_id)) ||
			(box.spent_height !== null && tx.height !== box.spent_height)
		) {
			throw new LineageMismatch(
				'This transaction does not match the recorded spend. Refresh box details.'
			);
		}
	}
	return {
		tx,
		inputs: tx.inputs.map((input) => ({ id: input.id, box: input.box })),
		outputs: tx.outputs.map((output) => ({ id: output.id, box: output })),
		missingInputs: tx.inputs.filter((input) => input.box === null).length
	};
}

/** Two independently requested neighbours; no recursive graph fetching. stop() makes
 * every late result inert after navigation/unmount, including errors and retries. */
export function createLineage(options: {
	box: BoxDto;
	getTx: (id: string) => Promise<TxDto>;
	onChange: (state: LineageState) => void;
}) {
	let state = initialLineage();
	let stopped = false;
	function publish(side: LineageSide, branch: BranchState) {
		if (stopped) return;
		state = { ...state, [side]: branch };
		options.onChange(state);
	}
	return {
		async load(side: LineageSide) {
			const id =
				side === 'producer'
					? hasProducer(options.box)
						? options.box.tx_id
						: null
					: options.box.spent_by;
			if (stopped || !id || state[side].status === 'loading' || state[side].status === 'ready')
				return;
			publish(side, { status: 'loading', data: null, message: null });
			try {
				const tx = await options.getTx(id);
				if (stopped) return;
				publish(side, {
					status: 'ready',
					data: inspectLineage(options.box, side, tx),
					message: null
				});
			} catch (error) {
				if (stopped) return;
				const budget =
					error instanceof LineageBudget || (error instanceof ApiError && error.status === 422);
				const mismatch = error instanceof LineageMismatch;
				publish(side, {
					status: budget ? 'budget' : mismatch ? 'mismatch' : 'unavailable',
					data: null,
					message: budget
						? 'This transaction exceeds the available detail budget. The transaction link remains available.'
						: mismatch
							? error.message
							: error instanceof ApiError && error.status === 404
								? 'This transaction is not available in the current index. Its history may be outside the indexed range, or the chain view may have changed.'
								: 'Transaction details could not be loaded. Retry when the index is available.'
				});
			}
		},
		stop() {
			stopped = true;
		}
	};
}
