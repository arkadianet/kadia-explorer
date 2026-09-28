import type { TokenKind } from '$lib/api/types';

/** Only the bounded EIP-4 Coll[Byte] form is interpreted; other register types stay explicit. */
export function collBytes(raw: unknown): Uint8Array | null {
	if (typeof raw !== 'string' || raw.length > 16_384 || !/^0e(?:[0-9a-f]{2})+$/i.test(raw))
		return null;
	const bytes = Uint8Array.from(raw.match(/../g)!, (byte) => Number.parseInt(byte, 16));
	let size = 0;
	let factor = 1;
	for (let i = 1; i < Math.min(bytes.length, 6); i++) {
		size += (bytes[i] & 127) * factor;
		if (size > 8192) return null;
		if (!(bytes[i] & 128)) return size === bytes.length - i - 1 ? bytes.slice(i + 1) : null;
		factor *= 128;
	}
	return null;
}

export interface MediaSource {
	url: string;
	host: string;
	gateway: boolean;
}

function publicHost(host: string): boolean {
	host = host.replace(/\.$/, '');
	if (host === 'localhost' || host.endsWith('.localhost') || host.endsWith('.local')) return false;
	// Avoid loopback/private literals and ambiguous non-public hosts in untrusted metadata.
	if (host.includes(':') || !host.includes('.')) return false;
	if (/^\d+(?:\.\d+){3}$/.test(host)) {
		const [a, b] = host.split('.').map(Number);
		return !(
			a === 0 ||
			a === 10 ||
			a === 127 ||
			a >= 224 ||
			(a === 169 && b === 254) ||
			(a === 172 && b >= 16 && b <= 31) ||
			(a === 192 && b === 168) ||
			(a === 100 && b >= 64 && b <= 127)
		);
	}
	return true;
}

export function mediaSource(raw: string): MediaSource | null {
	// eslint-disable-next-line no-control-regex -- Untrusted URIs must not contain control characters.
	if (raw.length > 4096 || /[\u0000-\u0020\u007f\\]/.test(raw)) return null;
	if (raw.startsWith('ipfs://')) {
		const match =
			/^ipfs:\/\/(?:ipfs\/)?(Qm[1-9A-HJ-NP-Za-km-z]{44}|b[a-z2-7]{20,120})(\/[^?#]*)?$/.exec(raw);
		if (!match) return null;
		let path: string;
		try {
			path = (match[2] ?? '')
				.split('/')
				.map((part) => {
					const decoded = decodeURIComponent(part);
					// eslint-disable-next-line no-control-regex -- Reject encoded controls after decoding each path segment.
					if (decoded === '.' || decoded === '..' || /[\u0000-\u001f\u007f/\\]/.test(decoded))
						throw new Error('Invalid IPFS path');
					return encodeURIComponent(decoded);
				})
				.join('/');
		} catch {
			return null;
		}
		return { url: `https://ipfs.io/ipfs/${match[1]}${path}`, host: 'ipfs.io', gateway: true };
	}
	try {
		const url = new URL(raw);
		if (
			url.protocol !== 'https:' ||
			url.username ||
			url.password ||
			!publicHost(url.hostname) ||
			url.port
		)
			return null;
		return { url: url.href, host: url.hostname, gateway: false };
	} catch {
		return null;
	}
}

export function tokenMedia(kind: TokenKind, registers: Record<string, unknown> | null) {
	if (!['nft-picture', 'nft-audio', 'nft-video'].includes(kind)) return null;
	const bytes = collBytes(registers?.R9);
	let uri: string | null = null;
	if (bytes) {
		try {
			uri = new TextDecoder('utf-8', { fatal: true }).decode(bytes);
		} catch {
			/* Invalid UTF-8 is not a link. */
		}
	}
	const hash = collBytes(registers?.R8);
	return {
		kind:
			kind === 'nft-picture'
				? ('image' as const)
				: kind === 'nft-audio'
					? ('audio' as const)
					: ('video' as const),
		uri,
		source: uri ? mediaSource(uri) : null,
		hash:
			hash?.length === 32
				? Array.from(hash, (byte) => byte.toString(16).padStart(2, '0')).join('')
				: null,
		reason: !registers?.R9
			? 'No media link was declared in R9.'
			: !uri
				? 'This R9 encoding is not supported. Only a single EIP-4 UTF-8 byte-array link can be previewed.'
				: 'This media URI cannot be previewed. Public HTTPS and IPFS links are supported.'
	};
}

export const MAX_MEDIA_BYTES = 32 * 1024 * 1024;

/** Called only after consent. No cookies/referrer, redirects, or unbounded downloads. */
export async function fetchMedia(source: MediaSource, signal: AbortSignal): Promise<Blob> {
	const response = await fetch(source.url, {
		signal,
		credentials: 'omit',
		referrerPolicy: 'no-referrer',
		redirect: 'error',
		mode: 'cors'
	});
	if (!response.ok || !response.body) {
		await response.body?.cancel().catch(() => {});
		throw new Error('The media host did not return a readable file.');
	}
	const length = Number(response.headers.get('content-length'));
	if (length > MAX_MEDIA_BYTES) {
		await response.body.cancel();
		throw new Error('This file exceeds the 32 MiB preview limit.');
	}
	const reader = response.body.getReader();
	const parts: ArrayBuffer[] = [];
	let total = 0;
	try {
		while (true) {
			const part = await reader.read();
			if (part.done) break;
			total += part.value.byteLength;
			if (total > MAX_MEDIA_BYTES) throw new Error('This file exceeds the 32 MiB preview limit.');
			parts.push(part.value.slice().buffer);
		}
	} catch (error) {
		await reader.cancel().catch(() => {});
		throw error;
	} finally {
		reader.releaseLock();
	}
	return new Blob(parts, {
		type: response.headers.get('content-type') ?? 'application/octet-stream'
	});
}
