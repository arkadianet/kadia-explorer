import type { TxDto, TxEvidence } from '$lib/api/types';

export const RENT_PERIOD = 1_051_200;
export type TxKind = 'rent' | 'token' | 'payment' | 'emission' | 'fee' | 'unknown';
export interface TxKindInfo {
	kind: TxKind;
	label: string;
	why: string;
}

/** Canonical serialized SShort: type 03 followed by zigzag VLQ. Reject negative,
 * overlong, trailing and unsupported typed constants rather than guessing an index. */
export function rentPointer(hex: string | null): number | null {
	if (!hex || !/^03(?:[0-9a-fA-F]{2}){1,3}$/.test(hex)) return null;
	const bytes = hex
		.slice(2)
		.match(/../g)!
		.map((b) => parseInt(b, 16));
	let encoded = 0;
	for (let i = 0; i < bytes.length; i++) {
		const byte = bytes[i];
		if ((i === bytes.length - 1) !== byte < 128) return null;
		if (i > 0 && i === bytes.length - 1 && byte === 0) return null;
		encoded += (byte & 127) * 2 ** (7 * i);
	}
	if (encoded > 65535 || encoded % 2 !== 0) return null;
	return encoded / 2;
}

export function matchedEvidence(tx: TxDto, evidence?: TxEvidence | null): TxEvidence | null {
	if (
		!evidence ||
		evidence.tx_id !== tx.id ||
		evidence.height !== tx.height ||
		!tx.block_id ||
		evidence.block_id !== tx.block_id ||
		evidence.assurance !== 'trusted_node_response' ||
		evidence.inputs.length !== tx.inputs.length ||
		evidence.inputs.some((input, i) => input.id !== tx.inputs[i].id)
	)
		return null;
	return evidence;
}

export function rentInputs(tx: TxDto, evidence?: TxEvidence | null) {
	const checked = matchedEvidence(tx, evidence);
	if (!checked) return [];
	return tx.inputs.filter((input, i) => {
		const box = input.box;
		const proof = checked.inputs[i];
		const pointer = rentPointer(proof.extension_127);
		// A confirmed standard P2PK input cannot pass ordinary script validation with an
		// empty proof. Together with maturity + selector, this identifies the rent path
		// without assuming today's fee factor applied at every historical height.
		// Arbitrary scripts can succeed with empty proofs: deliberately abstain for them.
		return (
			box &&
			/^0008cd(?:02|03)[0-9a-f]{64}$/i.test(box.ergo_tree ?? '') &&
			tx.height - box.creation_height >= RENT_PERIOD &&
			proof.proof === 'empty' &&
			pointer !== null &&
			pointer < tx.outputs.length
		);
	});
}

/** Shared conservative interpretation. Neither old age, change outputs, token presence,
 * nor a fee-paying claim proves payment intent, automation, or operator identity. */
export function txKind(tx: TxDto, evidence?: TxEvidence | null): TxKindInfo {
	const rent = rentInputs(tx, evidence);
	if (rent.length)
		return {
			kind: 'rent',
			label: 'Storage rent claim',
			why: `${rent.length} mature P2PK input${rent.length === 1 ? '' : 's'} spent with an empty proof and a valid rent selector in the node-reported confirmed transaction.`
		};
	if (!tx.inputs.length || tx.inputs.some((i) => !i.box))
		return {
			kind: 'unknown',
			label: 'Transaction',
			why: 'Input evidence is incomplete. The transaction type is not established.'
		};
	if (tx.inputs.some((i) => i.box?.kind === 'emission'))
		return {
			kind: 'emission',
			label: 'Emission',
			why: 'Spends the indexed genesis emission contract.'
		};
	if (tx.inputs.every((i) => i.box?.kind === 'fee'))
		return {
			kind: 'fee',
			label: 'Fee collection',
			why: 'Every input spends the standard miner-fee contract.'
		};
	const checked = matchedEvidence(tx, evidence);
	if (
		tx.inputs.some(
			(i, index) =>
				tx.height - i.box!.creation_height >= RENT_PERIOD &&
				checked?.inputs[index].proof !== 'nonempty'
		)
	)
		return {
			kind: 'unknown',
			label: 'Transaction',
			why: 'An input has reached rent age. Available evidence does not establish its spending path.'
		};
	if (tx.inputs.some((i) => !/^0008cd(?:02|03)[0-9a-f]{64}$/i.test(i.box!.ergo_tree ?? ''))) {
		return {
			kind: 'unknown',
			label: 'Contract interaction',
			why: 'A script input needs a supported contract decoder to explain its purpose.'
		};
	}
	if (
		[...tx.inputs.flatMap((i) => (i.box ? [i.box] : [])), ...tx.outputs].some(
			(b) => b.tokens.length
		)
	) {
		return {
			kind: 'token',
			label: 'Token activity',
			why: 'Tokens are present in the resolved inputs or outputs. Balance changes show movement; supply changes show minting or burning.'
		};
	}
	return {
		kind: 'payment',
		label: 'ERG transfer',
		why: 'Resolved P2PK inputs and ERG-only outputs. Address deltas include change; ownership and payment intent are not inferred.'
	};
}
