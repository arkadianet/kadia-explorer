import { describe, it, expect, vi } from 'vitest';
import { readFileSync } from 'node:fs';
import type { BoxDto, TxDto } from '$lib/api/types';
import {
	createSpectrumWorkflow,
	spectrumOrder,
	spectrumOutcome,
	transactionOrders,
	type WorkflowState
} from '$lib/tx/spectrum';

const fixture = JSON.parse(
	readFileSync(
		new URL('../../../tests/fixtures/apps/spectrum-v3-swap.json', import.meta.url),
		'utf8'
	)
) as { order: BoxDto; settlement: TxDto };
const source = fixture.order;
const settlement = fixture.settlement;
const order = spectrumOrder(source)!;
if (order.kind !== 'swap_sell') throw new Error('Expected the captured ERG-to-token order');
const copy = <T>(value: T): T => structuredClone(value);

const addedFixtures = ['swap-buy', 'deposit', 'redeem'].map((name) => ({
	name,
	...(JSON.parse(
		readFileSync(
			new URL('../../../tests/fixtures/apps/spectrum-v3-' + name + '.json', import.meta.url),
			'utf8'
		)
	) as { order: BoxDto; settlement: TxDto })
}));

describe('pinned token-to-ERG and liquidity workflows', () => {
	it.each(addedFixtures)(
		'decodes and matches the recorded $name settlement',
		({ name, order: box, settlement: tx }) => {
			const decoded = spectrumOrder(box);
			expect(decoded?.kind).toBe(name === 'swap-buy' ? 'swap_buy' : name);
			expect(decoded).not.toBeNull();
			const outcome = spectrumOutcome(decoded!, tx);
			expect(outcome.kind, JSON.stringify(outcome)).toBe('matched');
			if (outcome.kind !== 'matched') return;
			if (name === 'swap-buy') {
				expect(outcome.received).toEqual([{ id: 'erg', amount: '10457609' }]);
				expect(outcome.calculated).toEqual([{ id: 'erg', amount: '10147609' }]);
			} else if (name === 'deposit') {
				expect(outcome.received[0].amount).toBe('4206');
				expect(outcome.calculated[0].amount).toBe('4206');
			} else {
				expect(outcome.received.map((asset) => asset.amount)).toEqual(['33654950', '5']);
				expect(outcome.calculated.map((asset) => asset.amount)).toEqual(['33344950', '5']);
			}
		}
	);
});

describe('pinned Spectrum v3 ERG to token interpretation', () => {
	it('decodes a captured mainnet order without losing execution-fee precision', () => {
		expect(order).not.toBeNull();
		expect(order.baseNano).toBe(300_000_000n);
		expect(order.minimumQuote).toBe(5_248_464n);
		expect(order.executionDenominator).toBe(100_000_000_000_000_000n);
		expect(order.executionDelta).toBe(97_405_945_815_766_289n);
		expect(order.maximumExecutionFee).toBe(163_377n);
		expect(order.feeRemainderBase).toBe(1400n);
		expect(order.recipientTree).toBe(settlement.outputs[1].ergo_tree);
	});

	it('recognizes the captured pool transition and keeps output amount distinct from calculated quote', () => {
		const result = spectrumOutcome(order, settlement);
		expect(result.kind).toBe('matched');
		if (result.kind === 'matched') {
			expect(result.recipient.tokens[0].amount).toBe('5429060');
			expect(result.contractQuote).toBe('5405915');
			expect(result.poolBefore.tokens[1].amount).toBe('9223359717007728454');
		}
	});

	it('does not accept altered literals behind an identical template', () => {
		const changed = { ...source, ergo_tree: source.ergo_tree!.replace('04000404', '04020404') };
		// Directly alter constant0 while preserving the remainder of the expression.
		changed.ergo_tree = source.ergo_tree!.replace('210400', '210402');
		expect(spectrumOrder(changed)).toBeNull();
	});

	it.each([
		null,
		'00',
		'ff'.repeat(9000),
		'19ff',
		source.ergo_tree!.slice(0, -2),
		source.ergo_tree! + '00',
		source.ergo_tree!.slice(0, -2) + '21',
		source.ergo_tree!.replace('210400', '200400'),
		source.ergo_tree!.replace('199505', '199405')
	])('rejects unsupported, oversized or malformed serialization', (tree) => {
		expect(spectrumOrder({ ...source, ergo_tree: tree })).toBeNull();
	});

	it('does not recognize a name-only token or different fee funding', () => {
		const wrongToken = copy(source);
		wrongToken.tokens[0].id = 'a'.repeat(64);
		expect(spectrumOrder(wrongToken)).toBeNull();
		const wrongAmount = copy(source);
		wrongAmount.tokens[0].amount = '163378';
		expect(spectrumOrder(wrongAmount)).toBeNull();
		expect(spectrumOrder({ ...source, value: '900719925474099312345' })).toBeNull();
	});

	it('discovers orders in both creation and settlement without duplicates or unbounded scans', () => {
		expect(transactionOrders(settlement)).toHaveLength(1);
		const creation = { ...settlement, inputs: [], outputs: [source] };
		expect(transactionOrders(creation)[0].box.id).toBe(source.id);
		expect(
			transactionOrders({ ...settlement, outputs: [...settlement.outputs, source] })
		).toHaveLength(1);
		expect(
			transactionOrders({ ...settlement, inputs: Array(101).fill(settlement.inputs[1]) })
		).toEqual([]);
	});

	it.each([
		(tx: TxDto) => {
			tx.inputs[0].box = null;
		},
		(tx: TxDto) => {
			tx.inputs[1].box!.value = '300399999';
		},
		(tx: TxDto) => {
			tx.inputs[1].box!.registers = { R4: '0400' };
		},
		(tx: TxDto) => {
			tx.inputs.push(copy(tx.inputs[1]));
		},
		(tx: TxDto) => {
			tx.inputs[0].box!.ergo_tree = '00';
		},
		(tx: TxDto) => {
			tx.outputs[0].tokens[1].amount = '9223359717007728453';
		},
		(tx: TxDto) => {
			tx.outputs[1].ergo_tree = '0008cd02' + 'a'.repeat(64);
		},
		(tx: TxDto) => {
			tx.outputs[1].tokens[0].amount = '1';
		},
		(tx: TxDto) => {
			tx.outputs[0].value = '91751656564131';
		},
		(tx: TxDto) => {
			tx.inputs[0].box!.registers = { R4: '04c60f' };
		},
		(tx: TxDto) => {
			tx.outputs[3].value = '2000001';
		},
		(tx: TxDto) => {
			tx.outputs[1].tx_id = 'a'.repeat(64);
		}
	])('keeps contradictory or incomplete spends unrecognized', (mutate) => {
		const tx = copy(settlement);
		mutate(tx);
		expect(spectrumOutcome(order, tx).kind).toBe('unrecognized');
	});

	it('does not label an ordinary refund-shaped spend as a swap or proven cancellation', () => {
		const tx = {
			...settlement,
			inputs: [settlement.inputs[1]],
			outputs: [{ ...source, tx_id: settlement.id, index: 0, ergo_tree: order.refundTree }]
		};
		expect(spectrumOutcome(order, tx).kind).toBe('unrecognized');
	});
	it('withholds arithmetic that would overflow the contract Long intermediate', () => {
		const result = spectrumOutcome({ ...order, baseNano: 10_000_000_000_000_000n }, settlement);
		expect(result).toMatchObject({
			kind: 'unrecognized',
			reason: expect.stringContaining('arithmetic range')
		});
	});
	it('withholds malformed API references and LP quantities', () => {
		const wrongInput = copy(settlement);
		wrongInput.inputs[0].id = 'a'.repeat(64);
		expect(spectrumOutcome(order, wrongInput).kind).toBe('unrecognized');
		const duplicate = copy(settlement);
		duplicate.outputs[2].id = duplicate.outputs[1].id;
		expect(spectrumOutcome(order, duplicate).kind).toBe('unrecognized');
		const reused = copy(settlement);
		reused.outputs[2].id = reused.inputs[0].id;
		expect(spectrumOutcome(order, reused).kind).toBe('unrecognized');
		const badLP = copy(settlement);
		badLP.inputs[0].box!.tokens[1].amount = badLP.outputs[0].tokens[1].amount = 'not-a-number';
		expect(spectrumOutcome(order, badLP).kind).toBe('unrecognized');
	});
});

describe('token-to-ERG and liquidity evidence boundaries', () => {
	it.each(addedFixtures)(
		'rejects mutated serialized literals and funding for $name',
		({ name, order: box }) => {
			const count = name === 'swap-buy' ? '1a' : name === 'deposit' ? '17' : '12';
			const tree = box.ergo_tree!;
			const changed = tree.replace(count + '0400', count + '0402');
			expect(changed).not.toBe(tree);
			expect(spectrumOrder({ ...box, ergo_tree: changed })).toBeNull();
			expect(spectrumOrder({ ...box, ergo_tree: tree.slice(0, -2) + 'ff' })).toBeNull();
			const invalid = copy(box);
			invalid.tokens[0].amount = '0';
			expect(spectrumOrder(invalid)).toBeNull();
			invalid.tokens[0].amount = '9223372036854775808';
			expect(spectrumOrder(invalid)).toBeNull();
			invalid.tokens[0].amount = 17 as unknown as string;
			expect(spectrumOrder(invalid)).toBeNull();
			invalid.tokens = [...box.tokens, copy(box.tokens[0])];
			expect(spectrumOrder(invalid)).toBeNull();
		}
	);
	it.each(addedFixtures)(
		'keeps incomplete, altered or excessive $name evidence unknown',
		({ order: box, settlement: tx }) => {
			const decoded = spectrumOrder(box)!;
			const mutations: ((tx: TxDto) => void)[] = [
				(tx) => {
					tx.inputs[0].box = null;
				},
				(tx) => {
					tx.inputs[0].id = 'a'.repeat(64);
				},
				(tx) => {
					tx.inputs[1].box!.tokens[0].amount = '1';
				},
				(tx) => {
					tx.outputs[0].tokens[0].amount = '2';
				},
				(tx) => {
					tx.outputs[0].tokens[1].id = 'b'.repeat(64);
				},
				(tx) => {
					tx.outputs[0].value = '10000000';
				},
				(tx) => {
					tx.outputs[0].registers = { R4: '04c70f' };
				},
				(tx) => {
					tx.outputs[1].ergo_tree =
						decoded.refundTree === decoded.recipientTree ? '00' : decoded.refundTree;
				},
				(tx) => {
					tx.outputs[1].index = 0;
				},
				(tx) => {
					tx.outputs[3].value = (decoded.maximumMinerFee + 1n).toString();
				},
				(tx) => {
					tx.outputs[2].id = tx.outputs[1].id;
				},
				(tx) => {
					tx.inputs = Array(101).fill(tx.inputs[0]);
				}
			];
			for (const mutate of mutations) {
				const changed = copy(tx);
				mutate(changed);
				expect(spectrumOutcome(decoded, changed).kind).toBe('unrecognized');
			}
		}
	);
	it('keeps token-to-ERG order funding, pool input and returned order ERG distinct', () => {
		const fixture = addedFixtures[0];
		const decoded = spectrumOrder(fixture.order)!;
		if (decoded.kind !== 'swap_buy') throw new Error('Expected SwapBuy');
		expect(decoded.baseAmount).toBe(22n);
		expect(decoded.maximumExecutionFee).toBe(18n);
		expect(decoded.executionDenominator).toBe(1_000_000_000_000_000_000n);
		expect(decoded.executionNumerator).toBe(1_522_526_077_827n);
		const unfunded = copy(fixture.order);
		unfunded.tokens[0].amount = '39';
		expect(spectrumOrder(unfunded)).toBeNull();
		for (const mutate of [
			(tx: TxDto) => {
				tx.outputs[0].tokens[2].amount = (BigInt(tx.outputs[0].tokens[2].amount) + 1n).toString();
			},
			(tx: TxDto) => {
				tx.outputs[1].value = '1000000';
			},
			(tx: TxDto) => {
				tx.outputs[0].value = (BigInt(tx.outputs[0].value) + 1n).toString();
			}
		]) {
			const tx = copy(fixture.settlement);
			mutate(tx);
			expect(spectrumOutcome(decoded, tx).kind).toBe('unrecognized');
		}
		expect(
			spectrumOutcome({ ...decoded, baseAmount: 10_000_000_000_000_000n }, fixture.settlement)
		).toMatchObject({ kind: 'unrecognized', reason: expect.stringContaining('arithmetic range') });
		// The captured compiled tree divides by constant9, contrary to the parameter
		// names in its source comments. A nonpositive remainder has no refund condition.
		const noFeeRefund = copy(fixture.settlement);
		noFeeRefund.outputs[1].tokens = [];
		expect(spectrumOutcome(decoded, noFeeRefund).kind).toBe('matched');
	});
	it.each(addedFixtures.slice(1))(
		'requires the exact LP movement and recipient return for $name',
		({ order: box, settlement: tx }) => {
			const decoded = spectrumOrder(box)!;
			for (const mutate of [
				(tx: TxDto) => {
					tx.outputs[0].tokens[1].amount = tx.inputs[0].box!.tokens[1].amount;
				},
				(tx: TxDto) => {
					tx.outputs[1].tokens[0].amount = '1';
				},
				(tx: TxDto) => {
					tx.outputs[1].value = '1';
				},
				(tx: TxDto) => {
					tx.inputs[0].box!.tokens[1].amount = '9223372036854775807';
				},
				(tx: TxDto) => {
					tx.outputs[0].tokens[2].amount = tx.inputs[0].box!.tokens[2].amount;
				}
			]) {
				const changed = copy(tx);
				mutate(changed);
				expect(spectrumOutcome(decoded, changed).kind).toBe('unrecognized');
			}
		}
	);
});

describe('explicit workflow observations', () => {
	function setup(box: BoxDto = source, tx: TxDto = settlement) {
		const states: WorkflowState[] = [];
		const getBox = vi.fn(async () => box),
			getTx = vi.fn(async () => tx);
		const controller = createSpectrumWorkflow({
			order,
			getBox,
			getTx,
			onChange: (s) => states.push(s)
		});
		return { states, getBox, getTx, controller };
	}
	it('does not fetch on construction and loads at most box plus its recorded spend on request', async () => {
		const run = setup();
		expect(run.getBox).not.toHaveBeenCalled();
		await run.controller.inspect();
		expect(run.getBox).toHaveBeenCalledTimes(1);
		expect(run.getTx).toHaveBeenCalledWith(settlement.id);
		expect(run.states.at(-1)?.outcome?.kind).toBe('matched');
	});
	it('leaves unspent observations distinct from pending or rejected transactions', async () => {
		const run = setup({ ...source, spent_by: null, spent_height: null });
		await run.controller.inspect();
		expect(run.getTx).not.toHaveBeenCalled();
		expect(run.states.at(-1)).toMatchObject({
			checked: true,
			tx: null,
			outcome: null,
			error: null
		});
	});
	it('clears an old success when a refresh fails and never automatically retries', async () => {
		const run = setup();
		await run.controller.inspect();
		run.getBox.mockRejectedValueOnce(new Error('422 details budget'));
		await run.controller.inspect();
		expect(run.states.at(-1)).toMatchObject({
			outcome: null,
			tx: null,
			error: '422 details budget'
		});
		expect(run.getBox).toHaveBeenCalledTimes(2);
	});
	it('rejects a changed spend inclusion instead of retaining the previous interpretation', async () => {
		const run = setup(source, { ...settlement, height: settlement.height + 1 });
		await run.controller.inspect();
		expect(run.states.at(-1)?.error).toContain('reference changed');
	});
	it('rejects changed immutable box facts before fetching any spending transaction', async () => {
		const run = setup({ ...source, creation_height: 1 });
		await run.controller.inspect();
		expect(run.getTx).not.toHaveBeenCalled();
		expect(run.states.at(-1)?.error).toContain('no longer matches');
	});
	it('drops late observations and prevents duplicate concurrent requests', async () => {
		let resolve!: (box: BoxDto) => void;
		const states: WorkflowState[] = [],
			getTx = vi.fn(async () => settlement);
		const getBox = vi.fn(
			() =>
				new Promise<BoxDto>((r) => {
					resolve = r;
				})
		);
		const controller = createSpectrumWorkflow({
			order,
			getBox,
			getTx,
			onChange: (s) => states.push(s)
		});
		const pending = controller.inspect();
		await controller.inspect();
		expect(getBox).toHaveBeenCalledTimes(1);
		controller.stop();
		resolve(source);
		await pending;
		expect(getTx).not.toHaveBeenCalled();
		expect(states).toHaveLength(1);
	});
});
