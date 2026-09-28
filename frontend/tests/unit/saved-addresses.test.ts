import { describe, expect, it } from 'vitest';
import {
	createSavedAddresses,
	MAX_SAVED_ADDRESSES,
	MAX_BACKUP_BYTES,
	parseSavedBackup,
	SAVED_BACKUP_FORMAT,
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
		JSON.stringify({ version: 99, items: [] }),
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

describe('saved groups and backups', () => {
	it('loads v1 without rewriting it and migrates known fields losslessly on the next save', () => {
		const initial = encoded();
		const storage = memory(initial);
		const model = createSavedAddresses(() => storage);
		model.load();
		expect(model.readable).toBe(true);
		expect(storage.getItem()).toBe(initial);
		const exported = JSON.parse(model.exportBackup()!);
		expect(exported).toMatchObject({ format: SAVED_BACKUP_FORMAT, version: 2, items: [entry] });
		expect(storage.getItem()).toBe(initial);
		expect(model.save(other, 'Second', 'Research')).toBe(true);
		const migrated = JSON.parse(storage.getItem()!);
		expect(migrated.version).toBe(2);
		expect(migrated.items.find((item: { address: string }) => item.address === address)).toEqual(
			entry
		);
	});
	it('assigns and clears a group while label-only callers retain an existing group', () => {
		const storage = memory(encoded());
		const model = createSavedAddresses(() => storage);
		expect(model.save(address, 'Label', ' Research ')).toBe(true);
		expect(model.items[0]).toMatchObject({ group: 'Research', added_at: 1 });
		expect(model.save(address, 'Changed label')).toBe(true);
		expect(model.items[0].group).toBe('Research');
		expect(model.save(address, 'Changed label', '')).toBe(true);
		expect(model.items[0].group).toBeUndefined();
		expect(model.save(address, 'Label', 'x'.repeat(41))).toBe(false);
		expect(model.save(address, 'Label', 'bad\nname')).toBe(false);
	});
	it('previews every conflict without writing and merges new addresses while preserving local fields by default', () => {
		const original = JSON.stringify({ version: 2, items: [{ ...entry, group: 'Local group' }] });
		const storage = memory(original);
		const model = createSavedAddresses(() => storage);
		const review = model.previewImport(
			JSON.stringify({
				version: 2,
				items: [
					{ ...entry, label: 'Imported label', group: 'Backup group' },
					{ ...entry, address: other, label: 'New address', group: 'Backup group' }
				]
			})
		)!;
		expect(review).toMatchObject({ added: 1, conflicts: 1, unchanged: 0 });
		expect(storage.getItem()).toBe(original);
		expect(model.mergeImport(review)).toBe(true);
		expect(model.items).toEqual([
			{ ...entry, group: 'Local group' },
			{ ...entry, address: other, label: 'New address', group: 'Backup group' }
		]);
	});
	it('only explicit replacement applies blank imported fields and keeps the local creation time', () => {
		const storage = memory(JSON.stringify({ version: 2, items: [{ ...entry, group: 'Work' }] }));
		const model = createSavedAddresses(() => storage);
		const review = model.previewImport(
			JSON.stringify({
				version: 2,
				items: [{ ...entry, label: '', added_at: 900, updated_at: 901 }]
			})
		)!;
		expect(model.mergeImport(review, 'use-imported')).toBe(true);
		expect(model.items[0]).toMatchObject({ address, label: '', added_at: 1 });
		expect(model.items[0].group).toBeUndefined();
		expect(model.items[0].updated_at).toBeGreaterThan(2);
	});
	it('rejects a stale preview rather than overwriting a concurrent browser edit', () => {
		const storage = memory(encoded());
		const model = createSavedAddresses(() => storage);
		const backup = encoded([{ ...entry, address: other }]);
		const review = model.previewImport(backup)!;
		const changed = encoded([{ ...entry, label: 'Changed in another tab' }]);
		storage.setItem(SAVED_ADDRESSES_KEY, changed);
		expect(model.mergeImport(review)).toBe(false);
		expect(model.error).toContain('changed after this preview');
		expect(storage.getItem()).toBe(changed);
		expect(model.mergeImport(model.previewImport(backup)!)).toBe(true);
		expect(model.items[0].label).toBe('Changed in another tab');
		expect(model.items).toHaveLength(2);
	});
	it('does not clobber unreadable storage during import, export or merge', () => {
		const storage = memory(encoded());
		const model = createSavedAddresses(() => storage);
		const review = model.previewImport(encoded([{ ...entry, address: other }]))!;
		storage.setItem(SAVED_ADDRESSES_KEY, '{broken');
		expect(model.exportBackup()).toBeNull();
		expect(model.previewImport(encoded())).toBeNull();
		expect(model.mergeImport(review)).toBe(false);
		expect(model.readable).toBe(false);
		expect(storage.getItem()).toBe('{broken');
		expect(model.items).toEqual([entry]);
	});
	it('keeps stored and in-memory entries intact if a merge exceeds browser quota', () => {
		const storage = memory(encoded());
		const model = createSavedAddresses(() => storage);
		const review = model.previewImport(encoded([{ ...entry, address: other }]))!;
		storage.setItem = () => {
			throw new Error('Quota exceeded');
		};
		expect(model.mergeImport(review)).toBe(false);
		expect(model.items).toEqual([entry]);
		expect(storage.getItem()).toBe(encoded());
	});
	it('round-trips Unicode labels/groups and ignores unrecognized entry metadata', () => {
		const input = JSON.stringify({
			version: 2,
			items: [
				{ ...entry, label: '研究 <local>', group: '收藏', owner: true, __proto__: { unsafe: true } }
			]
		});
		const storage = memory(input);
		const model = createSavedAddresses(() => storage);
		const exported = model.exportBackup()!;
		expect(parseSavedBackup(exported)).toEqual([
			{ ...entry, label: '研究 <local>', group: '收藏' }
		]);
		expect(exported).not.toContain('owner');
		expect(exported).not.toContain('unsafe');
	});
	it('counts only new addresses against the merge limit and never partially imports an oversized merge', () => {
		const entries = Array.from({ length: MAX_SAVED_ADDRESSES }, (_, i) => ({
			...entry,
			address: '9'.repeat(i + 7)
		}));
		const storage = memory(encoded(entries));
		const model = createSavedAddresses(() => storage);
		expect(model.previewImport(encoded([entries[0]]))?.unchanged).toBe(1);
		expect(model.previewImport(encoded([{ ...entry, address: other }]))).toBeNull();
		expect(model.error).toContain('101');
		expect(storage.getItem()).toBe(encoded(entries));
	});
	it.each([
		'{',
		JSON.stringify({ version: 99, items: [] }),
		JSON.stringify({ format: 'other-product', version: 2, items: [] }),
		JSON.stringify({ version: 2, exported_at: -1, items: [] }),
		JSON.stringify({ version: 2, items: [{ ...entry, group: {} }] }),
		JSON.stringify({ version: 2, items: [{ ...entry, group: 'x'.repeat(41) }] }),
		JSON.stringify({ version: 2, items: [{ ...entry, group: 'bad\nname' }] }),
		encoded([entry, entry]),
		encoded(Array.from({ length: 101 }, (_, i) => ({ ...entry, address: '9'.repeat(i + 7) }))),
		'x'.repeat(MAX_BACKUP_BYTES + 1),
		'研'.repeat(200_000)
	])('rejects an invalid or unbounded backup before any merge', (raw) => {
		expect(() => parseSavedBackup(raw)).toThrow();
		const storage = memory(encoded());
		const model = createSavedAddresses(() => storage);
		expect(model.previewImport(raw)).toBeNull();
		expect(storage.getItem()).toBe(encoded());
	});
});
