import type { BoxDto, TokenDto, TxDto } from '$lib/api/types';

export interface TokenChange extends Omit<TokenDto, 'amount'> {
	amount: bigint;
}
export interface AddressEffect {
	tree: string;
	address: string | null;
	kind: BoxDto['kind'];
	erg: bigint;
	tokens: TokenChange[];
	inputs: number;
	outputs: number;
}

/** All arithmetic is in raw integer units. Unknown inputs invalidate every net amount:
 * an unresolved input might belong to any output address, including the selected one. */
export function transactionEffects(tx: TxDto) {
	const groups = new Map<string, AddressEffect>();
	const tokens = new Map<string, Map<string, TokenChange>>();
	const supply = new Map<string, bigint>();
	let ergTotal = 0n;
	function add(box: BoxDto, sign: bigint) {
		let effect = groups.get(box.tree_hash);
		if (!effect) {
			effect = {
				tree: box.tree_hash,
				address: box.address,
				kind: box.kind,
				erg: 0n,
				tokens: [],
				inputs: 0,
				outputs: 0
			};
			groups.set(box.tree_hash, effect);
			tokens.set(box.tree_hash, new Map());
		}
		if (sign < 0n) effect.inputs++;
		else effect.outputs++;
		effect.erg += sign * BigInt(box.value);
		ergTotal += sign * BigInt(box.value);
		const assets = tokens.get(box.tree_hash)!;
		for (const token of box.tokens) {
			const delta = sign * BigInt(token.amount);
			const existing = assets.get(token.id);
			if (existing) existing.amount += delta;
			else assets.set(token.id, { ...token, amount: delta });
			supply.set(token.id, (supply.get(token.id) ?? 0n) + delta);
		}
	}
	for (const input of tx.inputs) if (input.box) add(input.box, -1n);
	for (const output of tx.outputs) add(output, 1n);
	for (const effect of groups.values()) {
		effect.tokens = [...tokens.get(effect.tree)!.values()].filter((t) => t.amount !== 0n);
	}
	const missingInputs = tx.inputs.filter((input) => !input.box).length;
	return {
		complete: missingInputs === 0,
		missingInputs,
		addresses: [...groups.values()],
		ergTotal,
		supplyChanges: [...supply].filter(([, n]) => n !== 0n)
	};
}
