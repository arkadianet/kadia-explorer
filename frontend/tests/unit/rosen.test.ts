import { describe, expect, it, vi } from 'vitest';
import { readFileSync } from 'node:fs';
import type { BoxDto, PageDto, TxDto } from '$lib/api/types';
import {
	createRosenWorkflow,
	eventRegister,
	eventSpend,
	rosenDeposit,
	rosenEvent,
	rosenFields,
	transactionDeposits,
	type RosenState
} from '$lib/apps/rosen';
import { ROSEN_FRAUD } from '$lib/apps/rosen-contracts';
const fixture = JSON.parse(
	readFileSync(
		new URL('../../../tests/fixtures/apps/rosen-erg-cardano.json', import.meta.url),
		'utf8'
	)
) as { deposit: BoxDto; sourceTx: TxDto; event: BoxDto; eventSpend: TxDto };
const deposit = rosenDeposit(fixture.deposit)!;
const register = fixture.event.registers!.R5 as string;
const clone = <T>(value: T): T => structuredClone(value);
const anchor = { height: 1_880_000, block_id: 'a'.repeat(64) };
const eventPage = (items = [fixture.event]): PageDto<BoxDto> => ({
	items,
	next_cursor: null,
	next_snapshot: null,
	consistency: 'strict',
	anchor,
	observed_anchor: anchor
});

describe('Rosen pinned native ERG evidence', () => {
	it('decodes captured mainnet deposit metadata and reconstructs exact event bytes', () => {
		expect(deposit).not.toBeNull();
		expect(deposit.amount).toBe(418_527_417_582n);
		expect(deposit.bridgeFee).toBe(34_966_502_091n);
		expect(deposit.networkFee).toBe(3_132_942_641n);
		expect(deposit.toChain).toBe('cardano');
		expect(eventRegister(deposit, fixture.sourceTx)).toBe(register);
		expect(rosenEvent(fixture.event, register)?.watchers).toBe(57);
	});
	it('recognizes a recorded payout reference without asserting destination confirmation', () => {
		const event = rosenEvent(fixture.event, register)!;
		expect(eventSpend(event, fixture.eventSpend, deposit.toChain)).toMatchObject({
			kind: 'reward',
			paymentId: '852a604ac7e63c595277a6e6074354ba0df00bfaf0ed4b233b727e9b76b2ac19'
		});
	});
	it.each([
		(b: BoxDto) => {
			b.ergo_tree = b.ergo_tree!.replace('55bf', '55be');
		},
		(b: BoxDto) => {
			b.tokens = [{ id: 'a'.repeat(64), amount: '1', name: 'ERG', decimals: 9 }];
		},
		(b: BoxDto) => {
			b.value = '1';
		},
		(b: BoxDto) => {
			b.value = '9223372036854775808';
		},
		(b: BoxDto) => {
			b.registers!.R4 = '1a00';
		},
		(b: BoxDto) => {
			b.registers!.R4 = (b.registers!.R4 as string) + '00';
		},
		(b: BoxDto) => {
			b.registers!.R4 = (b.registers!.R4 as string).replace('63617264616e6f', '746573746e6574');
		},
		(b: BoxDto) => {
			b.registers!.R4 = (b.registers!.R4 as string).replace('1a05', '1a8500');
		},
		(b: BoxDto) => {
			b.id = '../anything';
		}
	])('withholds unsupported or malformed deposit shapes', (mutate) => {
		const box = clone(fixture.deposit);
		mutate(box);
		expect(rosenDeposit(box)).toBeNull();
	});
	it('keeps exact amounts beyond JavaScript safe integers', () => {
		const box = { ...fixture.deposit, value: '9007199254740993' };
		expect(rosenDeposit(box)?.amount.toString()).toBe('9007199254740993');
	});
	it('bounds register parsing before allocation and consumes the whole value', () => {
		for (const raw of ['1affffffff7f', '1a0500', '1a05' + '00'.repeat(3000), '1a058004'])
			expect(() => rosenFields(raw, 5)).toThrow();
	});
	it.each([
		(b: BoxDto) => {
			b.ergo_tree = '00';
		},
		(b: BoxDto) => {
			b.tokens[0].id = 'b'.repeat(64);
		},
		(b: BoxDto) => {
			b.tokens[0].amount = 'not-an-integer';
		},
		(b: BoxDto) => {
			b.registers!.R5 = register + '00';
		},
		(b: BoxDto) => {
			b.registers!.R4 = '0e00';
		},
		(b: BoxDto) => {
			b.registers!.R6 = '0e20' + 'a'.repeat(64);
		},
		(b: BoxDto) => {
			b.registers!.R7 = '0400';
		}
	])('rejects name-only or inconsistent trigger records', (mutate) => {
		const box = clone(fixture.event);
		mutate(box);
		expect(rosenEvent(box, register)).toBeNull();
	});
	it('requires the exact source output and an inclusion anchor', () => {
		for (const mutate of [
			(t: TxDto) => {
				delete t.block_id;
			},
			(t: TxDto) => {
				t.outputs[0].value = '1';
			},
			(t: TxDto) => {
				t.outputs[0].registers = {};
			},
			(t: TxDto) => {
				t.outputs.push({ ...fixture.deposit, id: 'b'.repeat(64), index: t.outputs.length });
			}
		]) {
			const tx = clone(fixture.sourceTx);
			mutate(tx);
			expect(() => eventRegister(deposit, tx)).toThrow();
		}
	});
	it('discovers only creating outputs and bounds its transaction scan', () => {
		expect(transactionDeposits(fixture.sourceTx)).toHaveLength(1);
		expect(
			transactionDeposits({
				...fixture.sourceTx,
				outputs: [],
				inputs: [{ id: fixture.deposit.id, box: fixture.deposit }]
			})
		).toEqual([]);
		expect(
			transactionDeposits({
				...fixture.sourceTx,
				inputs: Array(101).fill({ id: 'a'.repeat(64), box: null })
			})
		).toEqual([]);
	});
	it('labels cleanup shape separately and leaves unknown spends unknown', () => {
		const tx = clone(fixture.eventSpend),
			event = rosenEvent(fixture.event, register)!;
		tx.outputs[0].ergo_tree = ROSEN_FRAUD;
		expect(eventSpend(event, tx, 'cardano').kind).toBe('cleanup');
		tx.outputs[0].ergo_tree = '00';
		expect(eventSpend(event, tx, 'cardano').kind).toBe('unrecognized');
	});
	it('rejects stale event spending references and malformed input membership', () => {
		const event = rosenEvent(fixture.event, register)!;
		for (const mutate of [
			(t: TxDto) => {
				t.height++;
			},
			(t: TxDto) => {
				t.inputs[0].id = 'a'.repeat(64);
			},
			(t: TxDto) => {
				t.outputs[1].id = t.outputs[0].id;
			}
		]) {
			const tx = clone(fixture.eventSpend);
			mutate(tx);
			expect(() => eventSpend(event, tx, 'cardano')).toThrow();
		}
	});
});

describe('explicit Rosen observations', () => {
	function setup(page = eventPage()) {
		const states: RosenState[] = [];
		const getTx = vi.fn(async (id: string) =>
			id === fixture.sourceTx.id ? fixture.sourceTx : fixture.eventSpend
		);
		const findEvents = vi.fn(async (register: string) => {
			void register;
			return page;
		});
		const controller = createRosenWorkflow({
			deposit,
			getTx,
			findEvents,
			onChange: (s) => states.push(s)
		});
		return { states, getTx, findEvents, controller };
	}
	it('performs no automatic fetch; explicit inspection makes at most four requests', async () => {
		const run = setup();
		expect(run.getTx).not.toHaveBeenCalled();
		await run.controller.inspect();
		expect(run.getTx.mock.calls.map(([id]) => id)).toEqual([
			fixture.sourceTx.id,
			fixture.eventSpend.id,
			fixture.sourceTx.id
		]);
		expect(run.findEvents.mock.calls[0][0]).toBe(register);
		expect(run.states.at(-1)).toMatchObject({
			checked: true,
			loading: false,
			error: null,
			spend: { kind: 'reward' },
			lookupAnchor: anchor
		});
	});
	it('does not turn a missing trigger into a rejected or completed bridge', async () => {
		const run = setup(eventPage([]));
		await run.controller.inspect();
		expect(run.states.at(-1)).toMatchObject({ checked: true, events: [], spend: null });
		expect(run.getTx).toHaveBeenCalledTimes(2);
	});
	it('does not follow one spend when the result is ambiguous or truncated', async () => {
		for (const page of [
			{ ...eventPage(), next_cursor: '1', next_snapshot: 'snapshot' },
			eventPage([fixture.event, { ...fixture.event, id: 'b'.repeat(64) }])
		]) {
			const run = setup(page);
			await run.controller.inspect();
			expect(run.getTx).toHaveBeenCalledTimes(2);
			expect(run.states.at(-1)?.spend).toBeNull();
		}
	});
	it.each([
		eventPage(Array(11).fill(fixture.event)),
		{ ...eventPage(), consistency: 'best_effort' as const },
		{ ...eventPage(), observed_anchor: { ...anchor, height: 1 } },
		{ ...eventPage(), anchor: null }
	])('rejects unbounded or unanchored lookups', async (page) => {
		const run = setup(page);
		await run.controller.inspect();
		expect(run.states.at(-1)?.error).not.toBeNull();
		expect(run.states.at(-1)?.spend).toBeNull();
	});
	it('rechecks source inclusion and drops all interpretation after a reorg', async () => {
		const run = setup();
		run.getTx
			.mockResolvedValueOnce(fixture.sourceTx)
			.mockResolvedValueOnce(fixture.eventSpend)
			.mockResolvedValueOnce({ ...fixture.sourceTx, block_id: 'b'.repeat(64) });
		await run.controller.inspect();
		expect(run.states.at(-1)?.error).toContain('inclusion changed');
		expect(run.states.at(-1)?.events).toEqual([]);
	});
	it('clears old success after failed refresh and does not retry automatically', async () => {
		const run = setup();
		await run.controller.inspect();
		run.findEvents.mockRejectedValueOnce(new Error('503 unavailable'));
		await run.controller.inspect();
		expect(run.states.at(-1)).toMatchObject({
			checked: false,
			events: [],
			spend: null,
			error: '503 unavailable'
		});
		expect(run.findEvents).toHaveBeenCalledTimes(2);
	});
	it('cancels navigation and ignores a late response without following it', async () => {
		let resolve!: (tx: TxDto) => void;
		const states: RosenState[] = [],
			findEvents = vi.fn();
		const getTx = vi.fn((id: string, signal: AbortSignal) => {
			void id;
			void signal;
			return new Promise<TxDto>((r) => {
				resolve = r;
			});
		});
		const controller = createRosenWorkflow({
			deposit,
			getTx,
			findEvents,
			onChange: (s) => states.push(s)
		});
		const pending = controller.inspect();
		await controller.inspect();
		expect(getTx).toHaveBeenCalledTimes(1);
		controller.stop();
		expect(getTx.mock.calls[0][1].aborted).toBe(true);
		resolve(fixture.sourceTx);
		await pending;
		expect(findEvents).not.toHaveBeenCalled();
		expect(states).toHaveLength(1);
	});
});
