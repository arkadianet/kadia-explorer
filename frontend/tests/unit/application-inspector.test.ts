import { describe, expect, it, vi } from 'vitest';
import {
	applicationSelector,
	applicationLink,
	createApplicationInspector,
	type InspectorState
} from '../../src/lib/apps/inspector';
import type { BoxDto, TxDto } from '../../src/lib/api/types';

const id = 'a'.repeat(64),
	block = 'b'.repeat(64);
const box = {
	id,
	tx_id: 'c'.repeat(64),
	value: '1',
	tokens: [],
	ergo_tree: null
} as unknown as BoxDto;
const tx = { id, block_id: block, inputs: [], outputs: [] } as unknown as TxDto;
function setup() {
	const states: InspectorState[] = [];
	const getTx = vi.fn(async () => tx),
		getBox = vi.fn(async () => box);
	const inspector = createApplicationInspector({
		getTx,
		getBox,
		onChange: (value) => states.push(value)
	});
	return { states, getTx, getBox, inspector };
}
describe('application discovery controller', () => {
	it('normalizes IDs and pins only transactions', () => {
		expect(
			applicationSelector('tx', ' ' + id.toUpperCase() + ' ', ' ' + block.toUpperCase() + ' ')
		).toEqual({
			kind: 'tx',
			id,
			block
		});
		expect(applicationLink({ kind: 'tx', id, block })).toBe(
			`/applications?kind=tx&id=${id}&block=${block}`
		);
		expect(() => applicationSelector('box', id, block)).toThrow();
		expect(() => applicationSelector('tx', '../status')).toThrow();
	});
	it.each([undefined, null, '', ' \t '])('omits an empty block pin (%s)', (pin) => {
		for (const kind of ['tx', 'box'])
			expect(applicationSelector(kind, id, pin)).toEqual({ kind, id });
	});
	it('does no automatic reading, then uses only the requested detail route', async () => {
		const { inspector, getTx, getBox, states } = setup();
		expect(getTx).not.toHaveBeenCalled();
		expect(getBox).not.toHaveBeenCalled();
		await inspector.load({ kind: 'box', id });
		expect(getBox).toHaveBeenCalledOnce();
		expect(getTx).not.toHaveBeenCalled();
		expect(states.at(-1)?.evidence).toEqual({ kind: 'box', value: box });
	});
	it('rejects a changed or missing inclusion for a pinned transaction', async () => {
		const { inspector, states } = setup();
		await inspector.load({ kind: 'tx', id, block: 'd'.repeat(64) });
		expect(states.at(-1)).toMatchObject({ evidence: null, loading: false });
		expect(states.at(-1)?.error).toContain('pinned inclusion');
	});
	it('clears a previous interpretation before refreshing and after failure', async () => {
		const { inspector, getBox, states } = setup();
		await inspector.load({ kind: 'box', id });
		getBox.mockRejectedValueOnce(new Error('Read limit'));
		await inspector.load({ kind: 'box', id });
		expect(states.at(-2)).toEqual({ loading: true, evidence: null, error: '' });
		expect(states.at(-1)).toEqual({ loading: false, evidence: null, error: 'Read limit' });
	});
	it('discards a late result after editing, navigating or another request', async () => {
		const { inspector, getBox, states } = setup();
		let resolve!: (box: BoxDto) => void;
		getBox.mockImplementationOnce(
			() =>
				new Promise((done) => {
					resolve = done;
				})
		);
		const loading = inspector.load({ kind: 'box', id });
		inspector.reset();
		resolve(box);
		await loading;
		expect(states.at(-1)).toEqual({ loading: false, evidence: null, error: '' });
		inspector.stop();
		await inspector.load({ kind: 'tx', id });
		expect(states).toHaveLength(2);
	});
	it('rejects malformed or oversized transaction evidence before decoding', async () => {
		for (const bad of [
			{ ...tx, outputs: Array(101).fill(box) },
			{ ...tx, outputs: [null] },
			{ ...tx, id: block }
		]) {
			const { inspector, states, getTx } = setup();
			getTx.mockResolvedValue(bad as TxDto);
			await inspector.load({ kind: 'tx', id });
			expect(states.at(-1)?.evidence).toBeNull();
			expect(states.at(-1)?.error).not.toBe('');
		}
	});
	it('rejects mismatched input references, output membership and duplicate boxes', async () => {
		const output = { ...box, tx_id: id, index: 0 };
		for (const bad of [
			{ ...tx, inputs: [{ id: block, box }] },
			{ ...tx, outputs: [{ ...output, tx_id: block }] },
			{ ...tx, outputs: [{ ...output, index: 1 }] },
			{ ...tx, outputs: [output, { ...output, index: 1 }] },
			{
				...tx,
				inputs: [
					{ id, box },
					{ id, box }
				]
			}
		]) {
			const { inspector, states, getTx } = setup();
			getTx.mockResolvedValue(bad as TxDto);
			await inspector.load({ kind: 'tx', id });
			expect(states.at(-1)).toMatchObject({
				evidence: null,
				loading: false,
				error: 'Transaction box evidence is malformed.'
			});
		}
		const { inspector, states, getTx } = setup();
		getTx.mockResolvedValue({ ...tx, outputs: [output] });
		await inspector.load({ kind: 'tx', id });
		expect(states.at(-1)?.evidence?.kind).toBe('tx');
	});
});
