import { afterEach, describe, expect, it, vi } from 'vitest';
import { collBytes, fetchMedia, MAX_MEDIA_BYTES, mediaSource, tokenMedia } from '$lib/token/media';

function register(text: string) {
	const bytes = new TextEncoder().encode(text);
	expect(bytes.length).toBeLessThan(128);
	return (
		'0e' +
		bytes.length.toString(16).padStart(2, '0') +
		Array.from(bytes, (byte) => byte.toString(16).padStart(2, '0')).join('')
	);
}
afterEach(() => vi.unstubAllGlobals());

describe('token media metadata', () => {
	it.each(['nft-picture', 'nft-audio', 'nft-video'] as const)(
		'reads the bounded %s link and declared hash',
		(kind) => {
			const media = tokenMedia(kind, {
				R9: register('https://media.example.com/file'),
				R8: '0e20' + 'ab'.repeat(32)
			})!;
			expect(media.source?.url).toBe('https://media.example.com/file');
			expect(media.hash).toBe('ab'.repeat(32));
		}
	);
	it('does not guess from malformed, non-byte-array or invalid UTF-8 values', () => {
		for (const value of ['0e05ab', '0e016100', '0e01ff', '0402', '3c0e0e', '0e80808080808000'])
			expect(tokenMedia('nft-picture', { R9: value })?.source).toBeNull();
		expect(collBytes('0e0161')).toEqual(new Uint8Array([97]));
		expect(tokenMedia('token', { R9: register('https://media.example.com/file') })).toBeNull();
	});
	it('resolves explicit IPFS content links without allowing traversal', () => {
		const cid = 'Qm' + 'a'.repeat(44);
		expect(mediaSource(`ipfs://${cid}/art.png`)).toEqual({
			url: `https://ipfs.io/ipfs/${cid}/art.png`,
			host: 'ipfs.io',
			gateway: true
		});
		for (const path of [
			'/../secret',
			'/%2e%2e/secret',
			'/%2fsecret',
			'/%00',
			'/file?url=elsewhere'
		])
			expect(mediaSource(`ipfs://${cid}${path}`)).toBeNull();
	});
	it.each([
		'javascript:alert(1)',
		'data:image/svg+xml,hi',
		'http://example.com/a',
		'//example.com/a',
		'https://user:password@example.com/a',
		'https://localhost/a',
		'https://localhost./a',
		'https://printer.local./a',
		'https://127.1/a',
		'https://2130706433/a',
		'https://10.0.0.1/a',
		'https://192.168.1.1/a',
		'https://[::1]/a',
		'https://example.com:8443/a',
		'https://example.com/\nfile'
	])('rejects unsafe source %s', (uri) => {
		expect(mediaSource(uri)).toBeNull();
	});
});

describe('explicit media download', () => {
	it('cancels unread error bodies', async () => {
		const cancel = vi.fn();
		vi.stubGlobal(
			'fetch',
			vi.fn().mockResolvedValue(new Response(new ReadableStream({ cancel }), { status: 503 }))
		);
		await expect(
			fetchMedia(mediaSource('https://media.example.com/a')!, new AbortController().signal)
		).rejects.toThrow('readable file');
		expect(cancel).toHaveBeenCalledOnce();
	});
	it('omits credentials and referrers, disallows redirects, and preserves downloaded bytes', async () => {
		const mock = vi
			.fn()
			.mockResolvedValue(
				new Response(new Uint8Array([1, 2, 3]), { headers: { 'content-type': 'image/png' } })
			);
		vi.stubGlobal('fetch', mock);
		const signal = new AbortController().signal;
		const blob = await fetchMedia(mediaSource('https://media.example.com/a')!, signal);
		expect(mock).toHaveBeenCalledWith('https://media.example.com/a', {
			signal,
			credentials: 'omit',
			referrerPolicy: 'no-referrer',
			redirect: 'error',
			mode: 'cors'
		});
		expect(Array.from(new Uint8Array(await blob.arrayBuffer()))).toEqual([1, 2, 3]);
	});
	it('cancels an oversized advertised file before reading it', async () => {
		const cancel = vi.fn();
		vi.stubGlobal(
			'fetch',
			vi.fn().mockResolvedValue(
				new Response(new ReadableStream({ cancel }), {
					headers: { 'content-length': String(MAX_MEDIA_BYTES + 1) }
				})
			)
		);
		await expect(
			fetchMedia(mediaSource('https://media.example.com/a')!, new AbortController().signal)
		).rejects.toThrow('32 MiB');
		expect(cancel).toHaveBeenCalledOnce();
	});
	it('enforces the byte cap even without a Content-Length header', async () => {
		const cancel = vi.fn();
		vi.stubGlobal(
			'fetch',
			vi.fn().mockResolvedValue(
				new Response(
					new ReadableStream({
						start(controller) {
							controller.enqueue(new Uint8Array(MAX_MEDIA_BYTES + 1));
						},
						cancel
					})
				)
			)
		);
		await expect(
			fetchMedia(mediaSource('https://media.example.com/a')!, new AbortController().signal)
		).rejects.toThrow('32 MiB');
		expect(cancel).toHaveBeenCalledOnce();
	});
});
