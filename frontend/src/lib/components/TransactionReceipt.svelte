<script lang="ts">
	import type { TxDto, TxEvidence } from '$lib/api/types';
	import { api } from '$lib/api/endpoints';
	import { transactionEffects, type AddressEffect } from '$lib/tx/effects';
	import { matchedEvidence, RENT_PERIOD, rentInputs, txKind } from '$lib/tx/kind';
	import { formatErg, formatTokenAmount } from '$lib/format/amount';
	import { truncateMiddle } from '$lib/format/hash';
	import Hash from './Hash.svelte';
	import Icon from './Icon.svelte';
	import Tabs from './Tabs.svelte';
	import { status } from '$lib/status/status.svelte';

	let {
		tx,
		onrefresh,
		refreshing = false
	}: {
		tx: TxDto;
		onrefresh: () => void;
		refreshing?: boolean;
	} = $props();
	let active = $state('receipt');
	let selectedTree = $state('');
	let evidence = $state<TxEvidence | null>(null);
	let evidenceState = $state<'loading' | 'ready' | 'unavailable' | 'not-needed'>('not-needed');
	let retry = $state(0);
	const effects = $derived(transactionEffects(tx));
	const kind = $derived(txKind(tx, evidence));
	const claims = $derived(rentInputs(tx, evidence));
	const checkedEvidence = $derived(matchedEvidence(tx, evidence));
	const addresses = $derived(effects.addresses.filter((a) => a.kind !== 'fee'));
	const selected = $derived(
		addresses.find((a) => a.tree === selectedTree) ??
			addresses.find((a) => a.tokens.some((t) => t.amount > 0n)) ??
			addresses.find((a) => a.erg > 0n) ??
			addresses[0]
	);
	const hasSnapshot = $derived(tx.indexed_height != null && tx.indexed_height >= tx.height);
	const confirmations = $derived(hasSnapshot ? tx.indexed_height! - tx.height + 1 : null);
	const stale = $derived(status.health.state !== 'healthy');
	const snapshotChanged = $derived(
		tx.indexed_height != null &&
			status.current?.indexed != null &&
			tx.indexed_height !== status.current.indexed
	);

	$effect(() => {
		const current = tx;
		void retry;
		let cancelled = false;
		evidence = null;
		const needsProof = current.inputs.some(
			(i) => i.box && current.height - i.box.creation_height >= RENT_PERIOD
		);
		if (needsProof && current.block_id) {
			evidenceState = 'loading';
			api
				.txEvidence(current.id)
				.then((value) => {
					if (cancelled) return;
					evidence = matchedEvidence(current, value);
					evidenceState = evidence ? 'ready' : 'unavailable';
				})
				.catch(() => {
					if (!cancelled) evidenceState = 'unavailable';
				});
		} else evidenceState = needsProof ? 'unavailable' : 'not-needed';
		return () => {
			cancelled = true;
		};
	});

	function signedErg(amount: bigint) {
		return `${amount > 0n ? '+' : ''}${formatErg(amount)}`;
	}
	function tokenAmount(amount: bigint, decimals: number | null) {
		return `${amount > 0n ? '+' : ''}${formatTokenAmount(amount.toString(), decimals)}`;
	}
	function role(address: AddressEffect) {
		if (address.kind === 'fee') return 'Miner fee contract';
		if (address.kind === 'emission') return 'Emission contract';
		if (claims.some((i) => i.box?.tree_hash === address.tree)) return 'Rent-affected address';
		return 'Address';
	}
</script>

<section class="receipt-section" aria-label="Transaction receipt">
	<div class="receipt-heading">
		<div>
			<p class="eyebrow">Transaction receipt</p>
			<h2>{kind.label === 'Transaction' ? 'Transaction activity' : kind.label}</h2>
		</div>
		<span class="confirmation"
			><Icon name="layers" size={15} />
			{#if confirmations !== null}{confirmations.toLocaleString('en-US')} confirmation{confirmations ===
				1
					? ''
					: 's'} at last check{:else}Included · confirmations unavailable{/if}
		</span>
	</div>
	<div class="snapshot">
		<p>
			{#if hasSnapshot}Checked at indexed block {tx.indexed_height!.toLocaleString(
					'en-US'
				)}.{:else}Confirmation snapshot unavailable.{/if}
			{#if stale}
				{status.health.label}.{:else if snapshotChanged}
				Index has changed; refresh to recheck this transaction.{/if}
		</p>
		<button type="button" class="refresh" disabled={refreshing} onclick={onrefresh}
			>{refreshing ? 'Checking…' : 'Refresh'}</button
		>
	</div>
	<Tabs
		tabs={[
			{ id: 'receipt', label: 'Receipt' },
			{ id: 'changes', label: 'Balance changes' },
			{ id: 'evidence', label: 'Evidence' }
		]}
		{active}
		onchange={(id) => (active = id)}
		label="Transaction views"
	/>
	<div
		id="panel-receipt"
		role="tabpanel"
		aria-labelledby="tab-receipt"
		hidden={active !== 'receipt'}
	>
		{#if !effects.complete}
			<div class="incomplete">
				<h3>Balance changes unavailable</h3>
				<p>
					{effects.missingInputs} input box{effects.missingInputs === 1 ? ' is' : 'es are'} outside the
					indexed data. Exact address changes require every input. The resolved boxes remain available
					below.
				</p>
			</div>
		{:else if selected}
			<div class="perspective">
				<label for="receipt-address">View changes for an address</label>
				<select
					id="receipt-address"
					value={selected.tree}
					onchange={(event) => (selectedTree = event.currentTarget.value)}
				>
					{#each addresses as address (address.tree)}<option value={address.tree}
							>{role(address)} · {truncateMiddle(address.address ?? address.tree)}</option
						>{/each}
				</select>
			</div>
			<div class="receipt" aria-live="polite">
				<p class="eyebrow">
					<Icon name={kind.kind === 'rent' ? 'rent-coin' : 'txs'} size={16} />{role(selected)} · net change
				</p>
				{#if selected.tokens.length}
					{#each selected.tokens as token (token.id)}
						<div class="asset">
							<div class="asset-icon"><Icon name="token" size={26} /></div>
							<div class="asset-value">
								<p class="amount" class:negative={token.amount < 0n}>
									{tokenAmount(token.amount, token.decimals)}
								</p>
								<a class="asset-name" href={`/token/${token.id}`}
									>{token.name?.trim() || 'Unnamed token'}</a
								>
								<p class="token-id">
									{truncateMiddle(token.id)}{token.decimals === null
										? ' · raw units; decimals unknown'
										: ''}
								</p>
							</div>
						</div>
					{/each}
				{:else}
					<div class="asset">
						<div class="asset-icon">Σ</div>
						<div class="asset-value">
							<p class="amount" class:negative={selected.erg < 0n}>{signedErg(selected.erg)}</p>
							<p class="asset-name">ERG</p>
						</div>
					</div>
				{/if}
				<div class="receipt-facts">
					<div>
						<p>ERG balance change</p>
						<strong class:negative={selected.erg < 0n}>{signedErg(selected.erg)} ERG</strong>
					</div>
					<div>
						<p>Fee for the entire transaction</p>
						<strong>{formatErg(tx.fee)} ERG</strong>
					</div>
				</div>
				<p class="explanation">
					{#if kind.kind === 'rent'}Storage rent was collected without a signature for {claims.length}
						mature input{claims.length === 1 ? '' : 's'}.
					{/if}{' '}These changes include returned change. The transaction fee is already reflected
					across address changes; do not add it again.
				</p>
			</div>
			<div class="selected-address">
				<span>{role(selected)}</span><Hash
					value={selected.address ?? selected.tree}
					href={selected.address ? `/address/${selected.address}` : undefined}
					head={12}
				/>
			</div>
		{/if}
		<p class="interpretation">
			{evidenceState === 'loading' ? 'Checking spending evidence…' : kind.why}
		</p>
		{#if evidenceState === 'unavailable'}<p class="notice">
				Spending evidence is unavailable. An old input alone does not establish a rent claim. <button
					type="button"
					class="refresh"
					onclick={() => retry++}>Retry evidence</button
				>
			</p>{/if}
	</div>
	<div
		id="panel-changes"
		role="tabpanel"
		aria-labelledby="tab-changes"
		hidden={active !== 'changes'}
	>
		{#if effects.complete}
			<div class="ledger">
				{#each effects.addresses as address (address.tree)}
					<div class="ledger-row">
						<div>
							<p>{role(address)}</p>
							<Hash
								value={address.address ?? address.tree}
								href={address.address ? `/address/${address.address}` : undefined}
								copy={false}
							/>
						</div>
						<div class="deltas">
							<p class:loss={address.erg < 0n}>{signedErg(address.erg)} ERG</p>
							{#each address.tokens as token (token.id)}<p class:loss={token.amount < 0n}>
									<a href={`/token/${token.id}`}
										>{tokenAmount(token.amount, token.decimals)}
										{token.name?.trim() || truncateMiddle(token.id)}</a
									>{#if token.decimals === null}<small>raw units; decimals unknown</small>{/if}
								</p>{/each}
						</div>
					</div>
				{/each}
			</div>
			<p class="notice">
				{effects.ergTotal === 0n
					? 'All ERG changes sum to zero.'
					: 'ERG totals do not reconcile; inspect the raw boxes.'}
				{effects.supplyChanges.length === 0
					? 'Token quantities are conserved.'
					: 'Token supply changes are listed below.'}
			</p>
			{#each effects.supplyChanges as [id, amount] (id)}<p class="supply-change">
					{amount > 0n ? 'Minted' : 'Burned'}: {amount < 0n ? -amount : amount} raw units ·
					<a href={`/token/${id}`}>{truncateMiddle(id)}</a>
				</p>{/each}
			<p class="interpretation">
				Grouped by locking script. Addresses do not establish ownership, automation or operator
				identity.
			</p>
		{:else}<p class="notice">
				Exact changes unavailable: {effects.missingInputs} unresolved input box{effects.missingInputs ===
				1
					? ''
					: 'es'}.
			</p>{/if}
	</div>
	<div
		id="panel-evidence"
		role="tabpanel"
		aria-labelledby="tab-evidence"
		hidden={active !== 'evidence'}
	>
		<h3>How this receipt was determined</h3>
		<p class="interpretation">{kind.why}</p>
		<dl class="evidence-facts">
			<div>
				<dt>Input coverage</dt>
				<dd>
					{tx.inputs.length - effects.missingInputs} of {tx.inputs.length} input boxes resolved
				</dd>
			</div>
			<div>
				<dt>Block anchor</dt>
				<dd>
					{#if tx.block_id}<Hash
							value={tx.block_id}
							href={`/blocks/${tx.height}`}
						/>{:else}Unavailable on this server{/if}
				</dd>
			</div>
			<div>
				<dt>Spending evidence</dt>
				<dd>
					{checkedEvidence
						? 'Reported by the configured node; checked against the indexed block. No independent consensus replay.'
						: evidenceState === 'loading'
							? 'Checking…'
							: 'Not available in this receipt.'}
				</dd>
			</div>
			{#each tx.inputs as input, i (input.id)}
				<div>
					<dt>Input {i + 1}</dt>
					<dd>
						<Hash value={input.id} href={`/box/${input.id}`} copy={false} />
						{#if input.box}<p>
								Age at spend: {(tx.height - input.box.creation_height).toLocaleString('en-US')} blocks
							</p>{/if}
						{#if checkedEvidence}<p>
								{checkedEvidence.inputs[i].proof === 'empty' ? 'Empty' : 'Nonempty'} proof · rent selector:
								{checkedEvidence.inputs[i].extension_127 ?? 'absent or unsupported'}
							</p>{/if}
					</dd>
				</div>
			{/each}
		</dl>
		<p class="interpretation">
			Automatic rent recognition currently covers standard P2PK inputs. Other scripts need
			historical validation rules or a supported decoder. Token names are mint metadata, not an
			endorsement.
		</p>
		<div class="evidence-links">
			<a href={`/v1/txs/${tx.id}`} target="_blank" rel="noreferrer">Transaction JSON ↗</a
			>{#if checkedEvidence}<a href={`/v1/txs/${tx.id}/evidence`} target="_blank" rel="noreferrer"
					>Spending evidence JSON ↗</a
				>{/if}
		</div>
	</div>
</section>

<style>
	.receipt-section {
		min-width: 0;
		display: grid;
		gap: var(--space-4);
	}
	.receipt-heading {
		display: flex;
		align-items: center;
		justify-content: space-between;
		gap: 16px;
		flex-wrap: wrap;
	}
	.eyebrow {
		font-size: 12px;
		text-transform: uppercase;
		letter-spacing: 0.08em;
		color: var(--fg-muted);
		display: flex;
		gap: 8px;
		align-items: center;
	}
	h2 {
		font-size: clamp(24px, 3vw, 32px);
		letter-spacing: -1px;
		margin-top: 6px;
	}
	h3 {
		font-size: var(--fs-title);
		margin-top: 16px;
	}
	.confirmation {
		display: flex;
		gap: 8px;
		align-items: center;
		font-size: 12px;
		padding: 8px 12px;
		border-radius: var(--radius-pill);
		background: var(--accent-wash);
		color: var(--accent-ink);
	}
	.snapshot {
		display: flex;
		gap: 12px;
		align-items: center;
		justify-content: space-between;
		font-size: 12px;
		color: var(--fg-muted);
	}
	.refresh {
		background: none;
		border: 0;
		color: var(--accent-ink);
		text-decoration: underline;
		padding: 8px;
		cursor: pointer;
		font: inherit;
		flex: none;
	}
	.refresh:disabled {
		cursor: wait;
		opacity: 0.7;
	}
	.perspective {
		display: flex;
		justify-content: space-between;
		align-items: center;
		gap: 12px;
		margin: 4px 0 16px;
		flex-wrap: wrap;
	}
	.perspective label {
		font-size: 13px;
	}
	select {
		font: inherit;
		font-size: 13px;
		background: var(--surface-solid);
		color: var(--fg);
		border: var(--rule);
		border-radius: var(--radius-control);
		padding: 12px;
		max-width: 100%;
	}
	.receipt {
		position: relative;
		isolation: isolate;
		overflow: hidden;
		background: #13392c;
		color: #f4fff6;
		border-radius: 20px;
		padding: 28px;
	}
	.receipt::before {
		content: '';
		z-index: -1;
		pointer-events: none;
		position: absolute;
		right: -125px;
		top: -235px;
		height: 560px;
		width: 600px;
		transform: rotate(-18deg);
		background: repeating-radial-gradient(
			ellipse at center,
			transparent 0 25px,
			#436453 26px 27px,
			transparent 28px 44px
		);
		opacity: 0.22;
		mask-image: linear-gradient(90deg, transparent, #000);
	}
	.receipt .eyebrow {
		color: #bbd4c6;
		font-size: 11px;
	}
	.asset {
		display: flex;
		gap: 18px;
		align-items: center;
		margin: 24px 0;
	}
	.asset-value {
		min-width: 0;
	}
	.asset-icon {
		display: grid;
		place-items: center;
		flex: none;
		width: 52px;
		height: 52px;
		transform: rotate(-7deg);
		background: #c4f6a1;
		color: #13392c;
		border-radius: 16px;
		font-size: 26px;
	}
	.amount {
		color: #c4f6a1;
		font-size: clamp(26px, 4vw, 52px);
		letter-spacing: -1.5px;
		font-variant-numeric: tabular-nums;
		line-height: 1.12;
		overflow-wrap: anywhere;
	}
	.asset-name {
		display: block;
		color: #f4fff6;
		font-size: 18px;
		margin-top: 6px;
		overflow-wrap: anywhere;
	}
	.token-id {
		color: #bbd4c6;
		font-size: 12px;
		margin-top: 4px;
	}
	.receipt-facts {
		display: grid;
		grid-template-columns: 1fr 1fr;
		gap: 20px;
		padding-top: 20px;
		border-top: 1px solid #436453;
	}
	.receipt-facts p {
		color: #bbd4c6;
		font-size: 12px;
	}
	.receipt-facts strong {
		display: block;
		margin-top: 5px;
		font-weight: 500;
		font-size: 18px;
		font-variant-numeric: tabular-nums;
		overflow-wrap: anywhere;
	}
	.receipt .negative {
		color: #f6c1a6;
	}
	.explanation {
		color: #bbd4c6;
		font-size: 13px;
		margin-top: 20px;
		max-width: 80ch;
	}
	.selected-address {
		display: flex;
		gap: 12px;
		flex-wrap: wrap;
		margin-top: 16px;
		font-size: 12px;
		color: var(--fg-muted);
	}
	.selected-address :global(.hash) {
		min-width: 0;
	}
	.interpretation {
		margin-top: 16px;
		color: var(--fg-muted);
		font-size: 13px;
	}
	.notice,
	.incomplete {
		padding: 16px;
		background: var(--surface-solid);
		border: var(--rule);
		border-radius: var(--radius-card);
		margin-top: 16px;
		font-size: 13px;
	}
	.incomplete h3 {
		margin: 0 0 8px;
	}
	.ledger-row {
		display: grid;
		grid-template-columns: minmax(0, 1fr) minmax(0, 1fr);
		gap: 16px;
		padding: 20px 0;
		border-bottom: var(--rule);
	}
	.ledger-row :global(.hash) {
		font-size: 12px;
	}
	.deltas {
		text-align: right;
		font-variant-numeric: tabular-nums;
		overflow-wrap: anywhere;
	}
	.deltas a {
		color: inherit;
	}
	.deltas small {
		display: block;
		color: var(--fg-muted);
	}
	.loss {
		color: var(--danger-ink);
	}
	.evidence-facts {
		margin-top: 12px;
	}
	.evidence-facts > div {
		display: grid;
		grid-template-columns: 160px minmax(0, 1fr);
		gap: 16px;
		border-bottom: var(--rule);
		padding: 16px 0;
		font-size: 13px;
	}
	dt {
		color: var(--fg-muted);
	}
	dd {
		margin: 0;
		overflow-wrap: anywhere;
	}
	.evidence-links {
		display: flex;
		gap: 16px;
		flex-wrap: wrap;
		margin-top: 16px;
		font-size: 13px;
	}
	.supply-change {
		margin-top: 12px;
		font-size: 13px;
		overflow-wrap: anywhere;
	}
	@media (max-width: 600px) {
		.receipt {
			padding: 20px;
		}
		.receipt-facts {
			grid-template-columns: 1fr;
		}
		.perspective {
			display: block;
		}
		select {
			width: 100%;
			margin-top: 8px;
			font-size: 16px;
		}
		.asset-icon {
			width: 40px;
			height: 40px;
			border-radius: 12px;
		}
		.asset {
			gap: 12px;
		}
		.evidence-facts > div {
			grid-template-columns: 1fr;
			gap: 6px;
		}
		.ledger-row {
			grid-template-columns: 1fr;
			gap: 8px;
		}
		.deltas {
			text-align: left;
		}
	}
</style>
