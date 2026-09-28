import { describe, expect, it } from 'vitest';
import txJson from '../../../tests/fixtures/receipts/storage-rent-tx.json';
import blockJson from '../../../tests/fixtures/receipts/storage-rent-block.json';
import type { TxDto, TxEvidence } from '$lib/api/types';
import { transactionEffects } from '$lib/tx/effects';
import { rentPointer, txKind } from '$lib/tx/kind';

const tx = txJson as TxDto;
const evidence: TxEvidence = {
	tx_id: tx.id,
	block_id: tx.block_id!,
	height: tx.height,
	assurance: 'trusted_node_response',
	inputs: blockJson.blockTransactions.transactions[tx.index].inputs.map((input) => ({
		id: input.boxId,
		proof: input.spendingProof.proofBytes === '' ? 'empty' : 'nonempty',
		extension_127: (input.spendingProof.extension as Record<string, string>)['127'] ?? null
	}))
};

describe('real storage rent claim', () => {
	it('recognizes the fully consumed P2PK box even without a return to its script', () => {
		expect(txKind(tx, evidence).kind).toBe('rent');
		expect(txKind(tx).kind).toBe('unknown');
	});
	it('conserves exact assets and separates the externally funded fee', () => {
		const result = transactionEffects(tx);
		expect(result.complete).toBe(true);
		expect(result.ergTotal).toBe(0n);
		expect(result.supplyChanges).toEqual([]);
		const recipient = result.addresses.find((a) => a.address?.startsWith('9give'))!;
		const expired = result.addresses.find((a) => a.address?.startsWith('9hinv'))!;
		expect(recipient.erg).toBe(-989200n);
		expect(recipient.tokens[0].amount).toBe(2000n);
		expect(expired.erg).toBe(-110800n);
		expect(expired.tokens[0].amount).toBe(-2000n);
		expect(result.addresses.find((a) => a.kind === 'fee')!.erg).toBe(1100000n);
	});
	it('does not label an ordinary signed spend of an old box as rent', () => {
		const signed = structuredClone(evidence);
		signed.inputs[0].proof = 'nonempty';
		expect(txKind(tx, signed).kind).toBe('token');
	});
	it('rejects stale anchors, missing or out-of-range selectors and arbitrary scripts', () => {
		expect(txKind(tx, { ...evidence, block_id: 'wrong-fork' }).kind).toBe('unknown');
		for (const selector of [null, '0304', '0400', '038000', '0301']) {
			const changed = structuredClone(evidence);
			changed.inputs[0].extension_127 = selector;
			expect(txKind(tx, changed).kind).toBe('unknown');
		}
		const changed = structuredClone(tx);
		changed.inputs[0].box!.ergo_tree = '0008d3';
		expect(txKind(changed, evidence).kind).toBe('unknown');
	});
	it('does not erase a proven claim when another input is unresolved', () => {
		const partial = structuredClone(tx);
		partial.inputs[1].box = null;
		expect(txKind(partial, evidence).kind).toBe('rent');
		expect(transactionEffects(partial).complete).toBe(false);
	});
});

describe('exact transaction effects', () => {
	it('groups repeated inputs and change outputs, preserving values above 2^53', () => {
		const changed = structuredClone(tx);
		changed.inputs[0].box = { ...changed.inputs[1].box!, value: '9007199254740993', tokens: [] };
		changed.inputs[1].box!.value = '9';
		changed.outputs[0].value = '9007199253641002';
		changed.outputs[0].tokens = [];
		const result = transactionEffects(changed);
		expect(result.ergTotal).toBe(0n);
		expect(result.addresses[0].erg).toBe(-1100000n);
		expect(result.addresses[0].inputs).toBe(2);
	});
	it('reports mint and burn separately instead of asserting token conservation', () => {
		const changed = structuredClone(tx);
		changed.outputs[0].tokens[0].amount = '500';
		changed.outputs[0].tokens.push({ id: 'new-token', amount: '7', name: null, decimals: null });
		expect(transactionEffects(changed).supplyChanges).toEqual([
			[tx.outputs[0].tokens[0].id, -1500n],
			['new-token', 7n]
		]);
	});
	it('does not combine distinct tokens with the same name', () => {
		const changed = structuredClone(tx);
		changed.outputs[0].tokens.push({ ...changed.outputs[0].tokens[0], id: 'different-id' });
		expect(
			transactionEffects(changed).addresses.find((a) => a.address?.startsWith('9give'))!.tokens
		).toHaveLength(2);
	});
});

it('decodes only canonical nonnegative short selectors', () => {
	expect(rentPointer('0300')).toBe(0);
	expect(rentPointer('0302')).toBe(1);
	expect(rentPointer('03feff03')).toBe(32767);
	for (const raw of ['03', '0301', '0380', '038000', '030000', '0400', '03ffff03', '03feff07'])
		expect(rentPointer(raw)).toBeNull();
});
