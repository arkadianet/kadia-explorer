import { describe, expect, it, vi } from 'vitest';
import { readFileSync } from 'node:fs';
import { ApiError } from '../../src/lib/api/client';
import type { BoxDto, TxDto } from '../../src/lib/api/types';
import {
	createInvestigation,
	decodePlan,
	encodePlan,
	exportEvidence,
	references,
	seedRef,
	MAX_NODES,
	MAX_EDGES,
	MAX_PLAN_LENGTH,
	MAX_NODE_BYTES,
	type Plan
} from '../../src/lib/investigation/graph';

const id = (n: number) => n.toString(16).padStart(64, '0');
const seed = { kind: 'tx' as const, id: id(1) };
function box(n = 2, index = 0): BoxDto {
	return {
		id: id(n),
		tx_id: seed.id,
		index,
		value: '9007199254740993123',
		creation_height: 5,
		ergo_tree: null,
		address: null,
		tree_hash: id(90),
		template_hash: null,
		tokens: [{ id: id(80), amount: '18446744073709551615', name: null, decimals: null }],
		registers: null,
		size: 123,
		spent_by: n === 2 ? id(4) : null,
		spent_height: n === 2 ? 11 : null,
		rent: {
			maturity_height: 100,
			due_nano: '1',
			claimable_at_tip: false,
			consensus_fee_nano: '1',
			collectible: true
		}
	};
}
function tx(): TxDto {
	return {
		id: seed.id,
		block_id: id(100),
		height: 10,
		indexed_height: 20,
		confirmations: 11,
		index: 0,
		timestamp: 1_000,
		size: 300,
		fee: '1000000',
		inputs: [{ id: id(5), box: null }],
		outputs: [box(), box(3, 1)],
		data_inputs: [id(6)]
	};
}
function spending(): TxDto {
	return {
		...tx(),
		id: id(4),
		block_id: id(101),
		height: 11,
		inputs: [{ id: id(2), box: box() }],
		outputs: [],
		data_inputs: []
	};
}
function setup(plan: Plan = { nodes: [seed], edges: [] }) {
	const api = {
		tx: vi.fn(async (value: string) => (value === id(4) ? spending() : tx())),
		box: vi.fn(async (value: string) => (value === id(3) ? box(3, 1) : box()))
	};
	const change = vi.fn();
	const graph = createInvestigation({ ...api, onChange: change, now: () => 1234 });
	graph.restore(plan);
	return { graph, api, change };
}
async function openBox(graph: ReturnType<typeof createInvestigation>) {
	await graph.load(`tx:${seed.id}`);
	const output = references(graph.state.nodes[0]).find((r) => r.edge.relation === 'creates')!;
	await graph.expand(`tx:${seed.id}`, output);
}

describe('bounded explicit investigation', () => {
	it('normalizes explicit seeds without guessing hash entity kinds', () => {
		expect(seedRef('tx', 'A'.repeat(64))).toEqual({ kind: 'tx', id: 'a'.repeat(64) });
		expect(seedRef(null, id(1))).toBeNull();
		expect(seedRef('address', id(1))).toBeNull();
		expect(seedRef('box', 'no')).toBeNull();
	});
	it('restores locally, loads only one node, and never fans out automatically', async () => {
		const { graph, api } = setup();
		expect(api.tx).not.toHaveBeenCalled();
		expect(api.box).not.toHaveBeenCalled();
		await graph.load(`tx:${seed.id}`);
		expect(api.tx).toHaveBeenCalledTimes(1);
		expect(api.box).not.toHaveBeenCalled();
		expect(graph.state.nodes).toHaveLength(1);
		expect(references(graph.state.nodes[0])).toHaveLength(4);
		expect(graph.state.nodes[0].pin).toEqual({ height: 10, block_id: id(100) });
	});
	it('follows multiple steps with typed membership edges and deduplicated nodes', async () => {
		const { graph, api } = setup();
		await openBox(graph);
		const linkedBox = graph.state.nodes[1];
		const spender = references(linkedBox).find((r) => r.edge.relation === 'spends')!;
		await graph.expand(linkedBox.key, spender);
		expect(api.box).toHaveBeenCalledTimes(1);
		expect(api.tx).toHaveBeenCalledTimes(2);
		expect(graph.state.edges.map((e) => e.relation)).toEqual(['creates', 'spends']);
		const producer = references(linkedBox).find((r) => r.edge.relation === 'creates')!;
		await graph.expand(linkedBox.key, producer);
		expect(graph.state.nodes).toHaveLength(3);
		expect(graph.state.edges).toHaveLength(2);
		expect(api.tx).toHaveBeenCalledTimes(2);
	});
	it('loads the recorded Rosen source and expands its deposit with verified spend heights', async () => {
		const fixture = JSON.parse(
			readFileSync(
				new URL('../../../tests/fixtures/apps/rosen-erg-cardano.json', import.meta.url),
				'utf8'
			)
		) as { sourceTx: TxDto; deposit: BoxDto };
		const graph = createInvestigation({
			tx: vi.fn(async () => fixture.sourceTx),
			box: vi.fn(async () => fixture.deposit),
			onChange: vi.fn()
		});
		const sourceKey = `tx:${fixture.sourceTx.id}`;
		graph.restore({ nodes: [{ kind: 'tx', id: fixture.sourceTx.id }], edges: [] });
		await graph.load(sourceKey);
		expect(graph.state.conflict).toBeNull();
		expect(graph.state.nodes[0].status).toBe('ready');
		expect(fixture.sourceTx.outputs[2]).toMatchObject({
			spent_by: '58160baab41d222c19b9e3239776cf1da5c4d47c0842dcedb1bb008c913744a0',
			spent_height: 1878795
		});
		const deposit = references(graph.state.nodes[0]).find(
			(reference) =>
				reference.edge.relation === 'creates' && reference.ref.id === fixture.deposit.id
		)!;
		await graph.expand(sourceKey, deposit);
		expect(graph.state.conflict).toBeNull();
		expect(graph.state.nodes[1]).toMatchObject({
			status: 'ready',
			data: {
				id: fixture.deposit.id,
				value: '418527417582',
				spent_by: '0a6ffdc0dc55e88b128ae03be2e59290226d2321d4186e643274763115b9f651',
				spent_height: 1878804
			}
		});
		expect(graph.state.edges).toEqual([
			{ from: sourceKey, to: `box:${fixture.deposit.id}`, relation: 'creates' }
		]);
	});
	it('preserves unresolved input IDs and distinguishes read-only data inputs', async () => {
		const { graph } = setup();
		await graph.load(`tx:${seed.id}`);
		const refs = references(graph.state.nodes[0]);
		expect(refs.find((r) => r.ref.id === id(5))).toMatchObject({
			resolved: false,
			edge: { relation: 'spends' }
		});
		expect(refs.find((r) => r.ref.id === id(6))).toMatchObject({
			label: 'Data input (read only)',
			edge: { relation: 'reads' }
		});
	});
	it('checks box amounts and transaction inclusion with exact integers', async () => {
		const { graph } = setup();
		await openBox(graph);
		expect((graph.state.nodes[1].data as BoxDto).value).toBe('9007199254740993123');
		expect((graph.state.nodes[1].data as BoxDto).tokens[0].amount).toBe('18446744073709551615');
		expect(graph.state.conflict).toBeNull();
	});
	it.each(['amount', 'membership', 'spend', 'inclusion'])(
		'clears prior evidence on conflicting %s reads',
		async (kind) => {
			const { graph, api } = setup();
			if (kind === 'amount') {
				api.box.mockResolvedValue({ ...box(), value: '1' });
				await openBox(graph);
			} else if (kind === 'inclusion') {
				await graph.load(`tx:${seed.id}`);
				api.tx.mockResolvedValue({ ...tx(), block_id: id(999) });
				await graph.load(`tx:${seed.id}`);
			} else {
				await openBox(graph);
				api.tx.mockResolvedValue(
					kind === 'membership' ? { ...spending(), inputs: [] } : { ...spending(), height: 12 }
				);
				await graph.expand(
					graph.state.nodes[1].key,
					references(graph.state.nodes[1]).find((r) => r.edge.relation === 'spends')!
				);
			}
			expect(graph.state.conflict).toBeTruthy();
			expect(graph.state.nodes.every((n) => n.data === null)).toBe(true);
			expect(() => exportEvidence(graph.state)).toThrow('no loaded evidence');
		}
	);
	it('accepts a newly observed spend and allows following its spending transaction', async () => {
		const { graph, api } = setup();
		api.box.mockResolvedValueOnce({ ...box(), spent_by: null, spent_height: null });
		await openBox(graph);
		const key = graph.state.nodes[1].key;
		await graph.load(key);
		expect(graph.state.conflict).toBeNull();
		expect(graph.state.nodes[1]).toMatchObject({ status: 'ready', data: box() });
		await graph.expand(
			key,
			references(graph.state.nodes[1]).find((r) => r.edge.relation === 'spends')!
		);
		expect(graph.state.conflict).toBeNull();
		expect(graph.state.nodes[2]).toMatchObject({ status: 'ready', data: spending() });
	});
	it.each([
		{ spent_by: null, spent_height: null },
		{ spent_by: id(7), spent_height: 11 },
		{ spent_by: id(4), spent_height: 12 }
	])('rejects a changed previously observed spend: %j', async (spend) => {
		const { graph, api } = setup();
		await openBox(graph);
		api.box.mockResolvedValueOnce({ ...box(), ...spend });
		await graph.load(graph.state.nodes[1].key);
		expect(graph.state.conflict).toContain('spending reference for a loaded box changed');
		expect(graph.state.nodes.every((node) => node.data === null)).toBe(true);
	});
	it('withholds graph after a pinned shared inclusion fails validation', async () => {
		const { graph } = setup({
			nodes: [{ ...seed, pin: { height: 10, block_id: id(500) } }],
			edges: []
		});
		await graph.load(`tx:${seed.id}`);
		expect(graph.state.conflict).toContain('pinned transaction inclusion');
		expect(graph.state.nodes[0].data).toBeNull();
	});
	it('shows transient refresh failures as stale and requires refresh before expansion', async () => {
		const { graph, api } = setup();
		await graph.load(`tx:${seed.id}`);
		api.tx.mockRejectedValueOnce(new ApiError(503, 'Busy', 'Retry'));
		await graph.load(`tx:${seed.id}`);
		expect(graph.state.nodes[0].status).toBe('stale');
		await graph.expand(`tx:${seed.id}`, references(graph.state.nodes[0])[1]);
		expect(api.box).not.toHaveBeenCalled();
		await graph.load(`tx:${seed.id}`);
		expect(graph.state.nodes[0].status).toBe('ready');
	});
	it('treats removed formerly loaded details as incompatible but initial missing details as unavailable', async () => {
		const { graph, api } = setup();
		api.tx.mockRejectedValueOnce(new ApiError(404, 'Missing', 'Not indexed'));
		await graph.load(`tx:${seed.id}`);
		expect(graph.state.conflict).toBeNull();
		expect(graph.state.nodes[0].error).toContain('Not available');
		await graph.load(`tx:${seed.id}`);
		api.tx.mockRejectedValueOnce(new ApiError(404, 'Missing', 'Not indexed'));
		await graph.load(`tx:${seed.id}`);
		expect(graph.state.conflict).toContain('no longer available');
	});
	it('bounds transaction references and raw node bytes without truncating monetary evidence', async () => {
		const { graph, api } = setup();
		api.tx.mockResolvedValueOnce({ ...tx(), data_inputs: Array(1001).fill(id(99)) });
		await graph.load(`tx:${seed.id}`);
		expect(graph.state.nodes[0].error).toContain('1,000-reference');
		expect(graph.state.nodes[0].data).toBeNull();
		api.tx.mockResolvedValueOnce({
			...tx(),
			outputs: [{ ...box(), registers: { R4: 'a'.repeat(MAX_NODE_BYTES) } }]
		});
		await graph.load(`tx:${seed.id}`);
		expect(graph.state.nodes[0].error).toContain('1 MiB');
	});
	it('enforces the node cap before any extra request', async () => {
		const plan = {
			nodes: [
				seed,
				...Array.from({ length: MAX_NODES - 1 }, (_, index) => ({
					kind: 'box' as const,
					id: id(200 + index)
				}))
			],
			edges: []
		};
		const { graph, api } = setup(plan);
		await graph.load(`tx:${seed.id}`);
		await graph.expand(`tx:${seed.id}`, references(graph.state.nodes[0])[1]);
		expect(api.box).not.toHaveBeenCalled();
		expect(graph.state.nodes).toHaveLength(MAX_NODES);
		expect(graph.state.notice).toContain('limit reached');
	});
	it('bounds graph connections before loading an existing unloaded node', async () => {
		const nodes: Plan['nodes'] = [
			seed,
			{ kind: 'box', id: id(2) },
			...Array.from({ length: 5 }, (_, index) => ({ kind: 'tx' as const, id: id(200 + index) })),
			...Array.from({ length: 5 }, (_, index) => ({ kind: 'box' as const, id: id(300 + index) }))
		];
		const edges: Plan['edges'] = [];
		for (const txNode of nodes.filter((n) => n.kind === 'tx'))
			for (const boxNode of nodes.filter((n) => n.kind === 'box')) {
				if (txNode.id === seed.id && boxNode.id === id(2)) continue;
				edges.push(
					{ from: `tx:${txNode.id}`, to: `box:${boxNode.id}`, relation: 'creates' },
					{ from: `box:${boxNode.id}`, to: `tx:${txNode.id}`, relation: 'spends' }
				);
			}
		const { graph, api } = setup({ nodes, edges: edges.slice(0, MAX_EDGES) });
		await graph.load(`tx:${seed.id}`);
		await graph.expand(
			`tx:${seed.id}`,
			references(graph.state.nodes[0]).find((r) => r.ref.id === id(2))!
		);
		expect(api.box).not.toHaveBeenCalled();
		expect(graph.state.edges).toHaveLength(MAX_EDGES);
		expect(graph.state.notice).toContain('limit reached');
	});
	it('keeps admitted evidence when the aggregate byte budget is full', async () => {
		const nodes: Plan['nodes'] = Array.from({ length: 5 }, (_, index) => ({
			kind: 'box',
			id: id(200 + index)
		}));
		const { graph, api } = setup({ nodes, edges: [] });
		api.box.mockImplementation(async (value) => ({
			...box(),
			id: value,
			registers: { R4: 'a'.repeat(900_000) }
		}));
		for (const node of nodes) await graph.load(`box:${node.id}`);
		expect(graph.state.nodes.filter((n) => n.data)).toHaveLength(4);
		expect(graph.state.nodes[4].error).toContain('4 MiB');
		expect(graph.state.nodes[4].data).toBeNull();
	});
	it('detects conflicting block identities at a shared height across separate transactions', async () => {
		const { graph, api } = setup({ nodes: [seed, { kind: 'tx', id: id(7) }], edges: [] });
		await graph.load(`tx:${seed.id}`);
		api.tx.mockResolvedValue({
			...tx(),
			id: id(7),
			block_id: id(700),
			inputs: [],
			outputs: [],
			data_inputs: []
		});
		await graph.load(`tx:${id(7)}`);
		expect(graph.state.conflict).toContain('canonical block at one height');
		expect(graph.state.nodes.every((n) => !n.data)).toBe(true);
	});
	it('allows only one request at once and ignores results after reset or navigation', async () => {
		let finish!: (value: TxDto) => void;
		const { graph, api, change } = setup();
		api.tx.mockImplementationOnce(() => new Promise((resolve) => (finish = resolve)));
		const waiting = graph.load(`tx:${seed.id}`);
		await graph.load(`tx:${seed.id}`);
		expect(api.tx).toHaveBeenCalledTimes(1);
		graph.restore({ nodes: [{ kind: 'box', id: id(3) }], edges: [] });
		finish(tx());
		await waiting;
		expect(graph.state.nodes[0].data).toBeNull();
		expect(graph.state.nodes[0].kind).toBe('box');
		let finishBox!: (value: BoxDto) => void;
		api.box.mockImplementationOnce(() => new Promise((resolve) => (finishBox = resolve)));
		const waitingBox = graph.load(`box:${id(3)}`);
		graph.stop();
		const count = change.mock.calls.length;
		finishBox(box(3, 1));
		await waitingBox;
		await graph.load(`box:${id(3)}`);
		expect(change).toHaveBeenCalledTimes(count);
	});
});

describe('pinned share plans and exact evidence export', () => {
	it('round trips the explicit path and pins without carrying remote evidence', async () => {
		const { graph } = setup();
		await openBox(graph);
		const encoded = encodePlan(graph.state);
		expect(encoded.length).toBeLessThan(MAX_PLAN_LENGTH);
		const plan = decodePlan(encoded);
		expect(plan.nodes).toHaveLength(2);
		expect(plan.nodes[0].pin).toEqual({ height: 10, block_id: id(100) });
		expect(plan.edges).toEqual(graph.state.edges);
		const other = setup(plan);
		expect(other.api.tx).not.toHaveBeenCalled();
		expect(other.api.box).not.toHaveBeenCalled();
		expect(other.graph.state.nodes.every((n) => n.data === null)).toBe(true);
	});
	it.each([
		'',
		'a'.repeat(MAX_PLAN_LENGTH + 1),
		'not json!',
		btoa(JSON.stringify({ v: 2, n: [['tx', id(1)]], e: [] })),
		btoa(
			JSON.stringify({
				v: 1,
				n: [
					['tx', id(1)],
					['tx', id(2)]
				],
				e: [[0, 1, 'creates']]
			})
		),
		btoa(JSON.stringify({ v: 1, n: [['box', id(2), 10, id(100)]], e: [] }))
	])('rejects invalid or excessive share plans', (raw) => {
		expect(() => decodePlan(raw)).toThrow('invalid or exceeds');
	});
	it('exports exact quantities, independent read scope, unresolved coverage and pins', async () => {
		const { graph } = setup();
		await openBox(graph);
		const exported = JSON.parse(exportEvidence(graph.state, 5678));
		expect(exported.format).toBe('kadia.investigation');
		expect(exported.scope).toBe('independent_indexed_reads');
		expect(exported.exported_at_ms).toBe(5678);
		expect(exported.nodes[1].data.value).toBe('9007199254740993123');
		expect(exported.nodes[0].data.inputs[0].box).toBeNull();
		expect(exported.nodes[0].checkedAt).toBe(1234);
		expect(exported.edges[0].observed).toBe(true);
	});
});
