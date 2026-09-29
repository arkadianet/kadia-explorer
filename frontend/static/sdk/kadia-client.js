// Portable browser/Node ESM client. Download this file and operations.js together.
// Type declarations are provided separately. No dependency or credential storage.
import { operations } from './operations.js';

export class KadiaApiError extends Error {
	constructor(status, data, retryAfter = null) {
		super(typeof data?.detail === 'string' ? data.detail : `Kadia API returned HTTP ${status}.`);
		this.name = 'KadiaApiError';
		this.status = status;
		this.code = typeof data?.code === 'string' ? data.code : null;
		this.retryAfter = retryAfter;
		this.data = data;
	}
}
export class KadiaLimitError extends Error {
	constructor(message) {
		super(message);
		this.name = 'KadiaLimitError';
	}
}

function accepts(value, rule) {
	if (rule.anyOf) return rule.anyOf.some((r) => accepts(value, r));
	if (rule.enum && !rule.enum.includes(value)) return false;
	if (rule.type === 'integer')
		return (
			Number.isSafeInteger(value) &&
			value >= (rule.minimum ?? -Infinity) &&
			value <= (rule.maximum ?? Infinity)
		);
	if (rule.type === 'string')
		return (
			typeof value === 'string' &&
			value.length >= (rule.minLength ?? 0) &&
			value.length <= (rule.maxLength ?? Infinity) &&
			(!rule['x-max-utf8-bytes'] ||
				new TextEncoder().encode(value).length <= rule['x-max-utf8-bytes']) &&
			(!rule.pattern || new RegExp(rule.pattern).test(value))
		);
	if (rule.type === 'array')
		return (
			Array.isArray(value) &&
			value.length >= rule.minItems &&
			value.length <= rule.maxItems &&
			value.every((item) => accepts(item, rule.items))
		);
	return false;
}
function prepare(operation, parameters) {
	if (!Object.hasOwn(operations, operation)) throw new TypeError('Unsupported API operation.');
	const spec = operations[operation];
	if (!parameters || typeof parameters !== 'object' || Array.isArray(parameters))
		throw new TypeError('Parameters must be an object.');
	for (const key of spec.required)
		if (parameters[key] === undefined) throw new TypeError(`Missing parameter: ${key}`);
	for (const [key, value] of Object.entries(parameters)) {
		if (!Object.hasOwn(spec.parameters, key)) throw new TypeError(`Unsupported parameter: ${key}`);
		if (value !== undefined && !accepts(value, spec.parameters[key]))
			throw new TypeError(`Invalid parameter: ${key}`);
	}
	if (
		['compareBalances', 'networkHistory', 'mining'].includes(operation) &&
		parameters.from_height > parameters.to_height
	)
		throw new TypeError('from_height must not exceed to_height.');
	if (
		['networkHistory', 'mining'].includes(operation) &&
		parameters.to_height - parameters.from_height >= 20160
	)
		throw new KadiaLimitError('Header observations are limited to 20,160 blocks per request.');
	if (
		operation === 'addressActivity' &&
		parameters.from_ms !== undefined &&
		parameters.to_ms !== undefined &&
		parameters.from_ms >= parameters.to_ms
	)
		throw new TypeError('from_ms must precede to_ms.');
	if (operation === 'searchTokens' && /^[\t\n\v\f\r ]*$/.test(parameters.q))
		throw new TypeError('Token-name query must not be empty after ASCII whitespace normalization.');
	for (const [height, pin] of [
		['height', 'block_id'],
		['from_height', 'from_block_id'],
		['to_height', 'to_block_id']
	]) {
		if (parameters[height] === 0 && parameters[pin] !== undefined)
			throw new TypeError('Genesis has no block ID.');
	}
	let path = spec.path;
	const query = new URLSearchParams();
	for (const [key, value] of Object.entries(parameters)) {
		if (value === undefined) continue;
		if (path.includes(`{${key}}`))
			path = path.replace(`{${key}}`, encodeURIComponent(String(value)));
		else if (spec.method !== 'POST') query.set(key, String(value));
	}
	if (spec.paged) query.set('consistency', 'strict');
	let body;
	if (spec.method === 'POST') {
		const encoder = new TextEncoder();
		const sizes = parameters.addresses.map((value) => encoder.encode(value).length);
		if (sizes.some((size) => size > 4096) || sizes.reduce((a, b) => a + b, 0) > 128000)
			throw new KadiaLimitError('Address selection exceeds the byte limit.');
		body = JSON.stringify(parameters);
		if (encoder.encode(body).length > 512000)
			throw new KadiaLimitError('Request exceeds the byte limit.');
	}
	return { path: path + (query.size ? '?' + query : ''), method: spec.method ?? 'GET', body };
}

async function readJson(response, maxBytes, signal) {
	const declared = response.headers.get('content-length');
	if (declared && Number(declared) > maxBytes) {
		await response.body?.cancel();
		throw new KadiaLimitError('Response exceeds the byte limit.');
	}
	const reader = response.body?.getReader();
	if (!reader) return null;
	const cancel = () => {
		void reader.cancel().catch(() => {});
	};
	signal.addEventListener('abort', cancel, { once: true });
	const decoder = new TextDecoder('utf-8', { fatal: true });
	let invalidEncoding = false;
	const decode = (value, stream = false) => {
		try {
			return decoder.decode(value, { stream });
		} catch (error) {
			invalidEncoding = true;
			throw error;
		}
	};
	let bytes = 0,
		text = '';
	try {
		for (;;) {
			signal.throwIfAborted();
			const { value, done } = await reader.read();
			signal.throwIfAborted();
			if (done) break;
			bytes += value.byteLength;
			if (bytes > maxBytes) {
				await reader.cancel();
				throw new KadiaLimitError('Response exceeds the byte limit.');
			}
			text += decode(value, true);
		}
		text += decode();
		if (!text) return null;
		try {
			return JSON.parse(text);
		} catch {
			if (!response.ok) return null;
			throw new Error('The API returned invalid JSON.');
		}
	} catch (error) {
		if (!invalidEncoding) throw error;
		await reader.cancel().catch(() => {});
		if (!response.ok) return null;
		throw new Error('The API returned invalid UTF-8.', { cause: error });
	} finally {
		signal.removeEventListener('abort', cancel);
		reader.releaseLock();
	}
}

export function createKadiaClient({
	baseUrl,
	fetch: fetcher = globalThis.fetch,
	timeoutMs = 15000,
	maxResponseBytes = 2097152
} = {}) {
	const base = new URL(baseUrl);
	if (
		!['http:', 'https:'].includes(base.protocol) ||
		base.username ||
		base.password ||
		base.search ||
		base.hash
	)
		throw new TypeError('Use an HTTP(S) API base URL without credentials, query or fragment.');
	base.pathname = base.pathname.replace(/\/+$/, '') + '/';
	if (!Number.isSafeInteger(timeoutMs) || timeoutMs < 1 || timeoutMs > 60000)
		throw new TypeError('timeoutMs must be between 1 and 60000.');
	if (!Number.isSafeInteger(maxResponseBytes) || maxResponseBytes < 1 || maxResponseBytes > 8388608)
		throw new TypeError('maxResponseBytes must be between 1 and 8388608.');
	if (typeof fetcher !== 'function') throw new TypeError('A Fetch implementation is required.');
	async function observe(operation, parameters, { signal } = {}) {
		const prepared = prepare(operation, parameters);
		const url = new URL(prepared.path.slice(1), base);
		const controller = new AbortController();
		const abort = () => controller.abort(signal.reason);
		if (signal?.aborted) abort();
		else signal?.addEventListener('abort', abort, { once: true });
		const timer = setTimeout(
			() => controller.abort(new DOMException('API request timed out.', 'TimeoutError')),
			timeoutMs
		);
		try {
			controller.signal.throwIfAborted();
			const response = await fetcher(url.href, {
				method: prepared.method,
				body: prepared.body,
				signal: controller.signal,
				credentials: 'omit',
				redirect: 'error',
				referrerPolicy: 'no-referrer',
				headers: {
					Accept: 'application/json',
					...(prepared.body ? { 'Content-Type': 'application/json' } : {})
				}
			});
			const data = await readJson(response, maxResponseBytes, controller.signal);
			if (!response.ok)
				throw new KadiaApiError(response.status, data, response.headers.get('retry-after'));
			if (data === null || typeof data !== 'object')
				throw new Error('The API returned no structured response.');
			const coverage = response.headers.get('x-explorer-completeness');
			return {
				data,
				completeness: coverage === 'complete' || coverage === 'incomplete' ? coverage : 'unknown',
				status: response.status
			};
		} finally {
			clearTimeout(timer);
			signal?.removeEventListener('abort', abort);
		}
	}
	async function request(operation, parameters, options) {
		return (await observe(operation, parameters, options)).data;
	}
	async function* pages(operation, parameters, { maxPages = 10, maxItems = 5000, signal } = {}) {
		if (!Object.hasOwn(operations, operation) || !operations[operation].paged)
			throw new TypeError('This operation does not support strict page walking.');
		if (
			!Number.isSafeInteger(maxPages) ||
			maxPages < 1 ||
			maxPages > 100 ||
			!Number.isSafeInteger(maxItems) ||
			maxItems < 1 ||
			maxItems > 50000
		)
			throw new TypeError('Page walk limits are invalid.');
		let current = { ...parameters },
			items = 0,
			anchor;
		const seen = new Set();
		for (let i = 0; i < maxPages; i++) {
			const data = await request(operation, current, { signal });
			if (
				!Array.isArray(data.items) ||
				data.consistency !== 'strict' ||
				!Object.hasOwn(data, 'anchor') ||
				(data.next_cursor !== null && typeof data.next_cursor !== 'string')
			)
				throw new Error('Strict pagination metadata is unavailable.');
			const identity = JSON.stringify(data.anchor);
			if (anchor !== undefined && identity !== anchor)
				throw new Error('Page snapshot changed; discard the walk and restart.');
			anchor = identity;
			items += data.items.length;
			if (items > maxItems)
				throw new KadiaLimitError(
					'Page walk exceeds its item limit; previously yielded pages are incomplete.'
				);
			if (data.next_cursor !== null) {
				if (!data.next_cursor || typeof data.next_snapshot !== 'string' || !data.next_snapshot)
					throw new Error('Continuation is missing its cursor or snapshot.');
				if (seen.has(data.next_cursor)) throw new Error('Repeated page cursor; discard the walk.');
				seen.add(data.next_cursor);
				current = { ...current, cursor: data.next_cursor, snapshot: data.next_snapshot };
			}
			yield data;
			if (data.next_cursor === null) return;
		}
		throw new KadiaLimitError(
			'Page walk exceeds its page limit; previously yielded pages are incomplete.'
		);
	}
	return Object.freeze({ request, observe, pages });
}
