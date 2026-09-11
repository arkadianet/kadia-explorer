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
		ergo_tree: null,
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
	it('calls it a storage-rent claim when an old input returns to its own script', () => {
		const spent = box({ creation_height: 2_000_000 - RENT_PERIOD, tree_hash: 'tree-a' });
		const result = txKind(
			tx({ inputs: [{ id: spent.id, box: spent }], outputs: [box({ tree_hash: 'tree-a' })] })
		);
		expect(result.kind).toBe('rent');
		expect(result.label).toBe('Storage rent claim');
	});

	it('does not call it rent when the old value goes somewhere else', () => {
		const spent = box({ creation_height: 2_000_000 - RENT_PERIOD, tree_hash: 'tree-a' });
		const result = txKind(
			tx({ inputs: [{ id: spent.id, box: spent }], outputs: [box({ tree_hash: 'tree-b' })] })
		);
		expect(result.kind).toBe('payment');
	});

	it('does not call it rent one block short of the rent period', () => {
		const spent = box({ creation_height: 2_000_000 - RENT_PERIOD + 1, tree_hash: 'tree-a' });
		const result = txKind(
			tx({ inputs: [{ id: spent.id, box: spent }], outputs: [box({ tree_hash: 'tree-a' })] })
		);
		expect(result.kind).toBe('payment');
	});

	it('calls it a token transfer when a token moves', () => {
		const result = txKind(
			tx({ outputs: [box({ tokens: [{ id: 'token', amount: '5', name: null, decimals: null }] })] })
		);
		expect(result.kind).toBe('token');
	});

	it('ignores inputs the indexer could not resolve', () => {
		const result = txKind(tx({ inputs: [{ id: 'unknown', box: null }], outputs: [box({})] }));
		expect(result.kind).toBe('payment');
	});
});
