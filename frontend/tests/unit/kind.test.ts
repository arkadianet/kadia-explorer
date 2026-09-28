import { describe, expect, it } from 'vitest';
import { RENT_PERIOD, txKind } from '../../src/lib/tx/kind.ts';
import type { BoxDto, TxDto } from '../../src/lib/api/types.ts';

function box(over: Partial<BoxDto>): BoxDto {
	return {
		id: 'b',
		tx_id: 't',
		index: 0,
		value: '1000000000',
		creation_height: 1_000_000,
		ergo_tree: '0008cd03a5b9ae24950ee854694a9832914324a6b69121fcd468bcc7c4023b5874f353cf',
		address: null,
		template_hash: null,
		tree_hash: 'tree-a',
		tokens: [],
		registers: null,
		size: 80,
		spent_by: null,
		spent_height: null,
		rent: {
			maturity_height: 0,
			due_nano: '0',
			claimable_at_tip: false,
			consensus_fee_nano: '100000000',
			collectible: true
		},
		...over
	};
}

function tx(over: Partial<TxDto>): TxDto {
	return {
		id: 'tx',
		height: 2_000_000,
		index: 0,
		timestamp: 0,
		size: 100,
		fee: '0',
		inputs: [],
		data_inputs: [],
		outputs: [],
		...over
	};
}

describe('txKind', () => {
	it('recognizes emission and fee collection from indexed contract identities', () => {
		const emission = box({ kind: 'emission' });
		expect(txKind(tx({ inputs: [{ id: emission.id, box: emission }] })).kind).toBe('emission');
		const fee = box({ kind: 'fee' });
		expect(txKind(tx({ inputs: [{ id: fee.id, box: fee }] })).kind).toBe('fee');
	});
	it('does not infer rent from an old input returning to its own script', () => {
		const spent = box({ creation_height: 2_000_000 - RENT_PERIOD, tree_hash: 'tree-a' });
		const result = txKind(
			tx({ inputs: [{ id: spent.id, box: spent }], outputs: [box({ tree_hash: 'tree-a' })] })
		);
		expect(result.kind).toBe('unknown');
	});

	it('does not call it rent when the old value goes somewhere else', () => {
		const spent = box({ creation_height: 2_000_000 - RENT_PERIOD, tree_hash: 'tree-a' });
		const result = txKind(
			tx({ inputs: [{ id: spent.id, box: spent }], outputs: [box({ tree_hash: 'tree-b' })] })
		);
		expect(result.kind).toBe('unknown');
	});

	it('does not call it rent one block short of the rent period', () => {
		const spent = box({ creation_height: 2_000_000 - RENT_PERIOD + 1, tree_hash: 'tree-a' });
		const result = txKind(
			tx({ inputs: [{ id: spent.id, box: spent }], outputs: [box({ tree_hash: 'tree-a' })] })
		);
		expect(result.kind).toBe('payment');
	});

	it('does not infer a transfer when input coverage is absent', () => {
		const result = txKind(
			tx({ outputs: [box({ tokens: [{ id: 'token', amount: '5', name: null, decimals: null }] })] })
		);
		expect(result.kind).toBe('unknown');
	});

	it('keeps unknown inputs explicit', () => {
		const result = txKind(tx({ inputs: [{ id: 'unknown', box: null }], outputs: [box({})] }));
		expect(result.kind).toBe('unknown');
	});
});
