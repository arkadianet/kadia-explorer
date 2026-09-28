import type { BoxDto, TxDto } from '$lib/api/types';

// Exact, pinned Spectrum v3 ERG -> token contract. Unknown versions stay unknown.
// Sources and the captured mainnet example are documented in application-workflows.md.
export const SPECTRUM_SOURCE =
	'https://github.com/spectrum-finance/ergo-dex/blob/8fe94e1f8b4e1ab9402fca6fbc3955e3ad1415f7/contracts/amm/cfmm/v3/n2t/SwapSell.sc';
export const SPECTRUM_TEMPLATE =
	'd804d601b2a4730000d6027301d6037302d6049c73037e730405eb027305d195ed92b1a4730693b1db630872017307d806d605db63087201d606b2a5730800d607db63087206d608b27207730900d6098c720802d60a95730a9d9c7e997209730b067e7202067e7203067e720906edededededed938cb27205730c0001730d93c27206730e938c720801730f92720a7e7310069573117312d801d60b997e7313069d9c720a7e7203067e72020695ed91720b731492b172077315d801d60cb27207731600ed938c720c017317927e8c720c0206720b7318909c7e8cb2720573190002067e7204069c9a720a731a9a9c7ec17201067e731b067e72040690b0ada5d9010b639593c2720b731cc1720b731d731ed9010b599a8c720b018c720b02731f7320';
export const SPECTRUM_POOL_TREE =
	'1999030f0400040204020404040405feffffffffffffffff0105feffffffffffffffff01050004d00f040004000406050005000580dac409d819d601b2a5730000d602e4c6a70404d603db63087201d604db6308a7d605b27203730100d606b27204730200d607b27203730300d608b27204730400d6099973058c720602d60a999973068c7205027209d60bc17201d60cc1a7d60d99720b720cd60e91720d7307d60f8c720802d6107e720f06d6117e720d06d612998c720702720fd6137e720c06d6147308d6157e721206d6167e720a06d6177e720906d6189c72117217d6199c72157217d1ededededededed93c27201c2a793e4c672010404720293b27203730900b27204730a00938c7205018c720601938c7207018c72080193b17203730b9593720a730c95720e929c9c721072117e7202069c7ef07212069a9c72137e7214067e9c720d7e72020506929c9c721372157e7202069c7ef0720d069a9c72107e7214067e9c72127e7202050695ed720e917212730d907216a19d721872139d72197210ed9272189c721672139272199c7216721091720b730e';
export const SPECTRUM_FEE_TOKEN =
	'9a06d9e545a41fd51eeffc5e20d818073bf820c635e2a9d922269913e0de369d';
const HEX_ID = /^[a-f0-9]{64}$/;
const I64_MAX = (1n << 63n) - 1n;
const MAX_REFERENCES = 100;

type Constant = { type: number; value: bigint | string | boolean; raw: string };
/** A deliberately restricted parser, not a general Sigma serializer or interpreter. */
function constants(tree: string): Constant[] | null {
	if (tree.length > 16_384 || tree.length % 2 || !/^[a-f0-9]+$/.test(tree)) return null;
	const bytes = Uint8Array.from(tree.match(/../g)!, (value) => parseInt(value, 16));
	let offset = 0;
	const byte = () => {
		if (offset >= bytes.length) throw new Error('Truncated tree');
		return bytes[offset++];
	};
	const vlq = (bits: number) => {
		let value = 0n;
		for (let shift = 0; shift < bits; shift += 7) {
			const next = byte();
			value |= BigInt(next & 127) << BigInt(shift);
			if (next < 128) {
				if (value >= 1n << BigInt(bits) || (shift && next === 0))
					throw new Error('Invalid integer');
				return value;
			}
		}
		throw new Error('Integer limit');
	};
	const take = (length: number) => {
		if (length > 4096 || offset + length > bytes.length) throw new Error('Invalid constant');
		const text = tree.slice(offset * 2, (offset + length) * 2);
		offset += length;
		return text;
	};
	try {
		if (byte() !== 0x19) return null;
		const size = Number(vlq(32));
		if (size !== bytes.length - offset || vlq(32) !== 33n) return null;
		const values: Constant[] = [];
		for (let i = 0; i < 33; i++) {
			const start = offset;
			const type = byte();
			let value: Constant['value'];
			if (type === 4 || type === 5) {
				const encoded = vlq(type === 4 ? 32 : 64);
				value = (encoded >> 1n) ^ -(encoded & 1n);
			} else if (type === 1) {
				const encoded = byte();
				if (encoded > 1) return null;
				value = encoded === 1;
			} else if (type === 8) {
				if (byte() !== 0xcd) return null;
				value = take(33);
				if (!/^(02|03)[a-f0-9]{64}$/.test(value)) return null;
			} else if (type === 6 || type === 14) {
				value = take(Number(vlq(32)));
			} else return null;
			values.push({ type, value, raw: tree.slice(start * 2, offset * 2) });
		}
		return tree.slice(offset * 2) === SPECTRUM_TEMPLATE ? values : null;
	} catch {
		return null;
	}
}

export interface SpectrumOrder {
	box: BoxDto;
	baseNano: bigint;
	quoteId: string;
	minimumQuote: bigint;
	poolId: string;
	recipientTree: string;
	refundTree: string;
	feeNumerator: bigint;
	feeDenominator: bigint;
	executionDenominator: bigint;
	executionDelta: bigint;
	maximumExecutionFee: bigint;
	feeRemainderBase: bigint;
	maximumMinerFee: bigint;
	minerTree: string;
	quoteIsFeeToken: boolean;
}

function amount(raw: string): bigint {
	if (!/^(0|[1-9][0-9]{0,18})$/.test(raw)) throw new Error('Invalid amount');
	const value = BigInt(raw);
	if (value > I64_MAX) throw new Error('Amount overflow');
	return value;
}

export function spectrumOrder(box: BoxDto): SpectrumOrder | null {
	const tree = box.ergo_tree;
	if (!tree || !HEX_ID.test(box.id) || !HEX_ID.test(box.tx_id)) return null;
	const values = constants(tree);
	if (!values) return null;
	// Literal constants used by the published expression must also match. Matching only
	// the expression would accept altered input/output positions and truth conditions.
	const fixed: Record<number, string> = {
		0: '0400',
		6: '0404',
		7: '0406',
		8: '0402',
		9: '0400',
		12: '0400',
		17: '0101',
		18: '0101',
		19: '05f015',
		20: '060100',
		21: '0404',
		22: '0402',
		24: '0101',
		25: '0404',
		26: '060101',
		27: '04d00f',
		29: '0500',
		30: '0500',
		32: '0100'
	};
	if (Object.entries(fixed).some(([index, raw]) => values[Number(index)].raw !== raw)) return null;
	const types: Record<number, number> = {
		1: 5,
		2: 5,
		3: 5,
		4: 4,
		5: 8,
		10: 1,
		11: 5,
		13: 14,
		14: 14,
		15: 14,
		16: 5,
		23: 14,
		28: 14,
		31: 5
	};
	if (Object.entries(types).some(([index, type]) => values[Number(index)].type !== type))
		return null;
	const n = (i: number) => values[i].value as bigint;
	const s = (i: number) => values[i].value as string;
	try {
		if (
			!HEX_ID.test(s(13)) ||
			!HEX_ID.test(s(15)) ||
			s(23) !== SPECTRUM_FEE_TOKEN ||
			!/^0008cd(02|03)[a-f0-9]{64}$/.test(s(14)) ||
			n(1) <= 0n ||
			n(2) <= 0n ||
			n(2) > n(1) ||
			n(3) <= 0n ||
			n(3) > amount(box.value) ||
			n(4) <= 0n ||
			n(4) >= 1000n ||
			n(11) < 0n ||
			n(16) <= 0n ||
			n(31) < 0n ||
			(values[10].value === true) !== (s(15) === SPECTRUM_FEE_TOKEN) ||
			box.tokens.length !== 1 ||
			box.tokens[0].id !== SPECTRUM_FEE_TOKEN ||
			amount(box.tokens[0].amount) !== n(11)
		)
			return null;
		return {
			box,
			baseNano: n(3),
			quoteId: s(15),
			minimumQuote: n(16),
			poolId: s(13),
			recipientTree: s(14),
			refundTree: '0008cd' + s(5),
			feeNumerator: n(4),
			feeDenominator: n(27),
			executionDenominator: n(1),
			executionDelta: n(2),
			maximumExecutionFee: n(11),
			feeRemainderBase: n(19),
			maximumMinerFee: n(31),
			minerTree: s(28),
			quoteIsFeeToken: values[10].value === true
		};
	} catch {
		return null;
	}
}

export function transactionOrders(tx: TxDto): SpectrumOrder[] {
	if (tx.inputs.length + tx.outputs.length > MAX_REFERENCES) return [];
	const seen = new Set<string>();
	const orders: SpectrumOrder[] = [];
	for (const box of [
		...tx.inputs.flatMap((input) => (input.box ? [input.box] : [])),
		...tx.outputs
	]) {
		const order = spectrumOrder(box);
		if (order && !seen.has(box.id)) {
			seen.add(box.id);
			orders.push(order);
		}
		if (orders.length === 4) break;
	}
	return orders;
}

export function sameOrderBox(a: BoxDto, b: BoxDto): boolean {
	return (
		a.id === b.id &&
		a.tx_id === b.tx_id &&
		a.index === b.index &&
		a.value === b.value &&
		a.creation_height === b.creation_height &&
		JSON.stringify(a.registers) === JSON.stringify(b.registers) &&
		a.ergo_tree === b.ergo_tree &&
		a.tokens.length === b.tokens.length &&
		a.tokens.every(
			(token, index) => token.id === b.tokens[index].id && token.amount === b.tokens[index].amount
		)
	);
}

export type SpectrumOutcome =
	| {
			kind: 'matched';
			recipient: BoxDto;
			poolBefore: BoxDto;
			poolAfter: BoxDto;
			contractQuote: string;
	  }
	| { kind: 'unrecognized'; reason: string };

/** Recognizes the supported observed exchange shape. Does not determine which Sigma
 * OR branch was proved, validate consensus, or assert that all orders are supported. */
export function spectrumOutcome(order: SpectrumOrder, tx: TxDto): SpectrumOutcome {
	const unknown = (reason: string): SpectrumOutcome => ({ kind: 'unrecognized', reason });
	if (tx.inputs.length + tx.outputs.length > MAX_REFERENCES)
		return unknown('This transaction exceeds the workflow inspection limit.');
	if (tx.inputs.some((input) => !input.box))
		return unknown('Some transaction inputs are outside retained history.');
	const ids = new Set<string>();
	let tokenEntries = 0;
	try {
		for (const input of tx.inputs) {
			if (input.id !== input.box!.id)
				return unknown('Input references disagree with their box records.');
		}
		for (const box of [...tx.inputs.map((input) => input.box!), ...tx.outputs]) {
			if (!HEX_ID.test(box.id) || ids.has(box.id))
				return unknown('Box references are invalid or repeated.');
			ids.add(box.id);
			tokenEntries += box.tokens.length;
			if (tokenEntries > 10_000)
				return unknown('Token evidence exceeds the workflow inspection limit.');
			amount(box.value);
			const tokenIds = new Set<string>();
			for (const token of box.tokens) {
				if (!HEX_ID.test(token.id) || tokenIds.has(token.id) || amount(token.amount) <= 0n)
					return unknown('Token evidence contains an invalid or repeated asset.');
				tokenIds.add(token.id);
			}
		}
	} catch {
		return unknown('The returned amounts cannot be interpreted exactly.');
	}
	if (order.baseNano * order.feeNumerator > I64_MAX)
		return unknown('The order exceeds the supported signed-integer arithmetic range.');
	const focused = tx.inputs.filter((input) => input.id === order.box.id);
	if (focused.length !== 1 || !focused[0].box || !sameOrderBox(order.box, focused[0].box))
		return unknown('The spending transaction does not contain the expected order box.');
	if (tx.inputs.filter((input) => input.box && spectrumOrder(input.box)).length !== 1)
		return unknown('Multiple supported orders need a different interpretation.');
	const pool = tx.inputs[0]?.box;
	const next = tx.outputs[0];
	const recipient = tx.outputs[1];
	if (
		!pool ||
		!next ||
		!recipient ||
		pool.id === order.box.id ||
		pool.ergo_tree !== SPECTRUM_POOL_TREE ||
		next.ergo_tree !== pool.ergo_tree ||
		recipient.ergo_tree !== order.recipientTree ||
		pool.tokens.length !== 3 ||
		next.tokens.length !== 3 ||
		pool.tokens[0].id !== order.poolId ||
		pool.tokens[0].amount !== '1' ||
		next.tokens[0].amount !== '1' ||
		!pool.tokens.every((token, index) => token.id === next.tokens[index].id) ||
		pool.tokens[1].amount !== next.tokens[1].amount ||
		pool.tokens[2].id !== order.quoteId ||
		recipient.tokens[0]?.id !== order.quoteId ||
		tx.outputs.some((box, index) => box.tx_id !== tx.id || box.index !== index)
	)
		return unknown(
			'The observed spend does not match the supported pool and recipient layout. It may be a refund or another spend.'
		);
	try {
		const reservesX = amount(pool.value),
			reservesY = amount(pool.tokens[2].amount);
		const deltaX = amount(next.value) - reservesX;
		const deltaY = reservesY - amount(next.tokens[2].amount);
		const outputQuote = amount(recipient.tokens[0].amount);
		const quote = order.quoteIsFeeToken
			? ((outputQuote - order.maximumExecutionFee) * order.executionDenominator) /
				order.executionDelta
			: outputQuote;
		if (
			deltaX !== order.baseNano ||
			deltaY <= 0n ||
			quote < order.minimumQuote ||
			quote > I64_MAX - 1n ||
			amount(next.value) <= 10_000_000n
		)
			return unknown('The observed amounts do not meet the supported order constraints.');
		// Check both sides of the published pool/order price bounds with exact integers.
		const baseFee = order.baseNano * order.feeNumerator;
		const divisor = reservesX * order.feeDenominator + baseFee;
		if (reservesY * baseFee > (quote + 1n) * divisor || reservesY * baseFee < deltaY * divisor)
			return unknown('The observed pool movement does not meet the supported price bounds.');
		const poolFee = pool.registers?.R4;
		const nextFee = next.registers?.R4;
		// The API exposes register bytes; accept the exact canonical SInt encoding only.
		const feeHex = '04' + encodeUnsigned(order.feeNumerator * 2n);
		if (poolFee !== feeHex || nextFee !== feeHex)
			return unknown('The pool fee register is unavailable or differs from the order.');
		if (!order.quoteIsFeeToken) {
			const fee =
				(quote * (order.executionDenominator - order.executionDelta)) / order.executionDenominator;
			const remainder = order.feeRemainderBase - fee;
			// Mirrors the actual pinned expression, including its conditional token check.
			if (
				remainder > 0n &&
				recipient.tokens.length >= 2 &&
				(recipient.tokens[1].id !== SPECTRUM_FEE_TOKEN ||
					amount(recipient.tokens[1].amount) < remainder)
			)
				return unknown('The observed execution-fee remainder differs from the supported contract.');
		}
		const minerFees = tx.outputs
			.filter((box) => box.ergo_tree === order.minerTree)
			.reduce((sum, box) => sum + amount(box.value), 0n);
		if (minerFees > order.maximumMinerFee || minerFees > I64_MAX)
			return unknown('Miner-fee outputs exceed the declared order limit.');
		return {
			kind: 'matched',
			recipient,
			poolBefore: pool,
			poolAfter: next,
			contractQuote: quote.toString()
		};
	} catch {
		return unknown('The returned amounts cannot be interpreted exactly.');
	}
}

function encodeUnsigned(value: bigint) {
	let hex = '';
	do {
		const digit = Number(value & 127n);
		value >>= 7n;
		hex += (digit | (value ? 128 : 0)).toString(16).padStart(2, '0');
	} while (value);
	return hex;
}

export interface WorkflowState {
	loading: boolean;
	checked: boolean;
	error: string | null;
	box: BoxDto | null;
	tx: TxDto | null;
	outcome: SpectrumOutcome | null;
}
export function createSpectrumWorkflow(options: {
	order: SpectrumOrder;
	getBox: (id: string) => Promise<BoxDto>;
	getTx: (id: string) => Promise<TxDto>;
	onChange: (state: WorkflowState) => void;
}) {
	let generation = 0;
	let stopped = false;
	let state: WorkflowState = {
		loading: false,
		checked: false,
		error: null,
		box: null,
		tx: null,
		outcome: null
	};
	function emit() {
		options.onChange({ ...state });
	}
	async function inspect() {
		if (stopped || state.loading) return;
		const request = ++generation;
		state = { loading: true, checked: false, error: null, box: null, tx: null, outcome: null };
		emit();
		try {
			const box = await options.getBox(options.order.box.id);
			if (stopped || request !== generation) return;
			if (!sameOrderBox(options.order.box, box))
				throw new Error('The returned box no longer matches this order. Reload its detail page.');
			let tx: TxDto | null = null;
			let outcome: SpectrumOutcome | null = null;
			if (box.spent_by) {
				if (
					!HEX_ID.test(box.spent_by) ||
					!Number.isSafeInteger(box.spent_height) ||
					box.spent_height! < 1
				)
					throw new Error('The recorded spend is incomplete. Refresh box details.');
				tx = await options.getTx(box.spent_by);
				if (stopped || request !== generation) return;
				if (
					tx.id !== box.spent_by ||
					tx.height !== box.spent_height ||
					!tx.block_id ||
					!HEX_ID.test(tx.block_id)
				)
					throw new Error(
						'The spend reference changed or lacks an inclusion anchor. Refresh to recheck.'
					);
				outcome = spectrumOutcome(options.order, tx);
			} else if (box.spent_height !== null)
				throw new Error('The recorded spend is incomplete. Refresh box details.');
			state = { loading: false, checked: true, error: null, box, tx, outcome };
			emit();
		} catch (error) {
			if (stopped || request !== generation) return;
			state = {
				...state,
				loading: false,
				error: error instanceof Error ? error.message : 'The workflow could not be inspected.'
			};
			emit();
		}
	}
	return {
		inspect,
		stop() {
			stopped = true;
			generation++;
		}
	};
}
