export const SAVED_ADDRESSES_KEY = 'xp-saved-addresses';
export const MAX_SAVED_ADDRESSES = 100;
export const MAX_BACKUP_BYTES = 512_000;
export const SAVED_BACKUP_FORMAT = 'kadia.saved-addresses';
export interface SavedAddress {
	address: string;
	label: string;
	added_at: number;
	updated_at: number;
	group?: string;
}
export type ImportPolicy = 'keep-existing' | 'use-imported';
export interface SavedImportPreview {
	base: string | null;
	incoming: SavedAddress[];
	rows: {
		incoming: SavedAddress;
		existing?: SavedAddress;
		action: 'add' | 'conflict' | 'unchanged';
	}[];
	added: number;
	conflicts: number;
	unchanged: number;
}
type StorageLike = Pick<Storage, 'getItem' | 'setItem' | 'removeItem'>;
const MAX_SERIALIZED_CHARS = 512_000;
const safeAddress = (value: unknown): value is string =>
	typeof value === 'string' && /^[1-9A-HJ-NP-Za-km-z]{7,4096}$/.test(value);
const safeLabel = (value: unknown): value is string =>
	typeof value === 'string' &&
	value.length <= 80 &&
	![...value].some((char) => char.charCodeAt(0) < 32 || char.charCodeAt(0) === 127);
const safeGroup = (value: unknown): value is string => safeLabel(value) && value.length <= 40;
const safeTime = (value: unknown): value is number =>
	typeof value === 'number' &&
	Number.isSafeInteger(value) &&
	value >= 0 &&
	value <= 8_640_000_000_000_000;

/** Structural validation only: saving is offered after the API resolves an address.
 * Local labels never establish address ownership or verification. */
export function parseSavedAddresses(raw: string | null): SavedAddress[] {
	if (raw === null) return [];
	if (raw.length > MAX_SERIALIZED_CHARS)
		throw new Error('Saved data exceeds the local size limit.');
	const data: unknown = JSON.parse(raw);
	if (
		!data ||
		typeof data !== 'object' ||
		!('version' in data) ||
		(data.version !== 1 && data.version !== 2) ||
		!('items' in data) ||
		!Array.isArray(data.items) ||
		data.items.length > MAX_SAVED_ADDRESSES
	)
		throw new Error('Saved address data has an unsupported format.');
	// This set only validates one parse; it is never observed by a Svelte view.
	// eslint-disable-next-line svelte/prefer-svelte-reactivity
	const seen = new Set<string>();
	return data.items.map((item: unknown) => {
		if (
			!item ||
			typeof item !== 'object' ||
			!('address' in item) ||
			!safeAddress(item.address) ||
			!('label' in item) ||
			!safeLabel(item.label) ||
			!('added_at' in item) ||
			!safeTime(item.added_at) ||
			!('updated_at' in item) ||
			!safeTime(item.updated_at) ||
			(data.version === 2 && 'group' in item && !safeGroup(item.group)) ||
			seen.has(item.address)
		)
			throw new Error('Saved address data contains an invalid entry.');
		seen.add(item.address);
		const group = data.version === 2 && 'group' in item ? (item.group as string).trim() : '';
		return {
			address: item.address,
			label: item.label,
			added_at: item.added_at,
			updated_at: item.updated_at,
			...(group ? { group } : {})
		};
	});
}

/** Backups contain local labels, not proof that an imported address exists on-chain. */
export function parseSavedBackup(raw: string): SavedAddress[] {
	if (raw.length > MAX_BACKUP_BYTES || new TextEncoder().encode(raw).byteLength > MAX_BACKUP_BYTES)
		throw new Error('Backup exceeds the 512,000-byte limit.');
	let envelope: unknown;
	try {
		envelope = JSON.parse(raw);
	} catch {
		throw new Error('Choose a valid JSON saved-address backup.');
	}
	if (
		!envelope ||
		typeof envelope !== 'object' ||
		('format' in envelope && envelope.format !== SAVED_BACKUP_FORMAT) ||
		('exported_at' in envelope && !safeTime(envelope.exported_at))
	)
		throw new Error('This file is not a supported saved-address backup.');
	return parseSavedAddresses(raw);
}

function sameFields(a: SavedAddress, b: SavedAddress) {
	return a.label === b.label && (a.group ?? '') === (b.group ?? '');
}
function preview(
	current: SavedAddress[],
	incoming: SavedAddress[],
	base: string | null
): SavedImportPreview {
	const rows = incoming.map((item) => {
		const existing = current.find((entry) => entry.address === item.address);
		return {
			incoming: item,
			existing,
			action: !existing
				? ('add' as const)
				: sameFields(existing, item)
					? ('unchanged' as const)
					: ('conflict' as const)
		};
	});
	const added = rows.filter((row) => row.action === 'add').length;
	if (current.length + added > MAX_SAVED_ADDRESSES)
		throw new Error(
			`This merge would save ${current.length + added} addresses. The browser limit is ${MAX_SAVED_ADDRESSES}; no entries have been changed.`
		);
	return {
		base,
		incoming,
		rows,
		added,
		conflicts: rows.filter((row) => row.action === 'conflict').length,
		unchanged: rows.filter((row) => row.action === 'unchanged').length
	};
}

export function createSavedAddresses(storage: () => StorageLike) {
	let items = $state<SavedAddress[]>([]);
	let error = $state<string | null>(null);
	let ready = $state(false);
	let readable = $state(false);
	function current(target: StorageLike) {
		try {
			const raw = target.getItem(SAVED_ADDRESSES_KEY);
			const parsed = parseSavedAddresses(raw);
			readable = true;
			return { raw, parsed };
		} catch {
			readable = false;
			throw new Error(
				'Saved addresses could not be read. Existing browser data has not been changed.'
			);
		}
	}
	function load() {
		try {
			items = current(storage()).parsed;
			error = null;
		} catch {
			readable = false;
			error = 'Saved addresses could not be read. Existing browser data has not been changed.';
		}
		ready = true;
	}
	function write(change: (current: SavedAddress[], raw: string | null) => SavedAddress[]) {
		try {
			const target = storage();
			const before = current(target);
			const next = change(before.parsed, before.raw);
			const raw = JSON.stringify({ version: 2, items: next });
			if (raw.length > MAX_SERIALIZED_CHARS)
				throw new Error('Saved data exceeds the local size limit.');
			target.setItem(SAVED_ADDRESSES_KEY, raw);
			items = next;
			error = null;
			return true;
		} catch (cause) {
			error = cause instanceof Error ? cause.message : 'Browser storage is unavailable.';
			return false;
		}
	}
	return {
		get items() {
			return items;
		},
		get error() {
			return error;
		},
		get ready() {
			return ready;
		},
		get readable() {
			return readable;
		},
		load,
		save(address: string, label: string, group?: string) {
			const cleaned = label.trim();
			if (
				!safeAddress(address) ||
				!safeLabel(cleaned) ||
				(group !== undefined && !safeGroup(group.trim()))
			) {
				error =
					'Use a valid address, a label of at most 80 characters and a group of at most 40 characters, without control characters.';
				return false;
			}
			return write((current) => {
				const existing = current.find((item) => item.address === address);
				if (!existing && current.length >= MAX_SAVED_ADDRESSES)
					throw new Error(
						'You can save up to 100 addresses in this browser. Remove one to add another.'
					);
				const now = Date.now();
				const selectedGroup = group === undefined ? existing?.group : group.trim();
				return [
					{
						address,
						label: cleaned,
						added_at: existing?.added_at ?? now,
						updated_at: now,
						...(selectedGroup ? { group: selectedGroup } : {})
					},
					...current.filter((item) => item.address !== address)
				];
			});
		},
		remove(address: string) {
			return write((current) => current.filter((item) => item.address !== address));
		},
		exportBackup(): string | null {
			try {
				const latest = current(storage()).parsed;
				const backup = JSON.stringify(
					{ format: SAVED_BACKUP_FORMAT, version: 2, exported_at: Date.now(), items: latest },
					null,
					2
				);
				parseSavedBackup(backup);
				items = latest;
				error = null;
				return backup;
			} catch (cause) {
				error = cause instanceof Error ? cause.message : 'Browser storage is unavailable.';
				return null;
			}
		},
		previewImport(raw: string): SavedImportPreview | null {
			try {
				const latest = current(storage());
				const result = preview(latest.parsed, parseSavedBackup(raw), latest.raw);
				items = latest.parsed;
				error = null;
				return result;
			} catch (cause) {
				error = cause instanceof Error ? cause.message : 'Backup could not be read.';
				return null;
			}
		},
		mergeImport(review: SavedImportPreview, policy: ImportPolicy = 'keep-existing') {
			return write((latest, raw) => {
				if (raw !== review.base)
					throw new Error(
						'Saved addresses changed after this preview. Preview the backup again before merging.'
					);
				if (policy !== 'keep-existing' && policy !== 'use-imported')
					throw new Error('Choose a valid conflict policy.');
				const checked = preview(
					latest,
					parseSavedAddresses(JSON.stringify({ version: 2, items: review.incoming })),
					raw
				);
				const merged = latest.map((existing) => {
					const imported = checked.incoming.find((item) => item.address === existing.address);
					if (policy === 'keep-existing' || !imported || sameFields(existing, imported))
						return existing;
					return { ...imported, added_at: existing.added_at, updated_at: Date.now() };
				});
				return [
					...merged,
					...checked.rows.filter((row) => row.action === 'add').map((row) => row.incoming)
				];
			});
		}
	};
}
export const savedAddresses = createSavedAddresses(() => localStorage);
