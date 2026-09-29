import { afterEach, describe, expect, it, vi } from 'vitest';
import { ApiError } from '../../src/lib/api/client';
import {
	collectibleNano,
	createExposureLoader,
	emptyExposure,
	fetchExposure,
	rentCategory,
	summarizeExposure,
	validateExposure,
	type RentExposure,
	type RentExposureBox
} from '../../src/lib/rent/exposure';

const address = 'recorded-address';
function box(n = 1, maturity = 200, fee = '125000000'): RentExposureBox {
	return {
		id: n.toString(16).padStart(64, '0'),
		value: '1000000000',
		creation_height: 0,
		size: 100,
		token_count: 0,
		rent: {
			maturity_height: maturity,
			due_nano: '125000000',
			consensus_fee_nano: fee,
			collectible: BigInt(fee) > 0n,
			claimable_at_tip: maturity <= 200
		}
	};
}
function snapshot(): RentExposure {
	return {
		items: [box()],
		truncated: false,
		context: {
			scope: 'indexed_unspent_boxes',
			address,
			tree_hash: 'a'.repeat(64),
			indexed_height: 200,
			anchor: { height: 200, block_id: 'b'.repeat(64) },
			full_history: true,
			partial_from: null,
			scanned_count: 1,
			scan_limit: 5000,
			scan_complete: true
		}
	};
}
function deferred<T>() {
	let resolve!: (value: T) => void;
	const promise = new Promise<T>((done) => (resolve = done));
	return { promise, resolve };
}
afterEach(() => {
	vi.unstubAllGlobals();
	vi.useRealTimers();
});

describe('anchored rent exposure', () => {
	it('uses positive consensus fees capped by box value, never nominal due', () => {
		const row = box();
		row.value = '17';
		row.rent.due_nano = '17';
		expect(collectibleNano(row)).toBe(17n);
		row.value = '1000000000';
		row.rent.due_nano = '999999999';
		expect(collectibleNano(row)).toBe(125000000n);
		row.rent.consensus_fee_nano = '-1794967296';
		row.rent.collectible = false;
		expect(collectibleNano(row)).toBe(0n);
		expect(rentCategory(row, 200, 720)).toBe('non_collectible');
	});
	it('classifies exact maturity and horizon boundaries using only the read height', () => {
		expect(rentCategory(box(), 200, 720)).toBe('mature');
		expect(rentCategory(box(2, 920), 200, 720)).toBe('approaching');
		expect(rentCategory(box(3, 921), 200, 720)).toBe('later');
		expect(rentCategory(box(), null, 720)).toBe('unknown');
	});
	it('keeps large value sums exact and scope separate from maturity availability', () => {
		const data = snapshot();
		data.items = [box(1), box(2, 201)];
		data.context.scanned_count = 2;
		data.items.forEach((row) => (row.value = '18446744073709551615'));
		expect(summarizeExposure(data, 720)).toMatchObject({
			value: '36893488147419103230',
			mature: '125000000',
			approaching: '125000000',
			complete: true
		});
		data.context.full_history = false;
		data.context.partial_from = 100;
		expect(summarizeExposure(data, 720).complete).toBe(false);
		data.context.full_history = true;
		data.context.partial_from = null;
		data.truncated = true;
		data.context.scan_complete = false;
		expect(summarizeExposure(data, 720).complete).toBe(false);
		data.context.indexed_height = null;
		data.context.anchor = null;
		expect(summarizeExposure(data, 720)).toMatchObject({
			mature: null,
			approaching: null,
			counts: { unknown: 2 }
		});
	});
	it('accepts genesis-only context without inventing a tip or anchor', () => {
		const data = snapshot();
		data.context.indexed_height = null;
		data.context.anchor = null;
		data.items[0].rent.claimable_at_tip = false;
		expect(validateExposure(data, address)).toBe(data);
	});
	it.each([
		(data: RentExposure) => {
			data.context.address = 'other';
		},
		(data: RentExposure) => {
			data.context.anchor!.height++;
		},
		(data: RentExposure) => {
			data.context.scan_complete = false;
		},
		(data: RentExposure) => {
			data.context.scanned_count++;
		},
		(data: RentExposure) => {
			data.items.push(data.items[0]);
			data.context.scanned_count++;
		},
		(data: RentExposure) => {
			data.items[0].value = '18446744073709551616';
		},
		(data: RentExposure) => {
			data.items[0].rent.collectible = false;
		},
		(data: RentExposure) => {
			data.items[0].rent.claimable_at_tip = false;
		}
	])('rejects inconsistent identity, scope or fee evidence (%#)', (change) => {
		const data = snapshot();
		change(data);
		expect(() => validateExposure(data, address)).toThrow('consistent anchored');
	});
	it('does not treat a legacy unanchored response as current evidence', () => {
		expect(() => validateExposure({ items: [], truncated: false }, address)).toThrow(
			'server is updated'
		);
	});
});

describe('explicit rent snapshot loader', () => {
	it('does not auto-fetch, deduplicates clicks, and ignores old-address results', async () => {
		const pending = deferred<RentExposure>();
		const fetcher = vi.fn(() => pending.promise);
		let current = emptyExposure();
		const loader = createExposureLoader((next) => (current = next), fetcher);
		loader.setAddress(address);
		expect(fetcher).not.toHaveBeenCalled();
		const load = loader.load();
		await loader.load();
		expect(fetcher).toHaveBeenCalledTimes(1);
		loader.setAddress('new-address');
		expect(fetcher.mock.calls[0]?.length).toBe(2);
		pending.resolve(snapshot());
		await load;
		expect(current).toEqual(emptyExposure());
		loader.stop();
	});
	it('retains a failed refresh as explicitly stale, and clears it on address change', async () => {
		const fetcher = vi
			.fn()
			.mockResolvedValueOnce(snapshot())
			.mockRejectedValueOnce(new ApiError(503, 'Unavailable', 'Node down'));
		let current = emptyExposure();
		const loader = createExposureLoader((next) => (current = next), fetcher);
		loader.setAddress(address);
		await loader.load();
		expect(current.result?.items).toHaveLength(1);
		await loader.load();
		expect(current.stale).toBe(true);
		expect(current.error).toContain('503');
		expect(current.result?.items).toHaveLength(1);
		loader.setAddress('elsewhere');
		expect(current).toEqual(emptyExposure());
		loader.stop();
	});
	it('withholds a missing/unseen address result instead of displaying zero', async () => {
		let current = emptyExposure();
		const loader = createExposureLoader(
			(next) => (current = next),
			vi.fn().mockRejectedValue(new ApiError(404, 'Not found', 'Unseen'))
		);
		loader.setAddress(address);
		await loader.load();
		expect(current.result).toBeNull();
		expect(current.error).toContain('not zero exposure');
		loader.stop();
	});
	it('abort on stop prevents late success and timeout prevents acceptance', async () => {
		vi.useFakeTimers();
		const pending = deferred<RentExposure>();
		let current = emptyExposure();
		const loader = createExposureLoader(
			(next) => (current = next),
			() => pending.promise
		);
		loader.setAddress(address);
		const run = loader.load();
		await vi.advanceTimersByTimeAsync(10_000);
		pending.resolve(snapshot());
		await run;
		expect(current.error).toContain('timed out');
		expect(current.result).toBeNull();
		const late = deferred<RentExposure>();
		const observer = vi.fn();
		const stopped = createExposureLoader(observer, () => late.promise);
		stopped.setAddress(address);
		const task = stopped.load();
		stopped.stop();
		const count = observer.mock.calls.length;
		late.resolve(snapshot());
		await task;
		expect(observer).toHaveBeenCalledTimes(count);
		loader.stop();
	});
});

describe('bounded exposure transport', () => {
	it('requests only the compact view and rejects oversized or invalid UTF-8 responses', async () => {
		const fetcher = vi
			.fn()
			.mockResolvedValueOnce(new Response(JSON.stringify(snapshot())))
			.mockResolvedValueOnce(new Response('x'.repeat(2 * 1024 * 1024 + 1)))
			.mockResolvedValueOnce(new Response(new Uint8Array([0xff])));
		vi.stubGlobal('fetch', fetcher);
		const signal = new AbortController().signal;
		await fetchExposure(address, signal);
		expect(fetcher.mock.calls[0][0]).toBe(`/v1/addresses/${address}/rent?view=exposure`);
		await expect(fetchExposure(address, signal)).rejects.toThrow('response limit');
		await expect(fetchExposure(address, signal)).rejects.toThrow('HTTP 200');
	});
});
