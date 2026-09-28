import { describe, expect, it, vi } from 'vitest';
import { readFileSync } from 'node:fs';
import { ApiError } from '$lib/api/client';
import type { TxDto } from '$lib/api/types';
import {
	createLineage,
	inspectLineage,
	LineageBudget,
	LineageMismatch,
	MAX_LINEAGE_REFERENCES
} from '$lib/boxes/lineage';

const tx: TxDto = JSON.parse(
	readFileSync(
		new URL('../../../tests/fixtures/receipts/storage-rent-tx.json', import.meta.url),
		'utf8'
	)
);
const box = { ...tx.outputs[0], spent_by: 'c'.repeat(64), spent_height: tx.height + 1 };
const spending: TxDto = {
	...tx,
	id: box.spent_by,
	height: box.spent_height,
	indexed_height: box.spent_height + 2,
	inputs: [{ id: box.id, box }],
	outputs: []
};

describe('box lineage facts and bounds', () => {
	it('uses producing inclusion independently of the declared box creation height', () => {
		const oldCreation = { ...box, creation_height: 1 };
		const result = inspectLineage(oldCreation, 'producer', tx);
		expect(result.tx.height).toBe(tx.height);
		expect(result.outputs.find((row) => row.id === box.id)?.box?.value).toBe(box.value);
		expect(result.inputs).toHaveLength(tx.inputs.length);
	});
	it('keeps unresolved input IDs without inventing values or complete totals', () => {
		const partial = { ...tx, inputs: [{ id: 'a'.repeat(64), box: null }] };
		const result = inspectLineage(box, 'producer', partial);
		expect(result.missingInputs).toBe(1);
		expect(result.inputs[0]).toEqual({ id: 'a'.repeat(64), box: null });
	});
	it('rejects wrong transactions, output indexes, values, and spend membership', () => {
		expect(() => inspectLineage(box, 'producer', { ...tx, id: 'f'.repeat(64) })).toThrow(
			LineageMismatch
		);
		expect(() => inspectLineage({ ...box, index: 99 }, 'producer', tx)).toThrow(LineageMismatch);
		expect(() => inspectLineage({ ...box, value: '1' }, 'producer', tx)).toThrow(LineageMismatch);
		expect(() => inspectLineage(box, 'spender', { ...spending, inputs: [] })).toThrow(
			LineageMismatch
		);
		expect(() => inspectLineage({ ...box, spent_height: 1 }, 'spender', spending)).toThrow(
			LineageMismatch
		);
	});
	it('rejects inconsistent resolved spend inputs while retaining explicitly unresolved references', () => {
		for (const change of [{ value: '1' }, { index: 99 }, { tx_id: 'e'.repeat(64) }]) {
			expect(() =>
				inspectLineage(box, 'spender', {
					...spending,
					inputs: [{ id: box.id, box: { ...box, ...change } }]
				})
			).toThrow(LineageMismatch);
		}
		const unresolved = inspectLineage(box, 'spender', {
			...spending,
			inputs: [{ id: box.id, box: null }]
		});
		expect(unresolved.missingInputs).toBe(1);
	});
	it('admits a bounded reference count before validating every box', () => {
		expect(() =>
			inspectLineage(box, 'producer', {
				...tx,
				inputs: Array(MAX_LINEAGE_REFERENCES + 1).fill({ id: 'a'.repeat(64), box: null })
			})
		).toThrow(LineageBudget);
	});
	it('fetches at most each explicitly requested neighbour and never fetches recursively', async () => {
		const getTx = vi.fn(async (id: string) => (id === tx.id ? tx : spending));
		const onChange = vi.fn();
		const controller = createLineage({ box, getTx, onChange });
		expect(getTx).not.toHaveBeenCalled();
		await controller.load('producer');
		await controller.load('producer');
		expect(getTx).toHaveBeenCalledTimes(1);
		await controller.load('spender');
		expect(getTx.mock.calls.map(([id]) => id)).toEqual([tx.id, spending.id]);
		expect(onChange.mock.lastCall?.[0].spender.status).toBe('ready');
	});
	it('ignores a late result after navigation and deduplicates in-flight clicks', async () => {
		let resolve!: (value: TxDto) => void;
		const getTx = vi.fn(
			() =>
				new Promise<TxDto>((done) => {
					resolve = done;
				})
		);
		const onChange = vi.fn();
		const controller = createLineage({ box, getTx, onChange });
		const pending = controller.load('producer');
		await controller.load('producer');
		controller.stop();
		resolve(tx);
		await pending;
		expect(getTx).toHaveBeenCalledTimes(1);
		expect(onChange).toHaveBeenCalledTimes(1);
		expect(onChange.mock.lastCall?.[0].producer.status).toBe('loading');
	});
	it('separates unavailable and budget failures and allows an explicit retry', async () => {
		const getTx = vi
			.fn()
			.mockRejectedValueOnce(new ApiError(422, 'Budget', 'too large'))
			.mockRejectedValueOnce(new ApiError(404, 'Missing', 'missing'))
			.mockResolvedValueOnce(tx);
		const onChange = vi.fn();
		const controller = createLineage({ box, getTx, onChange });
		await controller.load('producer');
		expect(onChange.mock.lastCall?.[0].producer.status).toBe('budget');
		await controller.load('producer');
		expect(onChange.mock.lastCall?.[0].producer.status).toBe('unavailable');
		await controller.load('producer');
		expect(onChange.mock.lastCall?.[0].producer.status).toBe('ready');
	});
	it('never requests nonexistent genesis or unspent neighbours', async () => {
		const getTx = vi.fn();
		const controller = createLineage({
			box: { ...box, tx_id: '0'.repeat(64), spent_by: null, spent_height: null },
			getTx,
			onChange: vi.fn()
		});
		await controller.load('producer');
		await controller.load('spender');
		expect(getTx).not.toHaveBeenCalled();
	});
});
