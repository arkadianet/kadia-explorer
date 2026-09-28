import { describe, expect, it } from 'vitest';
import {
	createSavedAddresses,
	MAX_SAVED_ADDRESSES,
	parseSavedAddresses,
	SAVED_ADDRESSES_KEY
} from '../../src/lib/addresses/saved.svelte';

const address = '9'.repeat(51);
const other = '8'.repeat(51);
const entry = { address, label: 'Local label', added_at: 1, updated_at: 2 };
const encoded = (items = [entry]) => JSON.stringify({ version: 1, items });
function memory(initial: string | null = null) {
	let raw = initial;
	return {
		getItem: () => raw,
		setItem: (_key: string, value: string) => {
			raw = value;
		},
		removeItem: () => {
			raw = null;
		}
	};
}

describe('saved addresses', () => {
	it('whitelists valid versioned data without interpreting labels as ownership', () => {
		expect(parseSavedAddresses(null)).toEqual([]);
		expect(parseSavedAddresses(encoded())).toEqual([entry]);
		expect(
			parseSavedAddresses(JSON.stringify({ version: 1, items: [{ ...entry, owner: true }] }))
		).toEqual([entry]);
	});
	it.each([
		'{',
		JSON.stringify({ version: 2, items: [] }),
		encoded([entry, entry]),
		encoded([{ ...entry, address: '../not-an-address' }]),
		encoded([{ ...entry, label: 'x\n=1' }]),
		encoded([{ ...entry, label: 'x'.repeat(81) }]),
		encoded([{ ...entry, added_at: -1 }]),
		encoded([{ ...entry, updated_at: Infinity }]),
		'x'.repeat(512_001)
	])('rejects malformed or unsupported local data', (raw) =>
		expect(() => parseSavedAddresses(raw)).toThrow()
	);
	it('saves, edits and removes while retaining the original creation date', () => {
		const storage = memory(encoded());
		const model = createSavedAddresses(() => storage);
		model.load();
		expect(model.save(address, ' Renamed ')).toBe(true);
		expect(model.items[0]).toMatchObject({ address, label: 'Renamed', added_at: 1 });
		expect(model.save(other, '<script>local text</script>')).toBe(true);
		expect(model.items).toHaveLength(2);
		expect(model.remove(address)).toBe(true);
		expect(parseSavedAddresses(storage.getItem())).toEqual(model.items);
		expect(model.items[0].address).toBe(other);
	});
	it('reads the latest storage before writes so another tab is retained', () => {
		const storage = memory();
		const model = createSavedAddresses(() => storage);
		model.load();
		storage.setItem(SAVED_ADDRESSES_KEY, encoded());
		expect(model.save(other, 'Second')).toBe(true);
		expect(model.items.map((item) => item.address)).toEqual([other, address]);
	});
	it('preserves in-memory entries and stored bytes on quota failure or corruption', () => {
		const storage = memory(encoded());
		const model = createSavedAddresses(() => storage);
		model.load();
		storage.setItem = () => {
			throw new Error('Quota exceeded');
		};
		expect(model.save(other, 'Other')).toBe(false);
		expect(model.items).toEqual([entry]);
		expect(storage.getItem()).toBe(encoded());
		const corrupt = memory('{broken');
		const blocked = createSavedAddresses(() => corrupt);
		blocked.load();
		expect(blocked.error).toContain('could not be read');
		expect(blocked.save(address, 'Label')).toBe(false);
		expect(corrupt.getItem()).toBe('{broken');
	});
	it('bounds address count while allowing an existing label to be edited', () => {
		const entries = Array.from({ length: MAX_SAVED_ADDRESSES }, (_, i) => ({
			...entry,
			address: '9'.repeat(i + 7)
		}));
		const model = createSavedAddresses(() => memory(encoded(entries)));
		expect(model.save(other, 'Beyond limit')).toBe(false);
		expect(model.error).toContain('100');
		expect(model.save(entries[0].address, 'Edited')).toBe(true);
		expect(model.items).toHaveLength(100);
	});
	it('reports unavailable browser storage without throwing from a page', () => {
		const model = createSavedAddresses(() => {
			throw new Error('Blocked');
		});
		model.load();
		expect(model.ready).toBe(true);
		expect(model.error).toBeTruthy();
		expect(model.save(address, 'Label')).toBe(false);
	});
});
