/**
 * Builds a small in-memory copy of the xp-api data model out of the three node-JSON block
 * fixtures at `<repo>/tests/fixtures/blocks/*.json`, shaped exactly like the DTOs in
 * `src/lib/api/types.ts` (see `crates/xp-api/src/dto.rs` for the authoritative shapes).
 *
 * Deliberate deviations from the real server, all of them because three contiguous mainnet
 * blocks are not enough to exercise the UI:
 *
 *  - **Filler blocks.** The three real blocks alone cannot demonstrate cursor pagination at
 *    the mock's page size of 5, so heights below 1866000 are synthesised (empty blocks whose
 *    ids chain onto the real 1866000 parent id).
 *  - **Addresses.** Deriving a real P2PK/P2S address needs the sigma serialiser, which the
 *    frontend does not have. Every box therefore reports `address: null` (a state the UI
 *    handles by falling back to `tree_hash`) except the single richest ergo tree, which is
 *    mapped to the synthetic base58 string `MOCK_ADDRESS` so the populated address page is
 *    testable. `tree_hash` is sha256 of the serialised tree — stable, 64 hex, but not the
 *    store's Blake2b hash.
 *  - **Rent selection.** `maturity_height` and `due_nano` follow the real consensus rules
 *    (`creation + 1_051_200`, `size * 1_250_000` capped at value), which puts every fixture
 *    box's maturity ~1.05 M blocks past the tip — so nothing is genuinely "upcoming" or
 *    "eligible". The mock instead marks the `ELIGIBLE_COUNT` most valuable unspent boxes as
 *    `claimable_at_tip` and serves them from `/v1/rent/eligible`, with the remaining unspent
 *    boxes served (maturity ascending) from `/v1/rent/upcoming` regardless of the `blocks`
 *    window.
 *  - **JSON number precision.** The node fixtures encode `value` and token `amount` as JSON
 *    numbers, so `JSON.parse` gives them back as doubles. Every fixture amount is well under
 *    2^53 (the largest is the ~1.41e15 nanoERG emission box), so the round trip is exact —
 *    but a fixture with a larger amount would silently lose precision here, which the real
 *    server never does. Amounts are converted to `BigInt` immediately and every DTO carries
 *    them as decimal strings, as the API contract requires.
 *  - **Fees.** A tx fee is `sum(known inputs) - sum(outputs)`, which is only computable when
 *    every input box was created inside the three fixture blocks; otherwise it is 0. A
 *    block's `reward` is the second output of its first transaction (the miner's share of
 *    the emission spend) and its `fees` the sum of its tx fees.
 *  - **Tokens.** The blocks that *minted* the fixtures' tokens are far behind the fixture
 *    range, so there is no EIP-4 mint box to read a name, description or R7 type tag out of.
 *    Every observed token therefore gets an unnamed `TokenInfoDto` (`kind: 'token'`) whose
 *    "mint" is the oldest box in the fixtures that carries it, and exactly one synthetic
 *    EIP-4 token (`SYNTHETIC_TOKEN`) is minted into an unspent box of the richest tree so
 *    that a named, decimal-scaled token is on screen somewhere. `emission` is every unit
 *    ever seen, `supply` is what unspent boxes still hold, and `burned` is the difference —
 *    which makes the holders' `share_pct` add up to 100.
 *  - **Templates.** A real template hash is the ergo tree with its constants stripped; the
 *    frontend cannot strip constants, so a box's `template_hash` is a stable sha256 of its
 *    whole tree. Template and tree are therefore 1:1 here, where the real store maps many
 *    trees onto one template.
 *  - **Registers.** `/v1/registers/{reg}/{value}/boxes` keys on blake2b-256 of the raw
 *    register bytes server-side, and this Node build has no blake2b-256 (`getHashes()` lists
 *    only blake2b512/blake2s256). The frontend never hashes — it passes the raw hex from the
 *    URL straight through — so the mock indexes by that raw hex instead, which makes the
 *    same request URL resolve to the same boxes without a hash implementation in the mock.
 */

import { createHash } from 'node:crypto';
import { readFileSync } from 'node:fs';
import { fileURLToPath } from 'node:url';
import type {
	AddressDto,
	BlockDto,
	BoxDto,
	RentItemDto,
	RichlistItemDto,
	StatusDto,
	TemplateDto,
	TokenHolderDto,
	TokenInfoDto,
	TxDto
} from '../../../src/lib/api/types.ts';

/** Consensus rent constants, mirrored from `crates/xp-types/src/rent.rs`. */
const RENT_PERIOD = 1_051_200;
const RENT_PER_BYTE = 1_250_000n;

/** Heights of the committed block fixtures, oldest first. */
export const FIXTURE_HEIGHTS = [1866000, 1866001, 1866002] as const;
/** Lowest synthesised filler height; heights below the first fixture block. */
const FILLER_FROM = 1865990;
/** How many unspent boxes the mock reports as claimable right now. */
const ELIGIBLE_COUNT = 8;

/**
 * The one address string the mock resolves, mapped to the richest ergo tree. Base58 with no
 * `0OIl`, long enough for `classify()`'s address rule and the server's `looks_like_address`.
 */
export const MOCK_ADDRESS = '9mockAddressRichestTreeQqWweeRrTtYyUuPpAaSsDdFfGg';

/**
 * The one named, decimal-carrying token in the dataset. Minted (by fiat) into an unspent box
 * of the richest tree, so the address page, the box card and the token page all have a token
 * whose name and decimal point are visible rather than an id and a raw integer.
 */
export const SYNTHETIC_TOKEN = {
	id: sha256Hex('synthetic-token:mock-explorer'),
	name: 'Mock Explorer Token',
	description: 'A synthetic EIP-4 token minted by the e2e mock so names and decimals show up.',
	decimals: 2,
	/** 123456 at 2 decimals — "1,234.56" once `formatTokenAmount` has scaled it. */
	amount: '123456'
} as const;

// ---------------------------------------------------------------------------- node JSON

interface NodeAsset {
	tokenId: string;
	amount: number;
}

interface NodeOutput {
	boxId: string;
	value: number;
	ergoTree: string;
	assets: NodeAsset[];
	creationHeight: number;
	additionalRegisters: Record<string, unknown>;
	transactionId: string;
	index: number;
}

interface NodeTx {
	id: string;
	inputs: { boxId: string }[];
	dataInputs: { boxId: string }[];
	outputs: NodeOutput[];
	size: number;
}

interface NodeBlock {
	header: {
		id: string;
		parentId: string;
		height: number;
		timestamp: number;
		difficulty: string;
		size: number;
		version: number;
		powSolutions: { pk: string };
	};
	blockTransactions: { transactions: NodeTx[] };
	size: number;
}

const FIXTURE_DIR = fileURLToPath(new URL('../../../../tests/fixtures/blocks/', import.meta.url));

function readBlock(height: number): NodeBlock {
	return JSON.parse(readFileSync(`${FIXTURE_DIR}${height}.json`, 'utf8')) as NodeBlock;
}

// ------------------------------------------------------------------------------- helpers

function sha256Hex(input: string): string {
	return createHash('sha256').update(input).digest('hex');
}

function treeHashOf(ergoTree: string): string {
	return sha256Hex(`tree:${ergoTree}`);
}

function fillerId(height: number): string {
	return sha256Hex(`block:${height}`);
}

function rentDue(size: number, value: bigint): bigint {
	const due = BigInt(size) * RENT_PER_BYTE;
	return due < value ? due : value;
}

/**
 * Approximate serialised size of a box: the ergo tree's bytes plus rough allowances for the
 * value, creation height, registers and each token entry. The node fixtures do not carry a
 * per-box size, and this feeds both the box page's "Size" fact and the rent due, so it only
 * has to be stable and plausible — it is not the store's exact byte count.
 */
/**
 * A holder's share of a token's supply, as the decimal string the API sends, computed in
 * BigInt (an amount can exceed 2^53) via ten-thousandths of a percent: two decimals from
 * 0.01% up, up to four decimals with trailing zeros trimmed below that, and a bare "0" only
 * for an empty holder or for a supply of zero — every unit burned or spent out of the
 * fixture range — which has no shares to divide.
 */
function sharePct(amount: bigint, supply: bigint): string {
	if (supply <= 0n || amount <= 0n) return '0';
	const t = (amount * 1_000_000n) / supply || 1n;
	if (t >= 100n) return (Number(t / 100n) / 100).toFixed(2);
	return `0.${t.toString().padStart(4, '0')}`.replace(/0+$/, '');
}

function boxSize(out: NodeOutput): number {
	return Math.ceil(out.ergoTree.length / 2) + 40 + out.assets.length * 40;
}

// --------------------------------------------------------------------------- the dataset

export interface Dataset {
	status: StatusDto;
	/** All blocks, height descending. */
	blocks: BlockDto[];
	blockByHeight: Map<number, BlockDto>;
	blockById: Map<string, BlockDto>;
	/** All transactions, newest block first then index descending. */
	txs: TxDto[];
	txById: Map<string, TxDto>;
	/** Transaction ids per block height, in block order. */
	txIdsByHeight: Map<number, string[]>;
	boxById: Map<string, BoxDto>;
	/** Box ids per ergo-tree hash, newest first. */
	boxIdsByTree: Map<string, string[]>;
	/** Transaction ids touching a tree (as input or output), newest first. */
	txIdsByTree: Map<string, string[]>;
	addressByTree: Map<string, string | null>;
	treeByAddress: Map<string, string>;
	richlist: RichlistItemDto[];
	rentUpcoming: RentItemDto[];
	rentEligible: RentItemDto[];
	addresses: Map<string, AddressDto>;
	/** Every token seen in a fixture box, `sort=newest` order (mint height descending). */
	tokens: TokenInfoDto[];
	/** The same tokens in `sort=holders` order (holder count descending). */
	tokensByHolders: TokenInfoDto[];
	tokenById: Map<string, TokenInfoDto>;
	/** Holders per token, amount descending — one row per ergo tree with an unspent box. */
	tokenHolders: Map<string, TokenHolderDto[]>;
	/** Box ids per token, newest first. */
	boxIdsByToken: Map<string, string[]>;
	templates: Map<string, TemplateDto>;
	/** Box ids per script template hash, newest first. */
	boxIdsByTemplate: Map<string, string[]>;
	/** Box ids per `${reg}:${rawValueHex}`, newest first. See the register note above. */
	boxIdsByRegister: Map<string, string[]>;
}

/** Key into `boxIdsByRegister`. The value hex is lower-cased, as the frontend sends it. */
export function registerKey(reg: string, valueHex: string): string {
	return `${reg.toUpperCase()}:${valueHex.toLowerCase()}`;
}

export function buildDataset(): Dataset {
	const nodeBlocks = FIXTURE_HEIGHTS.map(readBlock);
	const tip = nodeBlocks[nodeBlocks.length - 1].header.height;

	// --- boxes -------------------------------------------------------------------------
	// Pass 1 creates every output box; pass 2 marks the ones spent inside the fixtures.
	const boxById = new Map<string, BoxDto>();
	const timestampByHeight = new Map<number, number>();
	const eligibleIds = new Set<string>();

	for (const nb of nodeBlocks) {
		timestampByHeight.set(nb.header.height, nb.header.timestamp);
		for (const tx of nb.blockTransactions.transactions) {
			for (const out of tx.outputs) {
				const size = boxSize(out);
				const value = BigInt(out.value);
				boxById.set(out.boxId, {
					id: out.boxId,
					tx_id: tx.id,
					index: out.index,
					value: value.toString(),
					creation_height: out.creationHeight,
					ergo_tree: out.ergoTree,
					address: null,
					template_hash: sha256Hex(`template:${out.ergoTree}`),
					tree_hash: treeHashOf(out.ergoTree),
					tokens: out.assets.map((a) => ({
						id: a.tokenId,
						amount: String(a.amount),
						name: null,
						decimals: null
					})),
					registers: out.additionalRegisters,
					size,
					spent_by: null,
					spent_height: null,
					rent: {
						maturity_height: out.creationHeight + RENT_PERIOD,
						due_nano: rentDue(size, value).toString(),
						claimable_at_tip: false
					}
				});
			}
		}
	}

	for (const nb of nodeBlocks) {
		for (const tx of nb.blockTransactions.transactions) {
			for (const input of tx.inputs) {
				const spent = boxById.get(input.boxId);
				if (!spent) continue;
				spent.spent_by = tx.id;
				spent.spent_height = nb.header.height;
			}
		}
	}

	// --- transactions ------------------------------------------------------------------
	const txById = new Map<string, TxDto>();
	const txIdsByHeight = new Map<number, string[]>();
	const feeByTxId = new Map<string, bigint>();
	const ordered: TxDto[] = [];

	for (const nb of nodeBlocks) {
		const height = nb.header.height;
		const ids: string[] = [];
		nb.blockTransactions.transactions.forEach((tx, index) => {
			const inputs = tx.inputs.map((i) => ({ id: i.boxId, box: boxById.get(i.boxId) ?? null }));
			const outputs = tx.outputs.map((o) => boxById.get(o.boxId) as BoxDto);
			const outSum = outputs.reduce((acc, o) => acc + BigInt(o.value), 0n);
			// Only computable when every input box was created inside the fixtures.
			const allKnown = inputs.every((i) => i.box !== null);
			const inSum = inputs.reduce((acc, i) => acc + (i.box ? BigInt(i.box.value) : 0n), 0n);
			const fee = allKnown && inSum > outSum ? inSum - outSum : 0n;
			feeByTxId.set(tx.id, fee);

			const dto: TxDto = {
				id: tx.id,
				height,
				index,
				timestamp: nb.header.timestamp,
				size: tx.size,
				fee: fee.toString(),
				inputs,
				data_inputs: tx.dataInputs.map((d) => d.boxId),
				outputs
			};
			txById.set(tx.id, dto);
			ordered.push(dto);
			ids.push(tx.id);
		});
		txIdsByHeight.set(height, ids);
	}

	// Newest first: block height descending, then index descending within a block.
	const txs = [...ordered].sort((a, b) => b.height - a.height || b.index - a.index);

	// --- blocks ------------------------------------------------------------------------
	const blocksAsc: BlockDto[] = [];
	const firstReal = nodeBlocks[0];
	for (let h = FILLER_FROM; h < firstReal.header.height; h++) {
		// The topmost filler block must carry the real 1866000 parent id so the chain links up.
		const id = h === firstReal.header.height - 1 ? firstReal.header.parentId : fillerId(h);
		const parent =
			h - 1 === firstReal.header.height - 1 ? firstReal.header.parentId : fillerId(h - 1);
		blocksAsc.push({
			id,
			height: h,
			parent_id: parent,
			timestamp: firstReal.header.timestamp - (firstReal.header.height - h) * 120_000,
			difficulty: firstReal.header.difficulty,
			miner_pk: firstReal.header.powSolutions.pk,
			tx_count: 0,
			size: 220,
			fees: '0',
			reward: '0',
			version: firstReal.header.version
		});
		timestampByHeight.set(h, blocksAsc[blocksAsc.length - 1].timestamp);
	}

	for (const nb of nodeBlocks) {
		const txsOfBlock = nb.blockTransactions.transactions;
		const fees = txsOfBlock.reduce((acc, t) => acc + (feeByTxId.get(t.id) ?? 0n), 0n);
		// The coinbase-ish first tx spends the emission box: output 0 re-creates it, output 1
		// is the miner's share.
		const reward = BigInt(txsOfBlock[0]?.outputs[1]?.value ?? 0);
		blocksAsc.push({
			id: nb.header.id,
			height: nb.header.height,
			parent_id: nb.header.parentId,
			timestamp: nb.header.timestamp,
			difficulty: nb.header.difficulty,
			miner_pk: nb.header.powSolutions.pk,
			tx_count: txsOfBlock.length,
			size: nb.size,
			fees: fees.toString(),
			reward: reward.toString(),
			version: nb.header.version
		});
	}

	const blocks = [...blocksAsc].sort((a, b) => b.height - a.height);
	const blockByHeight = new Map(blocks.map((b) => [b.height, b]));
	const blockById = new Map(blocks.map((b) => [b.id, b]));

	// --- trees, balances, addresses ----------------------------------------------------
	const boxIdsByTree = new Map<string, string[]>();
	const txIdsByTree = new Map<string, string[]>();
	const balanceByTree = new Map<string, bigint>();
	const unspentCountByTree = new Map<string, number>();
	const seenByTree = new Map<string, { first: number; last: number }>();
	const tokensByTree = new Map<string, Map<string, bigint>>();

	// Boxes newest-first: iterate the newest-first tx order.
	for (const tx of txs) {
		const trees = new Set<string>();
		for (const out of tx.outputs) {
			trees.add(out.tree_hash);
			const list = boxIdsByTree.get(out.tree_hash) ?? [];
			list.push(out.id);
			boxIdsByTree.set(out.tree_hash, list);

			const seen = seenByTree.get(out.tree_hash);
			seenByTree.set(out.tree_hash, {
				first: seen ? Math.min(seen.first, tx.height) : tx.height,
				last: seen ? Math.max(seen.last, tx.height) : tx.height
			});

			if (out.spent_by === null) {
				balanceByTree.set(
					out.tree_hash,
					(balanceByTree.get(out.tree_hash) ?? 0n) + BigInt(out.value)
				);
				unspentCountByTree.set(out.tree_hash, (unspentCountByTree.get(out.tree_hash) ?? 0) + 1);
				const bag = tokensByTree.get(out.tree_hash) ?? new Map<string, bigint>();
				for (const t of out.tokens) bag.set(t.id, (bag.get(t.id) ?? 0n) + BigInt(t.amount));
				tokensByTree.set(out.tree_hash, bag);
			}
		}
		for (const input of tx.inputs) if (input.box) trees.add(input.box.tree_hash);
		for (const tree of trees) {
			const list = txIdsByTree.get(tree) ?? [];
			list.push(tx.id);
			txIdsByTree.set(tree, list);
		}
	}

	const byBalance = [...balanceByTree.entries()].sort(
		(a, b) => (b[1] > a[1] ? 1 : b[1] < a[1] ? -1 : 0) || a[0].localeCompare(b[0])
	);
	const richestTree = byBalance[0]?.[0] ?? '';

	const addressByTree = new Map<string, string | null>();
	for (const tree of boxIdsByTree.keys()) {
		addressByTree.set(tree, tree === richestTree ? MOCK_ADDRESS : null);
	}
	const treeByAddress = new Map<string, string>([[MOCK_ADDRESS, richestTree]]);

	// Stamp the resolved address onto the richest tree's boxes so box/tx/rent views link out.
	for (const id of boxIdsByTree.get(richestTree) ?? []) {
		const b = boxById.get(id);
		if (b) b.address = MOCK_ADDRESS;
	}

	// Mint the one synthetic EIP-4 token into an unspent box of the richest tree, before the
	// balances below are read off, so it shows up on the address page as well as the box.
	const syntheticHost = (boxIdsByTree.get(richestTree) ?? [])
		.map((id) => boxById.get(id))
		.find((b): b is BoxDto => b !== undefined && b.spent_by === null);
	if (syntheticHost) {
		syntheticHost.tokens = [
			...syntheticHost.tokens,
			{
				id: SYNTHETIC_TOKEN.id,
				amount: SYNTHETIC_TOKEN.amount,
				name: SYNTHETIC_TOKEN.name,
				decimals: SYNTHETIC_TOKEN.decimals
			}
		];
		const bag = tokensByTree.get(richestTree) ?? new Map<string, bigint>();
		bag.set(SYNTHETIC_TOKEN.id, BigInt(SYNTHETIC_TOKEN.amount));
		tokensByTree.set(richestTree, bag);
	}

	const richlist: RichlistItemDto[] = byBalance.map(([tree, nano]) => ({
		address: addressByTree.get(tree) ?? null,
		tree_hash: tree,
		nano: nano.toString()
	}));

	const addresses = new Map<string, AddressDto>();
	{
		const seen = seenByTree.get(richestTree);
		addresses.set(MOCK_ADDRESS, {
			address: MOCK_ADDRESS,
			tree_hash: richestTree,
			balance: {
				nano: (balanceByTree.get(richestTree) ?? 0n).toString(),
				tokens: [...(tokensByTree.get(richestTree) ?? new Map<string, bigint>())].map(
					([id, amount]) => ({ id, amount: amount.toString(), name: null, decimals: null })
				)
			},
			box_count: unspentCountByTree.get(richestTree) ?? 0,
			tx_count: (txIdsByTree.get(richestTree) ?? []).length,
			first_seen: seen?.first ?? 0,
			last_seen: seen?.last ?? 0
		});
	}

	// --- tokens, templates, registers --------------------------------------------------
	// One newest-first pass over every output builds all three indexes at once, so each of
	// them lists boxes in the same order the address and richlist views use.
	interface TokenAcc {
		/** Every unit ever seen in a fixture box. */
		emission: bigint;
		/** Units still sitting in an unspent box — the token's `supply`. */
		unspent: bigint;
		boxCount: number;
		/** Unspent amount per ergo tree; the tree count is the holder count. */
		holders: Map<string, bigint>;
		mintTx: string;
		mintBox: string;
		mintHeight: number;
	}
	interface TemplateAcc {
		boxes: number;
		unspent: number;
		first: number;
		address: string | null;
	}

	const tokenAcc = new Map<string, TokenAcc>();
	const templateAcc = new Map<string, TemplateAcc>();
	const boxIdsByToken = new Map<string, string[]>();
	const boxIdsByTemplate = new Map<string, string[]>();
	const boxIdsByRegister = new Map<string, string[]>();

	const push = (map: Map<string, string[]>, key: string, id: string) => {
		const list = map.get(key) ?? [];
		list.push(id);
		map.set(key, list);
	};

	for (const tx of txs) {
		for (const out of tx.outputs) {
			for (const t of out.tokens) {
				const amount = BigInt(t.amount);
				const acc = tokenAcc.get(t.id);
				if (acc === undefined) {
					tokenAcc.set(t.id, {
						emission: amount,
						unspent: out.spent_by === null ? amount : 0n,
						boxCount: 1,
						holders:
							out.spent_by === null
								? new Map([[out.tree_hash, amount]])
								: new Map<string, bigint>(),
						mintTx: tx.id,
						mintBox: out.id,
						mintHeight: out.creation_height
					});
				} else {
					acc.emission += amount;
					acc.boxCount += 1;
					if (out.spent_by === null) {
						acc.unspent += amount;
						acc.holders.set(out.tree_hash, (acc.holders.get(out.tree_hash) ?? 0n) + amount);
					}
					// The scan runs newest first, so anything at or below the height recorded so far
					// is the older box — and the oldest one stands in for the mint.
					if (out.creation_height <= acc.mintHeight) {
						acc.mintTx = tx.id;
						acc.mintBox = out.id;
						acc.mintHeight = out.creation_height;
					}
				}
				push(boxIdsByToken, t.id, out.id);
			}

			if (out.template_hash) {
				push(boxIdsByTemplate, out.template_hash, out.id);
				const acc = templateAcc.get(out.template_hash);
				if (acc === undefined) {
					templateAcc.set(out.template_hash, {
						boxes: 1,
						unspent: out.spent_by === null ? 1 : 0,
						first: tx.height,
						address: out.address
					});
				} else {
					acc.boxes += 1;
					if (out.spent_by === null) acc.unspent += 1;
					acc.first = Math.min(acc.first, tx.height);
					acc.address ??= out.address;
				}
			}

			for (const [reg, value] of Object.entries(out.registers ?? {})) {
				if (!/^R[4-9]$/.test(reg) || typeof value !== 'string' || value === '') continue;
				push(boxIdsByRegister, registerKey(reg, value), out.id);
			}
		}
	}

	const tokens: TokenInfoDto[] = [...tokenAcc.entries()].map(([id, acc]) => {
		const synthetic = id === SYNTHETIC_TOKEN.id;
		return {
			id,
			// Nothing but the synthetic token has a mint box in range to read a name out of.
			name: synthetic ? SYNTHETIC_TOKEN.name : '',
			description: synthetic ? SYNTHETIC_TOKEN.description : '',
			decimals: synthetic ? SYNTHETIC_TOKEN.decimals : null,
			// The raw R7 type tag as the store holds it, not a spec name: "0101" is EIP-4's
			// plain-token tag, which is the `kind` every mock token carries.
			token_type: synthetic ? '0101' : null,
			kind: 'token',
			emission: acc.emission.toString(),
			burned: (acc.emission - acc.unspent).toString(),
			supply: acc.unspent.toString(),
			holder_count: acc.holders.size,
			box_count: acc.boxCount,
			mint_tx: acc.mintTx,
			mint_box: acc.mintBox,
			mint_height: acc.mintHeight
		};
	});
	const tokenById = new Map(tokens.map((t) => [t.id, t]));

	const tokenHolders = new Map<string, TokenHolderDto[]>();
	for (const [id, acc] of tokenAcc) {
		const rows = [...acc.holders.entries()]
			.sort((a, b) => (b[1] > a[1] ? 1 : b[1] < a[1] ? -1 : 0) || a[0].localeCompare(b[0]))
			.map(([tree, amount]) => ({
				address: addressByTree.get(tree) ?? null,
				tree_hash: tree,
				amount: amount.toString(),
				share_pct: sharePct(amount, acc.unspent)
			}));
		tokenHolders.set(id, rows);
	}

	// The store joins a mint row onto every token amount it serves; only the synthetic token
	// has one, so everything else keeps the null name/decimals the UI falls back on.
	const label = (id: string) => {
		const info = tokenById.get(id);
		return { name: info && info.name !== '' ? info.name : null, decimals: info?.decimals ?? null };
	};
	for (const b of boxById.values()) b.tokens = b.tokens.map((t) => ({ ...t, ...label(t.id) }));
	for (const info of addresses.values()) {
		info.balance.tokens = info.balance.tokens.map((t) => ({ ...t, ...label(t.id) }));
	}

	const templates = new Map<string, TemplateDto>(
		[...templateAcc.entries()].map(([hash, acc]) => [
			hash,
			{
				hash,
				box_count: acc.boxes,
				unspent_count: acc.unspent,
				first_seen: acc.first,
				example_address: acc.address
			}
		])
	);

	// --- rent --------------------------------------------------------------------------
	const unspent = [...boxById.values()].filter((b) => b.spent_by === null);
	const byValueDesc = [...unspent].sort((a, b) =>
		BigInt(b.value) > BigInt(a.value) ? 1 : BigInt(b.value) < BigInt(a.value) ? -1 : 0
	);
	for (const b of byValueDesc.slice(0, ELIGIBLE_COUNT)) {
		b.rent.claimable_at_tip = true;
		eligibleIds.add(b.id);
	}

	const toItem = (b: BoxDto): RentItemDto => ({ maturity_height: b.rent.maturity_height, box: b });
	const rentEligible = byValueDesc.slice(0, ELIGIBLE_COUNT).map(toItem);
	const rentUpcoming = unspent
		.filter((b) => !eligibleIds.has(b.id))
		.sort((a, b) => a.rent.maturity_height - b.rent.maturity_height || a.id.localeCompare(b.id))
		.map(toItem);

	const status: StatusDto = {
		indexed: tip,
		best: tip,
		mode: 'tip',
		source: 'mock',
		halted: null,
		lag_blocks: 0,
		stalled: null,
		inflight_reads: 0,
		rate_limited_total: 0
	};

	return {
		status,
		blocks,
		blockByHeight,
		blockById,
		txs,
		txById,
		txIdsByHeight,
		boxById,
		boxIdsByTree,
		txIdsByTree,
		addressByTree,
		treeByAddress,
		richlist,
		rentUpcoming,
		rentEligible,
		addresses,
		tokens: [...tokens].sort((a, b) => b.mint_height - a.mint_height || a.id.localeCompare(b.id)),
		tokensByHolders: [...tokens].sort(
			(a, b) => b.holder_count - a.holder_count || a.id.localeCompare(b.id)
		),
		tokenById,
		tokenHolders,
		boxIdsByToken,
		templates,
		boxIdsByTemplate,
		boxIdsByRegister
	};
}
