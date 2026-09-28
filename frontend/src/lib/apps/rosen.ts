import type { BoxDto, PageDto, TxDto } from '$lib/api/types';
import { ROSEN_FRAUD, ROSEN_LOCK, ROSEN_PERMIT, ROSEN_RWT, ROSEN_TRIGGER } from './rosen-contracts';

const ID = /^[a-f0-9]{64}$/;
const MAX_LONG = (1n << 63n) - 1n;
export const ROSEN_EVENT_LIMIT = 10;
const MAX_REFERENCES = 100;
// Native ERG mappings pinned to the published mainnet token map. No live price or
// token-name lookup can change what asset this decoder identifies.
export const ROSEN_TARGETS = {
	cardano: '04b95368393c821f180deee8229fbd941baaf9bd748ebcdbf7adbb14.7273455247',
	ethereum: '0x6c060ba738af39a09f3b45ac6487dfc9ebb885f6',
	binance: '0xe0e8a04242f35b95dc64b07e0eae23a8e43a78e4'
} as const;
export type RosenChain = keyof typeof ROSEN_TARGETS;
export interface RosenDeposit {
	box: BoxDto;
	toChain: RosenChain;
	toAddress: string;
	fromAddress: string;
	amount: bigint;
	bridgeFee: bigint;
	networkFee: bigint;
}

function amount(value: unknown): bigint {
	if (typeof value !== 'string' || !/^(0|[1-9][0-9]{0,18})$/.test(value))
		throw new Error('Invalid integer amount');
	const n = BigInt(value);
	if (n > MAX_LONG) throw new Error('Amount outside supported range');
	return n;
}
const hex = (bytes: Uint8Array) =>
	Array.from(bytes, (byte) => byte.toString(16).padStart(2, '0')).join('');
function bytes(raw: unknown): Uint8Array {
	if (typeof raw !== 'string' || raw.length > 4096 || raw.length % 2 || !/^[a-f0-9]+$/.test(raw))
		throw new Error('Unsupported register bytes');
	return Uint8Array.from(raw.match(/../g)!, (value) => parseInt(value, 16));
}
function unsigned(data: Uint8Array, cursor: { offset: number }): number {
	let n = 0;
	for (let shift = 0; shift < 28; shift += 7) {
		const b = data[cursor.offset++];
		if (b === undefined) throw new Error('Truncated register');
		n += (b & 127) * 2 ** shift;
		if (b < 128) {
			if (shift && b === 0) throw new Error('Noncanonical register length');
			return n;
		}
	}
	throw new Error('Oversized register length');
}
/** Exact bounded Coll[Coll[Byte]], not a general Sigma interpreter. */
export function rosenFields(raw: unknown, count: number): Uint8Array[] {
	const data = bytes(raw),
		cursor = { offset: 1 };
	if (data[0] !== 0x1a || unsigned(data, cursor) !== count)
		throw new Error('Unsupported register shape');
	const fields: Uint8Array[] = [];
	for (let i = 0; i < count; i++) {
		const size = unsigned(data, cursor);
		if (size > 512 || cursor.offset + size > data.length) throw new Error('Invalid register field');
		fields.push(data.slice(cursor.offset, cursor.offset + size));
		cursor.offset += size;
	}
	if (cursor.offset !== data.length) throw new Error('Trailing register bytes');
	return fields;
}
function text(data: Uint8Array): string {
	if (!data.length || data.some((b) => b < 33 || b > 126)) throw new Error('Unsupported text');
	return new TextDecoder('utf-8', { fatal: true }).decode(data);
}
function collBytes(raw: unknown): Uint8Array {
	const data = bytes(raw),
		cursor = { offset: 1 };
	if (data[0] !== 0x0e) throw new Error('Expected byte collection');
	const size = unsigned(data, cursor);
	if (cursor.offset + size !== data.length) throw new Error('Invalid byte collection');
	return data.slice(cursor.offset);
}
function positiveInt(raw: unknown): number {
	const data = bytes(raw),
		cursor = { offset: 1 };
	if (data[0] !== 4) throw new Error('Expected integer');
	const encoded = unsigned(data, cursor);
	if (cursor.offset !== data.length || encoded % 2 || encoded === 0 || encoded > 200_000)
		throw new Error('Unsupported watcher count');
	return encoded / 2;
}
export function rosenDeposit(box: BoxDto): RosenDeposit | null {
	try {
		if (
			!ID.test(box.id) ||
			!ID.test(box.tx_id) ||
			box.ergo_tree !== ROSEN_LOCK ||
			box.tokens.length !== 0
		)
			return null;
		const fields = rosenFields(box.registers?.R4, 5).map(text);
		if (!Object.hasOwn(ROSEN_TARGETS, fields[0])) return null;
		const toChain = fields[0] as RosenChain;
		if (
			toChain === 'cardano'
				? !/^addr1[0-9a-z]{20,200}$/.test(fields[1])
				: !/^0x[a-fA-F0-9]{40}$/.test(fields[1])
		)
			return null;
		if (!/^[1-9A-HJ-NP-Za-km-z]{30,512}$/.test(fields[4])) return null;
		const value = amount(box.value),
			networkFee = amount(fields[2]),
			bridgeFee = amount(fields[3]);
		if (value === 0n || networkFee + bridgeFee >= value) return null;
		return {
			box,
			toChain,
			toAddress: fields[1],
			fromAddress: fields[4],
			amount: value,
			networkFee,
			bridgeFee
		};
	} catch {
		return null;
	}
}
export function transactionDeposits(tx: TxDto): RosenDeposit[] {
	if (tx.outputs.length + tx.inputs.length > MAX_REFERENCES) return [];
	return tx.outputs
		.flatMap((box) => {
			const deposit = rosenDeposit(box);
			return deposit ? [deposit] : [];
		})
		.slice(0, 4);
}
function encodeUnsigned(n: number): number[] {
	const out: number[] = [];
	do {
		const b = n % 128;
		n = Math.floor(n / 128);
		out.push(b | (n ? 128 : 0));
	} while (n);
	return out;
}
const utf8 = (value: string) => new TextEncoder().encode(value);
const longBytes = (n: bigint) =>
	Uint8Array.from({ length: 8 }, (_, i) => Number((n >> BigInt((7 - i) * 8)) & 255n));
export function eventRegister(deposit: RosenDeposit, tx: TxDto): string {
	verifySource(deposit, tx);
	const fields = [
		utf8(tx.id),
		utf8('ergo'),
		utf8(deposit.toChain),
		utf8(deposit.fromAddress),
		utf8(deposit.toAddress),
		longBytes(deposit.amount),
		longBytes(deposit.bridgeFee),
		longBytes(deposit.networkFee),
		utf8('erg'),
		utf8(ROSEN_TARGETS[deposit.toChain]),
		utf8(tx.block_id!),
		longBytes(BigInt(tx.height))
	];
	return hex(
		Uint8Array.from([
			0x1a,
			fields.length,
			...fields.flatMap((field) => [...encodeUnsigned(field.length), ...field])
		])
	);
}
function sameBox(a: BoxDto, b: BoxDto): boolean {
	return (
		a.id === b.id &&
		a.tx_id === b.tx_id &&
		a.index === b.index &&
		a.value === b.value &&
		a.creation_height === b.creation_height &&
		a.ergo_tree === b.ergo_tree &&
		JSON.stringify(a.registers) === JSON.stringify(b.registers) &&
		JSON.stringify(a.tokens.map((t) => [t.id, t.amount])) ===
			JSON.stringify(b.tokens.map((t) => [t.id, t.amount]))
	);
}
function validTx(tx: TxDto) {
	if (
		!ID.test(tx.id) ||
		!tx.block_id ||
		!ID.test(tx.block_id) ||
		!Number.isSafeInteger(tx.height) ||
		tx.height < 1 ||
		!Number.isSafeInteger(tx.indexed_height) ||
		tx.indexed_height! < tx.height ||
		tx.inputs.length + tx.outputs.length > MAX_REFERENCES
	)
		throw new Error('Transaction evidence is incomplete or exceeds the inspection limit.');
	if (
		new Set(tx.inputs.map((i) => i.id)).size !== tx.inputs.length ||
		new Set(tx.outputs.map((b) => b.id)).size !== tx.outputs.length ||
		tx.inputs.some((i) => !ID.test(i.id) || (i.box !== null && i.box.id !== i.id)) ||
		tx.outputs.some(
			(b, i) =>
				!ID.test(b.id) ||
				b.tx_id !== tx.id ||
				b.index !== i ||
				tx.inputs.some((input) => input.id === b.id)
		)
	)
		throw new Error('Transaction box references are inconsistent.');
}
function verifySource(deposit: RosenDeposit, tx: TxDto) {
	validTx(tx);
	const box = tx.outputs.find((b) => b.id === deposit.box.id);
	if (tx.id !== deposit.box.tx_id || !box || !sameBox(box, deposit.box))
		throw new Error(
			'The deposit no longer matches its creation transaction. Reload its detail page.'
		);
	// Rosen's extractor chooses the first supported deposit output. Multiple lock
	// requests are deliberately not interpreted as independent bridge transfers.
	if (tx.outputs.filter((b) => b.ergo_tree === ROSEN_LOCK && b.registers?.R4).length !== 1)
		throw new Error('Multiple lock requests need a different interpretation.');
}
export interface RosenEvent {
	box: BoxDto;
	watchers: number;
}
export function rosenEvent(box: BoxDto, register: string): RosenEvent | null {
	try {
		if (
			!ID.test(box.id) ||
			!ID.test(box.tx_id) ||
			box.ergo_tree !== ROSEN_TRIGGER ||
			box.registers?.R5 !== register ||
			box.tokens.length !== 1 ||
			box.tokens[0].id !== ROSEN_RWT ||
			amount(box.tokens[0].amount) === 0n ||
			collBytes(box.registers?.R4).length !== 32 ||
			hex(collBytes(box.registers?.R6)) !==
				'd1509fb23bedc93323731aa850c9922ae099ecbb7081c1028399088934a24a91'
		)
			return null;
		rosenFields(register, 12);
		return { box, watchers: positiveInt(box.registers?.R7) };
	} catch {
		return null;
	}
}
export type RosenSpend = {
	kind: 'unrecognized' | 'cleanup' | 'reward';
	paymentId: string | null;
	tx: TxDto;
};
export function eventSpend(event: RosenEvent, tx: TxDto, chain: RosenChain): RosenSpend {
	validTx(tx);
	const focused = tx.inputs.filter((i) => i.id === event.box.id);
	if (
		tx.id !== event.box.spent_by ||
		tx.height !== event.box.spent_height ||
		focused.length !== 1 ||
		!focused[0].box ||
		!sameBox(focused[0].box, event.box)
	)
		throw new Error('The event spend reference changed. Inspect the workflow again.');
	const result: RosenSpend = { kind: 'unrecognized', paymentId: null, tx };
	if (tx.outputs[0]?.ergo_tree === ROSEN_FRAUD) return { ...result, kind: 'cleanup' };
	if (
		tx.outputs[0]?.ergo_tree !== ROSEN_PERMIT ||
		tx.inputs.some((i) => !i.box) ||
		!tx.inputs.some((i) => i.box?.ergo_tree === ROSEN_LOCK)
	)
		return result;
	// Mirrors the pinned scanner's first non-permit R4 search, but accepts only
	// the supported target-chain transaction-ID form. This is a recorded claim.
	for (const box of tx.outputs) {
		if (box.ergo_tree === ROSEN_PERMIT || !box.registers?.R4) continue;
		try {
			const raw = collBytes(box.registers.R4);
			let paymentId: string;
			try {
				paymentId = text(raw);
			} catch {
				paymentId = hex(raw);
			}
			const valid =
				chain === 'cardano' ? ID.test(paymentId) : /^0x[a-fA-F0-9]{64}$/.test(paymentId);
			return { ...result, kind: 'reward', paymentId: valid ? paymentId : null };
		} catch {
			continue;
		}
	}
	return { ...result, kind: 'reward' };
}

export interface RosenState {
	loading: boolean;
	checked: boolean;
	error: string | null;
	source: TxDto | null;
	events: RosenEvent[];
	lookupTruncated: boolean;
	lookupAnchor: { height: number; block_id: string } | null;
	rejectedCandidates: number;
	spend: RosenSpend | null;
}
export const emptyRosenState = (): RosenState => ({
	loading: false,
	checked: false,
	error: null,
	source: null,
	events: [],
	lookupTruncated: false,
	lookupAnchor: null,
	rejectedCandidates: 0,
	spend: null
});
export function createRosenWorkflow(options: {
	deposit: RosenDeposit;
	getTx: (id: string, signal: AbortSignal) => Promise<TxDto>;
	findEvents: (register: string, signal: AbortSignal) => Promise<PageDto<BoxDto>>;
	onChange: (state: RosenState) => void;
}) {
	let stopped = false,
		generation = 0,
		controller: AbortController | null = null;
	let state = emptyRosenState();
	function emit(next: RosenState) {
		state = next;
		options.onChange({ ...next });
	}
	return {
		async inspect() {
			if (stopped || state.loading) return;
			const request = ++generation;
			controller = new AbortController();
			const signal = controller.signal;
			const timer = setTimeout(() => controller?.abort(), 20_000);
			const current = () => !stopped && request === generation && !signal.aborted;
			emit({ ...emptyRosenState(), loading: true });
			try {
				const source = await options.getTx(options.deposit.box.tx_id, signal);
				if (!current()) return;
				const register = eventRegister(options.deposit, source);
				const page = await options.findEvents(register, signal);
				if (!current()) return;
				if (
					page.consistency !== 'strict' ||
					!page.anchor ||
					!ID.test(page.anchor.block_id) ||
					!Number.isSafeInteger(page.anchor.height) ||
					page.anchor.height < source.height ||
					page.observed_anchor?.height !== page.anchor.height ||
					page.observed_anchor.block_id !== page.anchor.block_id ||
					page.items.length > ROSEN_EVENT_LIMIT ||
					new Set(page.items.map((b) => b.id)).size !== page.items.length ||
					(page.next_cursor !== null &&
						(typeof page.next_cursor !== 'string' || typeof page.next_snapshot !== 'string'))
				)
					throw new Error('The event lookup has no usable strict snapshot.');
				const events = page.items.flatMap((b) => {
					const event = rosenEvent(b, register);
					return event ? [event] : [];
				});
				if (events.some(({ box }) => (box.spent_by === null) !== (box.spent_height === null)))
					throw new Error('The event spend record is incomplete.');
				let spend: RosenSpend | null = null;
				if (events.length === 1 && page.next_cursor === null && events[0].box.spent_by) {
					const box = events[0].box;
					if (
						!ID.test(box.spent_by!) ||
						!Number.isSafeInteger(box.spent_height) ||
						box.spent_height! < 1
					)
						throw new Error('The event spend record is incomplete.');
					const tx = await options.getTx(box.spent_by!, signal);
					if (!current()) return;
					spend = eventSpend(events[0], tx, options.deposit.toChain);
				}
				// Independent reads are not an atomic cross-chain view. Recheck the
				// deposit inclusion before publishing events derived from that anchor.
				const recheck = await options.getTx(source.id, signal);
				if (!current()) return;
				verifySource(options.deposit, recheck);
				if (recheck.block_id !== source.block_id || recheck.height !== source.height)
					throw new Error('The deposit inclusion changed during inspection. Inspect again.');
				emit({
					loading: false,
					checked: true,
					error: null,
					source: recheck,
					events,
					lookupTruncated: page.next_cursor !== null,
					lookupAnchor: page.anchor,
					rejectedCandidates: page.items.length - events.length,
					spend
				});
			} catch (error) {
				if (!stopped && request === generation)
					emit({
						...emptyRosenState(),
						error: signal.aborted
							? 'Inspection timed out. Retry explicitly.'
							: error instanceof Error
								? error.message
								: 'The workflow could not be inspected.'
					});
			} finally {
				clearTimeout(timer);
				if (!stopped && request === generation && signal.aborted && state.loading)
					emit({ ...emptyRosenState(), error: 'Inspection timed out. Retry explicitly.' });
			}
		},
		stop() {
			stopped = true;
			generation++;
			controller?.abort();
		}
	};
}

/** Same-origin, explicit, nonstreaming API observations; cap bytes before JSON parsing. */
async function read<T>(path: string, signal: AbortSignal): Promise<T> {
	const response = await fetch('/v1' + path, { signal, credentials: 'omit', redirect: 'error' });
	if (!response.ok)
		throw new Error(
			`Indexed evidence unavailable (HTTP ${response.status}). No bridge outcome inferred.`
		);
	if (!response.body) throw new Error('Empty indexed response');
	const reader = response.body.getReader();
	const chunks: Uint8Array[] = [];
	let size = 0;
	try {
		while (true) {
			const next = await reader.read();
			if (next.done) break;
			size += next.value.length;
			if (size > 2 * 1024 * 1024)
				throw new Error('Indexed evidence exceeds the 2 MiB inspection limit.');
			chunks.push(next.value);
		}
	} finally {
		await reader.cancel();
		reader.releaseLock();
	}
	const data = new Uint8Array(size);
	let offset = 0;
	for (const chunk of chunks) {
		data.set(chunk, offset);
		offset += chunk.length;
	}
	return JSON.parse(new TextDecoder('utf-8', { fatal: true }).decode(data)) as T;
}
export const rosenApi = {
	getTx: (id: string, signal: AbortSignal) => read<TxDto>('/txs/' + id, signal),
	findEvents: (register: string, signal: AbortSignal) =>
		read<PageDto<BoxDto>>(
			'/registers/R5/' + register + '/boxes?consistency=strict&limit=10&dir=desc',
			signal
		),
	getBox: (id: string, signal: AbortSignal) => read<BoxDto>('/boxes/' + id, signal)
};
