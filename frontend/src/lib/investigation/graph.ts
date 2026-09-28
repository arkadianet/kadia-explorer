import { ApiError } from '$lib/api/client';
import type { BoxDto, TxDto } from '$lib/api/types';

export const MAX_NODES = 24;
export const MAX_EDGES = 48;
export const MAX_REFERENCES = 1000;
export const MAX_NODE_BYTES = 1_048_576;
export const MAX_EVIDENCE_BYTES = 4_194_304;
export const MAX_PLAN_LENGTH = 8192;
export type Kind = 'tx' | 'box';
export interface Ref {
	kind: Kind;
	id: string;
}
export interface Pin {
	height: number;
	block_id: string;
}
export interface Edge {
	from: string;
	to: string;
	relation: 'creates' | 'spends' | 'reads';
}
export interface Node extends Ref {
	key: string;
	pin?: Pin;
	status: 'idle' | 'loading' | 'ready' | 'stale' | 'error';
	data: TxDto | BoxDto | null;
	checkedAt: number | null;
	error: string | null;
}
export interface GraphState {
	nodes: Node[];
	edges: Edge[];
	selected: string | null;
	busy: boolean;
	conflict: string | null;
	notice: string | null;
}
export interface Plan {
	nodes: (Ref & { pin?: Pin })[];
	edges: Edge[];
}
export interface Reference {
	ref: Ref;
	edge: Edge;
	label: string;
	resolved: boolean;
}
export const keyOf = (ref: Ref) => `${ref.kind}:${ref.id}`;
const hash = (v: unknown): v is string => typeof v === 'string' && /^[a-f0-9]{64}$/.test(v);
const integer = (v: unknown): v is number => Number.isSafeInteger(v) && Number(v) >= 0;
const amount = (v: unknown): v is string =>
	typeof v === 'string' &&
	/^(0|[1-9]\d{0,19})$/.test(v) &&
	BigInt(v) <= 18_446_744_073_709_551_615n;
export const emptyGraph = (): GraphState => ({
	nodes: [],
	edges: [],
	selected: null,
	busy: false,
	conflict: null,
	notice: null
});
export function seedRef(kind: string | null, id: string | null): Ref | null {
	const normalized = id?.trim().toLowerCase();
	return (kind === 'tx' || kind === 'box') && hash(normalized) ? { kind, id: normalized } : null;
}
const newNode = (ref: Ref & { pin?: Pin }): Node => ({
	...ref,
	key: keyOf(ref),
	status: 'idle',
	data: null,
	checkedAt: null,
	error: null
});
export class InvestigationConflict extends Error {}
export class InvestigationLimit extends Error {}
const conflict = (message: string): never => {
	throw new InvestigationConflict(message);
};
const bytes = (data: unknown) => new TextEncoder().encode(JSON.stringify(data)).length;

function validateBox(box: BoxDto): void {
	if (
		!box ||
		!hash(box.id) ||
		!hash(box.tx_id) ||
		!hash(box.tree_hash) ||
		!integer(box.index) ||
		!integer(box.creation_height) ||
		!amount(box.value) ||
		!Array.isArray(box.tokens) ||
		box.tokens.length > MAX_REFERENCES ||
		!box.tokens.every((t) => t && hash(t.id) && amount(t.amount)) ||
		new Set(box.tokens.map((t) => t.id)).size !== box.tokens.length ||
		(box.spent_by !== null && !hash(box.spent_by)) ||
		(box.spent_height !== null && !integer(box.spent_height)) ||
		(box.spent_by === null) !== (box.spent_height === null)
	)
		conflict('The returned box has inconsistent identity, amounts or spend evidence.');
}
function validateTx(tx: TxDto): void {
	if (
		!tx ||
		!hash(tx.id) ||
		!integer(tx.height) ||
		!integer(tx.index) ||
		!integer(tx.timestamp) ||
		!amount(tx.fee) ||
		!Array.isArray(tx.inputs) ||
		!Array.isArray(tx.outputs) ||
		!Array.isArray(tx.data_inputs) ||
		(tx.block_id !== undefined && !hash(tx.block_id)) ||
		(tx.indexed_height != null && (!integer(tx.indexed_height) || tx.indexed_height < tx.height))
	)
		conflict('The returned transaction has inconsistent identity or inclusion evidence.');
	if (tx.inputs.length + tx.outputs.length + tx.data_inputs.length > MAX_REFERENCES)
		throw new InvestigationLimit(
			'This transaction exceeds the 1,000-reference investigation limit. Its full detail page remains available.'
		);
	for (const input of tx.inputs) {
		if (!input || !hash(input.id)) conflict('A transaction input has an invalid box reference.');
		if (input.box !== null) {
			validateBox(input.box);
			if (input.box.id !== input.id) conflict('An input box does not match its reference.');
		}
	}
	for (const [index, box] of tx.outputs.entries()) {
		validateBox(box);
		if (box.tx_id !== tx.id || box.index !== index)
			conflict('An output does not match its producing transaction.');
	}
	if (
		!tx.data_inputs.every(hash) ||
		new Set(tx.inputs.map((i) => i.id)).size !== tx.inputs.length ||
		new Set(tx.outputs.map((b) => b.id)).size !== tx.outputs.length ||
		new Set(tx.data_inputs).size !== tx.data_inputs.length
	)
		conflict('A transaction contains inconsistent or repeated box references.');
}
function boxIdentity(box: BoxDto) {
	return JSON.stringify([
		box.id,
		box.tx_id,
		box.index,
		box.creation_height,
		box.value,
		box.tree_hash,
		box.tokens.map((t) => [t.id, t.amount]).sort(([a], [b]) => a.localeCompare(b))
	]);
}
function txIdentity(tx: TxDto) {
	return JSON.stringify([
		tx.id,
		tx.height,
		tx.block_id ?? null,
		tx.fee,
		tx.inputs.map((i) => i.id),
		tx.outputs.map(boxIdentity),
		tx.data_inputs
	]);
}
export function references(node: Node): Reference[] {
	if (!node.data) return [];
	if (node.kind === 'tx') {
		const tx = node.data as TxDto;
		return [
			...tx.inputs.map((input) => ({
				ref: { kind: 'box' as const, id: input.id },
				edge: { from: `box:${input.id}`, to: node.key, relation: 'spends' as const },
				label: 'Input box',
				resolved: input.box !== null
			})),
			...tx.outputs.map((box) => ({
				ref: { kind: 'box' as const, id: box.id },
				edge: { from: node.key, to: `box:${box.id}`, relation: 'creates' as const },
				label: 'Output box',
				resolved: true
			})),
			...tx.data_inputs.map((id) => ({
				ref: { kind: 'box' as const, id },
				edge: { from: `box:${id}`, to: node.key, relation: 'reads' as const },
				label: 'Data input (read only)',
				resolved: false
			}))
		];
	}
	const box = node.data as BoxDto;
	return [
		...(/^0{64}$/.test(box.tx_id)
			? []
			: [
					{
						ref: { kind: 'tx' as const, id: box.tx_id },
						edge: { from: `tx:${box.tx_id}`, to: node.key, relation: 'creates' as const },
						label: 'Producing transaction',
						resolved: true
					}
				]),
		...(box.spent_by === null
			? []
			: [
					{
						ref: { kind: 'tx' as const, id: box.spent_by },
						edge: { from: node.key, to: `tx:${box.spent_by}`, relation: 'spends' as const },
						label: 'Indexed spending transaction',
						resolved: true
					}
				])
	];
}
const sameEdge = (a: Edge, b: Edge) =>
	a.from === b.from && a.to === b.to && a.relation === b.relation;
export function edgeObserved(edge: Edge, nodes: Node[]): boolean {
	return nodes.some((n) => n.data && references(n).some((r) => sameEdge(r.edge, edge)));
}

/** Independent API reads are checked for contradictions, not promoted to one shared snapshot. */
function inspect(node: Node, data: TxDto | BoxDto, nodes: Node[], edges: Edge[]) {
	if (data.id !== node.id) conflict('The server returned a different entity than requested.');
	if (bytes(data) > MAX_NODE_BYTES)
		throw new InvestigationLimit('This detail exceeds the 1 MiB per-node evidence limit.');
	if (node.kind === 'tx') {
		const tx = data as TxDto;
		validateTx(tx);
		if (node.pin && (node.pin.height !== tx.height || node.pin.block_id !== tx.block_id))
			conflict('A pinned transaction inclusion no longer matches this local index.');
		if (node.data && txIdentity(node.data as TxDto) !== txIdentity(tx))
			conflict('A previously loaded transaction inclusion or immutable box evidence changed.');
		for (const other of nodes)
			if (other.kind === 'tx' && other.key !== node.key && other.data) {
				const prior = other.data as TxDto;
				if (
					prior.height === tx.height &&
					prior.block_id &&
					tx.block_id &&
					prior.block_id !== tx.block_id
				)
					conflict('Loaded transactions disagree about the canonical block at one height.');
			}
	} else {
		validateBox(data as BoxDto);
		if (node.data && boxIdentity(node.data as BoxDto) !== boxIdentity(data as BoxDto))
			conflict('A previously loaded box has changed immutable identity or amounts.');
		if (
			node.data &&
			((node.data as BoxDto).spent_by !== (data as BoxDto).spent_by ||
				(node.data as BoxDto).spent_height !== (data as BoxDto).spent_height)
		)
			conflict('The indexed spending reference for a loaded box changed.');
	}
	const candidate = { ...node, data };
	const all = nodes.map((n) => (n.key === node.key ? candidate : n));
	for (const edge of edges) {
		const from = all.find((n) => n.key === edge.from);
		const to = all.find((n) => n.key === edge.to);
		if (!from?.data || !to?.data) continue;
		const txNode = from.kind === 'tx' ? from : to;
		const boxNode = from.kind === 'box' ? from : to;
		const tx = txNode.data as TxDto;
		const box = boxNode.data as BoxDto;
		if (!references(txNode).some((r) => sameEdge(r.edge, edge)))
			conflict('The transaction does not contain a linked box reference.');
		if (edge.relation === 'creates') {
			const output = tx.outputs.find((b) => b.id === box.id)!;
			if (boxIdentity(output) !== boxIdentity(box))
				conflict('Linked output evidence disagrees about the box identity or amount.');
		} else if (edge.relation === 'spends') {
			const input = tx.inputs.find((i) => i.id === box.id)!;
			if (
				(input.box && boxIdentity(input.box) !== boxIdentity(box)) ||
				box.spent_by !== tx.id ||
				box.spent_height !== tx.height
			)
				conflict('Linked spending evidence disagrees between independent reads.');
		}
	}
	if (all.reduce((n, row) => n + (row.data ? bytes(row.data) : 0), 0) > MAX_EVIDENCE_BYTES)
		throw new InvestigationLimit(
			'The 4 MiB evidence budget is full. Export this investigation and start another.'
		);
}

export function createInvestigation(options: {
	tx: (id: string, signal?: AbortSignal) => Promise<TxDto>;
	box: (id: string, signal?: AbortSignal) => Promise<BoxDto>;
	onChange: (state: GraphState) => void;
	now?: () => number;
}) {
	let state = emptyGraph();
	let generation = 0;
	let abort: AbortController | null = null;
	let stopped = false;
	const publish = (next: GraphState) => {
		state = next;
		options.onChange(next);
	};
	const update = (key: string, patch: Partial<Node>, rest: Partial<GraphState> = {}) =>
		publish({
			...state,
			...rest,
			nodes: state.nodes.map((n) => (n.key === key ? { ...n, ...patch } : n))
		});
	async function load(key: string) {
		const node = state.nodes.find((n) => n.key === key);
		if (stopped || state.busy || state.conflict || !node) return;
		const request = generation;
		abort = new AbortController();
		update(key, { status: 'loading', error: null }, { busy: true, notice: null });
		try {
			const data = await (node.kind === 'tx'
				? options.tx(node.id, abort.signal)
				: options.box(node.id, abort.signal));
			if (stopped || request !== generation) return;
			inspect(node, data, state.nodes, state.edges);
			const tx = node.kind === 'tx' ? (data as TxDto) : null;
			update(
				key,
				{
					data,
					status: 'ready',
					checkedAt: (options.now ?? Date.now)(),
					error: null,
					...(tx?.block_id ? { pin: { height: tx.height, block_id: tx.block_id } } : {})
				},
				{ busy: false }
			);
		} catch (error) {
			if (stopped || request !== generation) return;
			if (
				error instanceof InvestigationConflict ||
				(node.data && error instanceof ApiError && error.status === 404)
			) {
				generation++;
				publish({
					...state,
					busy: false,
					nodes: state.nodes.map((n) => ({
						...n,
						data: null,
						status: 'error',
						error: 'Evidence cleared after a conflicting read.',
						checkedAt: null
					})),
					conflict: `${error instanceof InvestigationConflict ? error.message : 'Previously loaded evidence is no longer available in this index.'} Evidence was cleared. Restart from the seed; this does not establish a network reorganization.`
				});
				return;
			}
			const message =
				error instanceof InvestigationLimit
					? error.message
					: error instanceof ApiError && error.status === 422
						? 'Detail exceeds the server’s read budget. Open the entity page for available summary or evidence.'
						: error instanceof ApiError && error.status === 404
							? 'Not available in the current index. This may be outside retained history or the local chain view may have changed.'
							: 'Details could not be loaded. Retry when the index is available.';
			update(key, { status: node.data ? 'stale' : 'error', error: message }, { busy: false });
		}
	}
	return {
		get state() {
			return state;
		},
		restore(plan: Plan) {
			generation++;
			abort?.abort();
			publish({
				...emptyGraph(),
				nodes: plan.nodes.map(newNode),
				edges: plan.edges.map((e) => ({ ...e })),
				selected: plan.nodes[0] ? keyOf(plan.nodes[0]) : null
			});
		},
		select(key: string) {
			if (state.nodes.some((n) => n.key === key)) publish({ ...state, selected: key });
		},
		load,
		async expand(source: string, reference: Reference) {
			if (stopped || state.busy || state.conflict) return;
			const node = state.nodes.find((n) => n.key === source);
			if (
				!node?.data ||
				node.status === 'stale' ||
				!references(node).some(
					(r) => keyOf(r.ref) === keyOf(reference.ref) && sameEdge(r.edge, reference.edge)
				)
			)
				return;
			const key = keyOf(reference.ref);
			const exists = state.nodes.some((n) => n.key === key);
			const edgeExists = state.edges.some((e) => sameEdge(e, reference.edge));
			if (
				(!exists && state.nodes.length >= MAX_NODES) ||
				(!edgeExists && state.edges.length >= MAX_EDGES)
			) {
				publish({
					...state,
					notice: `Investigation limit reached: ${MAX_NODES} nodes and ${MAX_EDGES} connections. Export this view or start another seed.`
				});
				return;
			}
			publish({
				...state,
				nodes: exists ? state.nodes : [...state.nodes, newNode(reference.ref)],
				edges: edgeExists ? state.edges : [...state.edges, reference.edge],
				selected: key
			});
			if (!exists || !state.nodes.find((n) => n.key === key)?.data) await load(key);
			else {
				try {
					const target = state.nodes.find((n) => n.key === key)!;
					inspect(target, target.data!, state.nodes, state.edges);
				} catch (error) {
					generation++;
					publish({
						...state,
						busy: false,
						nodes: state.nodes.map((n) => ({
							...n,
							data: null,
							status: 'error',
							error: null,
							checkedAt: null
						})),
						conflict:
							error instanceof Error
								? `${error.message} Evidence cleared; restart from the seed.`
								: 'Conflicting evidence; restart.'
					});
				}
			}
		},
		stop() {
			stopped = true;
			generation++;
			abort?.abort();
		}
	};
}

export function encodePlan(state: GraphState): string {
	if (!state.nodes.length) throw new Error('Load or select a seed first.');
	const raw = JSON.stringify({
		v: 1,
		n: state.nodes.map((n) => [n.kind, n.id, ...(n.pin ? [n.pin.height, n.pin.block_id] : [])]),
		e: state.edges.map((e) => [
			state.nodes.findIndex((n) => n.key === e.from),
			state.nodes.findIndex((n) => n.key === e.to),
			e.relation
		])
	});
	const encoded = btoa(raw).replaceAll('+', '-').replaceAll('/', '_').replace(/=+$/, '');
	if (encoded.length > MAX_PLAN_LENGTH)
		throw new Error(
			'This investigation exceeds the share-link budget. Export the evidence JSON instead.'
		);
	return encoded;
}
export function decodePlan(encoded: string): Plan {
	function invalid(): never {
		throw new Error('This investigation link is invalid or exceeds its limits.');
	}
	try {
		if (!encoded || encoded.length > MAX_PLAN_LENGTH || !/^[\w-]+$/.test(encoded)) invalid();
		const value = JSON.parse(atob(encoded.replaceAll('-', '+').replaceAll('_', '/')));
		if (
			!value ||
			value.v !== 1 ||
			!Array.isArray(value.n) ||
			!value.n.length ||
			value.n.length > MAX_NODES ||
			!Array.isArray(value.e) ||
			value.e.length > MAX_EDGES
		)
			invalid();
		const nodes = value.n.map((row: unknown) => {
			if (!Array.isArray(row) || ![2, 4].includes(row.length)) invalid();
			const ref = seedRef(row[0], row[1]);
			if (!ref || row[1] !== ref.id) invalid();
			if (row.length === 4 && (ref.kind !== 'tx' || !integer(row[2]) || !hash(row[3]))) invalid();
			return { ...ref, ...(row.length === 4 ? { pin: { height: row[2], block_id: row[3] } } : {}) };
		}) as Plan['nodes'];
		if (new Set(nodes.map(keyOf)).size !== nodes.length) invalid();
		const edges = value.e.map((row: unknown) => {
			if (
				!Array.isArray(row) ||
				row.length !== 3 ||
				!integer(row[0]) ||
				!integer(row[1]) ||
				row[0] >= nodes.length ||
				row[1] >= nodes.length ||
				!['creates', 'spends', 'reads'].includes(row[2])
			)
				invalid();
			const from = nodes[row[0]],
				to = nodes[row[1]];
			if (
				from.kind === to.kind ||
				(row[2] === 'creates' ? from.kind !== 'tx' : from.kind !== 'box')
			)
				invalid();
			return { from: keyOf(from), to: keyOf(to), relation: row[2] };
		}) as Edge[];
		if (new Set(edges.map((e) => JSON.stringify(e))).size !== edges.length) invalid();
		return { nodes, edges };
	} catch {
		return invalid();
	}
}

export function exportEvidence(state: GraphState, now = Date.now()): string {
	if (state.conflict || !state.nodes.some((n) => n.data))
		throw new Error('There is no loaded evidence to export.');
	const output = JSON.stringify(
		{
			format: 'kadia.investigation',
			version: 1,
			exported_at_ms: now,
			scope: 'independent_indexed_reads',
			note: 'References establish indexed box membership, not ownership or allocation of value between inputs and outputs. Inclusion pins are observations, not proof that all reads share a canonical branch.',
			nodes: state.nodes.map(({ key, ...node }) => ({ key, ...node })),
			edges: state.edges.map((edge) => ({ ...edge, observed: edgeObserved(edge, state.nodes) }))
		},
		null,
		2
	);
	if (new TextEncoder().encode(output).length > MAX_EVIDENCE_BYTES * 2)
		throw new Error('The formatted evidence exceeds the 8 MiB export limit.');
	return output;
}
