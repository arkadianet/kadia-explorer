// Token presentation rules shared by /tokens and /token/[id]: what a kind is called, what a
// token is called when it was minted without a name, and how wide a holder's share bar is.

import type { TokenKind } from '$lib/api/types';
import { truncateMiddle } from '$lib/format/hash';

/** EIP-4's R7 type tags, in the words a reader knows them by. */
const KIND_LABELS: Record<TokenKind, string> = {
	'nft-picture': 'Picture NFT',
	'nft-audio': 'Audio NFT',
	'nft-video': 'Video NFT',
	membership: 'Membership',
	token: 'Token'
};

export function tokenKindLabel(kind: TokenKind): string {
	// An unknown tag (a backend that learned a new kind before the frontend did) still has to
	// render something, so it falls back to the plainest word rather than "undefined".
	return KIND_LABELS[kind] ?? KIND_LABELS.token;
}

/** Whether the kind is one of EIP-4's media NFTs — the ones that carry the media mark. */
export function isNftKind(kind: TokenKind): boolean {
	return kind === 'nft-picture' || kind === 'nft-audio' || kind === 'nft-video';
}

/** A token's display name: its minted name, or its truncated id when it was minted with an
 * empty name (R4 absent or blank). Never returns an empty string, so a table cell is never
 * a blank link. */
export function tokenDisplayName(name: string, id: string): string {
	const trimmed = name.trim();
	return trimmed === '' ? truncateMiddle(id) : trimmed;
}

/** A holder's share as text, to two decimals so the column stays one width. The API can
 * return a share with many decimals (a dust holder of a huge supply); printing it raw would
 * overrun the column, and rounding it to "0.00%" would claim the holder owns nothing — hence
 * the explicit "<0.01%" band. */
export function formatSharePct(sharePct: string): string {
	const pct = Number.parseFloat(sharePct);
	if (!Number.isFinite(pct)) return '—';
	if (pct <= 0) return '0%';
	if (pct < 0.01) return '<0.01%';
	return `${Math.min(pct, 100).toFixed(2)}%`;
}

/** Width, in percent, of a holder's share bar. `share_pct` arrives as a decimal string; a
 * bar is clamped to 0–100 so a malformed or rounded-over value cannot spill out of its
 * track, and a sub-pixel share still shows a sliver rather than nothing. */
export function shareBarWidth(sharePct: string): number {
	const pct = Number.parseFloat(sharePct);
	if (!Number.isFinite(pct) || pct <= 0) return 0;
	if (pct >= 100) return 100;
	return Math.max(pct, 0.5);
}
