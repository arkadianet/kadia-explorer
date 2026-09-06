import type { TxDto } from '$lib/api/types';

/** Ergo's storage-rent period: a box becomes rent-claimable 1,051,200 blocks (≈ 4 years)
 * after the height it was created at. */
export const RENT_PERIOD = 1_051_200;

export type TxKind = 'rent' | 'token' | 'payment';

export interface TxKindInfo {
	kind: TxKind;
	/** The words shown in the list. */
	label: string;
	/** How the label was decided — rendered as the row's `title`, so no classification on
	 * screen is unexplained. */
	why: string;
}

/**
 * What a transaction is, decided only from the transaction the API returned.
 *
 * A storage-rent claim is recognisable without any privileged knowledge: the miner spends a
 * box old enough to be claimable and hands the remainder straight back to the script it came
 * from, so an input's creation height is at least a rent period below the block and one of
 * the outputs is locked by the same tree. Anything moving a token is a token transfer;
 * everything else is a payment.
 *
 * Inputs whose box the indexer could not resolve are simply not evidence either way, so a
 * transaction with no resolved inputs falls through to the value-based branches.
 */
export function txKind(tx: TxDto): TxKindInfo {
	const outputTrees = new Set(tx.outputs.map((o) => o.tree_hash));

	for (const input of tx.inputs) {
		const box = input.box;
		if (!box) continue;
		if (box.creation_height <= tx.height - RENT_PERIOD && outputTrees.has(box.tree_hash)) {
			return {
				kind: 'rent',
				label: 'Storage rent claim',
				why: `An input created at height ${box.creation_height} is at least ${RENT_PERIOD.toLocaleString('en-US')} blocks below block ${tx.height}, and an output returns to the same script.`
			};
		}
	}

	const tokensOut = tx.outputs.reduce((n, o) => n + o.tokens.length, 0);
	if (tokensOut > 0) {
		return {
			kind: 'token',
			label: 'Token transfer',
			why: `${tokensOut} token entr${tokensOut === 1 ? 'y' : 'ies'} appear in the outputs.`
		};
	}

	return {
		kind: 'payment',
		label: 'Payment',
		why: 'No claimable input returns to its own script and no token moves, so this is a plain transfer of ERG.'
	};
}
