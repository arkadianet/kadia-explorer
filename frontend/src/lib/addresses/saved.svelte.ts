export const SAVED_ADDRESSES_KEY = 'xp-saved-addresses';
export const MAX_SAVED_ADDRESSES = 100;
export interface SavedAddress {
	address: string;
	label: string;
	added_at: number;
	updated_at: number;
}
type StorageLike = Pick<Storage, 'getItem' | 'setItem' | 'removeItem'>;
const MAX_SERIALIZED_CHARS = 512_000;
const safeAddress = (value: unknown): value is string =>
	typeof value === 'string' && /^[1-9A-HJ-NP-Za-km-z]{7,4096}$/.test(value);
const safeLabel = (value: unknown): value is string =>
	typeof value === 'string' &&
	value.length <= 80 &&
	![...value].some((char) => char.charCodeAt(0) < 32 || char.charCodeAt(0) === 127);
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
		data.version !== 1 ||
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
			seen.has(item.address)
		)
			throw new Error('Saved address data contains an invalid entry.');
		seen.add(item.address);
		return {
			address: item.address,
			label: item.label,
			added_at: item.added_at,
			updated_at: item.updated_at
		};
	});
}

export function createSavedAddresses(storage: () => StorageLike) {
	let items = $state<SavedAddress[]>([]);
	let error = $state<string | null>(null);
	let ready = $state(false);
	function load() {
		try {
			items = parseSavedAddresses(storage().getItem(SAVED_ADDRESSES_KEY));
			error = null;
		} catch {
			error = 'Saved addresses could not be read. Existing browser data has not been changed.';
		}
		ready = true;
	}
	function write(change: (current: SavedAddress[]) => SavedAddress[]) {
		try {
			const target = storage();
			const next = change(parseSavedAddresses(target.getItem(SAVED_ADDRESSES_KEY)));
			const raw = JSON.stringify({ version: 1, items: next });
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
		load,
		save(address: string, label: string) {
			const cleaned = label.trim();
			if (!safeAddress(address) || !safeLabel(cleaned)) {
				error =
					'Use a valid address and a label of at most 80 characters without control characters.';
				return false;
			}
			return write((current) => {
				const existing = current.find((item) => item.address === address);
				if (!existing && current.length >= MAX_SAVED_ADDRESSES)
					throw new Error(
						'You can save up to 100 addresses in this browser. Remove one to add another.'
					);
				const now = Date.now();
				return [
					{ address, label: cleaned, added_at: existing?.added_at ?? now, updated_at: now },
					...current.filter((item) => item.address !== address)
				];
			});
		},
		remove(address: string) {
			return write((current) => current.filter((item) => item.address !== address));
		}
	};
}
export const savedAddresses = createSavedAddresses(() => localStorage);
