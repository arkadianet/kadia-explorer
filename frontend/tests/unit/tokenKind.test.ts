import { describe, it, expect } from 'vitest';
import {
	formatSharePct,
	isNftKind,
	shareBarWidth,
	tokenDisplayName,
	tokenKindLabel
} from '$lib/token/kind';
import type { TokenKind } from '$lib/api/types';

describe('tokenKindLabel', () => {
	it('names every EIP-4 kind', () => {
		expect(tokenKindLabel('nft-picture')).toBe('Picture NFT');
		expect(tokenKindLabel('nft-audio')).toBe('Audio NFT');
		expect(tokenKindLabel('nft-video')).toBe('Video NFT');
		expect(tokenKindLabel('membership')).toBe('Membership');
		expect(tokenKindLabel('token')).toBe('Token');
	});

	it('falls back to "Token" for a kind the backend learned first', () => {
		expect(tokenKindLabel('nft-hologram' as TokenKind)).toBe('Token');
	});
});

describe('isNftKind', () => {
	it('covers the three media kinds only', () => {
		expect(isNftKind('nft-picture')).toBe(true);
		expect(isNftKind('nft-audio')).toBe(true);
		expect(isNftKind('nft-video')).toBe(true);
		expect(isNftKind('membership')).toBe(false);
		expect(isNftKind('token')).toBe(false);
	});
});

describe('tokenDisplayName', () => {
	const id = '0'.repeat(30) + 'abcdef' + '1'.repeat(28);

	it('uses the minted name when there is one', () => {
		expect(tokenDisplayName('SigUSD', id)).toBe('SigUSD');
	});

	it('trims surrounding whitespace', () => {
		expect(tokenDisplayName('  SigUSD \n', id)).toBe('SigUSD');
	});

	it('falls back to the truncated id for an empty or blank name', () => {
		expect(tokenDisplayName('', id)).toBe('00000000…111111');
		expect(tokenDisplayName('   ', id)).toBe('00000000…111111');
	});
});

describe('shareBarWidth', () => {
	it('passes a plain percentage through', () => {
		expect(shareBarWidth('42.5')).toBe(42.5);
	});

	it('clamps above 100 and below 0', () => {
		expect(shareBarWidth('100')).toBe(100);
		expect(shareBarWidth('100.0000001')).toBe(100);
		expect(shareBarWidth('-3')).toBe(0);
	});

	it('gives a dust holder a visible sliver instead of nothing', () => {
		expect(shareBarWidth('0.00001')).toBe(0.5);
		// The shape the API actually sends for a sub-0.01% holder.
		expect(shareBarWidth('0.0007')).toBe(0.5);
		expect(shareBarWidth('0.0001')).toBe(0.5);
	});

	it('shows no bar at all for zero', () => {
		// The API's bare "0": an empty holder, or a fully burned token.
		expect(shareBarWidth('0')).toBe(0);
	});

	it('treats an unparsable share as no bar', () => {
		expect(shareBarWidth('')).toBe(0);
		expect(shareBarWidth('n/a')).toBe(0);
	});
});

describe('formatSharePct', () => {
	it('keeps two decimals so the column stays one width', () => {
		expect(formatSharePct('50')).toBe('50.00%');
		expect(formatSharePct('1.6666667')).toBe('1.67%');
		// 0.01% is the bottom of the two-decimal band on both sides of the wire.
		expect(formatSharePct('0.01')).toBe('0.01%');
	});

	it('never rounds a real holder down to nothing', () => {
		// The sub-0.01% strings the API sends, all of which mean "some, but not much".
		expect(formatSharePct('0.0099')).toBe('<0.01%');
		expect(formatSharePct('0.0007')).toBe('<0.01%');
		expect(formatSharePct('0.001')).toBe('<0.01%');
		expect(formatSharePct('0.0001')).toBe('<0.01%');
		expect(formatSharePct('0.000004')).toBe('<0.01%');
	});

	it('handles zero, over-100 and unparsable shares', () => {
		// The API's bare "0" — an empty holder, or a fully burned token — is the only
		// share that owns nothing.
		expect(formatSharePct('0')).toBe('0%');
		expect(formatSharePct('100.4')).toBe('100.00%');
		expect(formatSharePct('n/a')).toBe('—');
	});
});
