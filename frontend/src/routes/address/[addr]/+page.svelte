<script lang="ts">
	import { goto } from '$app/navigation';
	import { page } from '$app/state';
	import Panel from '$lib/components/Panel.svelte';
	import PageHead from '$lib/components/PageHead.svelte';
	import Table from '$lib/components/Table.svelte';
	import Tabs from '$lib/components/Tabs.svelte';
	import InfiniteList from '$lib/components/InfiniteList.svelte';
	import Hash from '$lib/components/Hash.svelte';
	import Amount from '$lib/components/Amount.svelte';
	import Age from '$lib/components/Age.svelte';
	import RentBadge from '$lib/components/RentBadge.svelte';
	import EmptyState from '$lib/components/EmptyState.svelte';
	import Skeleton from '$lib/components/Skeleton.svelte';
	import SaveAddress from '$lib/components/SaveAddress.svelte';
	import AddressActivity from '$lib/components/AddressActivity.svelte';
	import AddressHistory from '$lib/components/AddressHistory.svelte';
	import AddressRentExposure from '$lib/components/AddressRentExposure.svelte';
	import { savedAddresses } from '$lib/addresses/saved.svelte';
	import { api } from '$lib/api/endpoints';
	import { createPager, type Pager } from '$lib/pager/pager.svelte';
	import { status } from '$lib/status/status.svelte';
	import { formatErg, formatTokenAmount, sumNano } from '$lib/format/amount';
	import { truncateMiddle } from '$lib/format/hash';
	import type { BoxDto, TxSummaryDto } from '$lib/api/types';
	import type { PageData } from './$types';

	const PAGE_SIZE = 50;

	let { data }: { data: PageData } = $props();

	const addr = $derived(data.addr);
	const info = $derived(data.info);
	const tip = $derived(status.current?.indexed ?? null);
	const savedIdentity = $derived(
		savedAddresses.items.find((item) => item.address === info?.address)
	);
	const TABS = $derived([
		{ id: 'activity', label: 'Activity' },
		{ id: 'tokens', label: 'Tokens', count: info?.balance.tokens.length ?? 0 },
		{ id: 'history', label: 'Historical snapshot' },
		{ id: 'txs', label: 'Transactions' },
		{ id: 'unspent', label: 'Unspent boxes' },
		{ id: 'boxes', label: 'All boxes' },
		{ id: 'rent', label: 'Rent' }
	]);
	const TAB_IDS = $derived(TABS.map((t) => t.id));

	/** Active tab, driven by the URL hash so a tab is linkable and survives reload. */
	const active = $derived.by(() => {
		const id = page.url.hash.replace(/^#/, '');
		return TAB_IDS.includes(id) ? id : 'activity';
	});

	function selectTab(id: string) {
		// `replaceState` so flipping between tabs does not pile up history entries.
		void goto(`#${id}`, { replaceState: true, noScroll: true, keepFocus: true });
	}

	// Each tab's data source is created on first activation, so opening the page costs one
	// activity request rather than fetching each tab in advance.
	let txPager = $state<Pager<TxSummaryDto> | null>(null);
	let unspentPager = $state<Pager<BoxDto> | null>(null);
	let boxPager = $state<Pager<BoxDto> | null>(null);

	// Navigating from one address to another reuses this component, so the lazily-created
	// per-tab state has to be dropped explicitly or the new address shows the old one's rows.
	// Plain `let` (not `$state`) so reading it here does not make this effect depend on it.
	let loadedFor: string | null = null;

	$effect(() => {
		if (addr === loadedFor) return;
		loadedFor = addr;
		txPager = null;
		unspentPager = null;
		boxPager = null;
	});

	$effect(() => {
		if (info === null) return;
		if (active === 'txs' && !txPager) {
			const p = createPager<TxSummaryDto>((c, snapshot) =>
				api.addressTxs(addr, c, PAGE_SIZE, undefined, undefined, snapshot)
			);
			txPager = p;
			void p.loadMore();
		} else if (active === 'unspent' && !unspentPager) {
			const p = createPager<BoxDto>((c, snapshot) =>
				api.addressBoxes(addr, true, c, PAGE_SIZE, undefined, undefined, snapshot)
			);
			unspentPager = p;
			void p.loadMore();
		} else if (active === 'boxes' && !boxPager) {
			const p = createPager<BoxDto>((c, snapshot) =>
				api.addressBoxes(addr, false, c, PAGE_SIZE, undefined, undefined, snapshot)
			);
			boxPager = p;
			void p.loadMore();
		}
	});

	const unspentTotal = $derived(
		unspentPager ? formatErg(sumNano(unspentPager.items.map((b) => b.value)), { maxFrac: 9 }) : '0'
	);
</script>

<svelte:head>
	<title>Address {truncateMiddle(addr, 10, 8)} — Ergo Explorer</title>
</svelte:head>

{#if info === null}
	<div class="head">
		<PageHead title="Address" id={addr} />
		<EmptyState
			message="Address not seen yet — no boxes for it in the index. It may never have been funded, or the indexer may not have reached its first transaction."
		/>
	</div>
	<Panel title="Historical snapshot"><AddressHistory address={addr} /></Panel>
{:else}
	<div class="address-head">
		<header class="address-masthead">
			<div class="address-title">
				<h1>Address</h1>
				<span class="address-balance" title={`${info.balance.nano} nanoERG`}
					>{formatErg(info.balance.nano, { maxFrac: 9 })}<small>ERG</small></span
				>
			</div>
			<div class="address-tools"><SaveAddress address={info.address} compact /></div>
			<div class="address-id">
				<Hash value={info.address} head={info.address.length} tail={0} />
			</div>
			{#if savedIdentity?.label || savedIdentity?.group}<p class="address-labels">
					{#if savedIdentity.label}<span>Local label <bdi>{savedIdentity.label}</bdi></span>{/if}
					{#if savedIdentity.group}<span>Group <bdi>{savedIdentity.group}</bdi></span>{/if}
				</p>{/if}
		</header>
		<dl class="address-facts">
			<div>
				<dt>Boxes</dt>
				<dd>{info.box_count}</dd>
			</div>
			<div>
				<dt>Tokens</dt>
				<dd>{info.balance.tokens.length}</dd>
			</div>
			<div>
				<dt>First seen</dt>
				<dd><a href={`/blocks/${info.first_seen}`}>{info.first_seen}</a></dd>
			</div>
			<div>
				<dt>Last seen</dt>
				<dd><a href={`/blocks/${info.last_seen}`}>{info.last_seen}</a></dd>
			</div>
		</dl>
	</div>

	{#snippet holdings()}
		{#if info.balance.tokens.length > 0}
			<div class="holdings-table">
				<Table>
					{#snippet head()}
						<tr>
							<th>Token</th>
							<th class="num">Amount</th>
						</tr>
					{/snippet}
					{#each info.balance.tokens as token (token.id)}
						<tr>
							<td>
								<!-- A minted name is prose and keeps the body face; a token minted without one
							     falls back to its truncated id in mono. Either way the cell links to the
							     token's own page. -->
								{#if token.name}
									<a class="token-name" href={`/token/${token.id}`} title={token.id}>{token.name}</a
									>
								{:else}
									<a class="mono" href={`/token/${token.id}`} title={token.id}
										>{truncateMiddle(token.id)}</a
									>
								{/if}
							</td>
							<td class="num mono">{formatTokenAmount(token.amount, token.decimals)}</td>
						</tr>
					{/each}
				</Table>
			</div>
		{:else}<EmptyState message="No token balances in this indexed address snapshot." />{/if}
	{/snippet}

	<Panel>
		<div class="address-section-tabs">
			<Tabs tabs={TABS} {active} onchange={selectTab} label="Address sections" />
		</div>

		<div role="tabpanel" id={`panel-${active}`} tabindex="0" aria-labelledby={`tab-${active}`}>
			{#if active === 'activity'}
				<AddressActivity address={addr} />
			{:else if active === 'tokens'}
				{@render holdings()}
			{:else if active === 'history'}
				<AddressHistory address={addr} />
			{:else if active === 'txs'}
				{#if txPager}
					<InfiniteList
						table
						columns={4}
						dense
						pager={txPager}
						empty="No transactions for this address in the indexed range."
					>
						{#snippet head()}
							<tr>
								<th>Id</th>
								<th class="num">Block</th>
								<th>Age</th>
								<th class="num">Fee</th>
							</tr>
						{/snippet}
						{#snippet children(tx: TxSummaryDto)}
							<tr>
								<td
									><Hash
										value={tx.id}
										href={`/tx/${tx.id}?address=${encodeURIComponent(addr)}`}
										copy={false}
									/></td
								>
								<td class="num mono"><a href={`/blocks/${tx.height}`}>{tx.height}</a></td>
								<td><Age ms={tx.timestamp} /></td>
								<td class="num"><Amount nano={tx.fee} maxFrac={3} /></td>
							</tr>
						{/snippet}
					</InfiniteList>
				{:else}
					<Skeleton />
				{/if}
			{:else if active === 'unspent'}
				{#if unspentPager}
					<InfiniteList
						table
						columns={5}
						dense
						pager={unspentPager}
						empty="Every box at this address has been spent."
					>
						{#snippet head()}
							<tr>
								<th>Id</th>
								<th class="num">Value</th>
								<th class="num">Created</th>
								<th class="num">Tokens</th>
								<th>Rent</th>
							</tr>
						{/snippet}
						{#snippet children(box: BoxDto)}
							<tr>
								<td><Hash value={box.id} href={`/box/${box.id}`} copy={false} /></td>
								<td class="num"><Amount nano={box.value} maxFrac={9} /></td>
								<td class="num mono"
									><a href={`/blocks/${box.creation_height}`}>{box.creation_height}</a></td
								>
								<td class="num mono">{box.tokens.length}</td>
								<td><RentBadge rent={box.rent} {tip} /></td>
							</tr>
						{/snippet}
					</InfiniteList>
					{#if unspentPager.items.length > 0}
						<p class="sum">
							Total{#if !unspentPager.done}&nbsp;loaded so far{/if}:
							<span class="mono">{unspentTotal}</span> ERG
						</p>
					{/if}
				{:else}
					<Skeleton />
				{/if}
			{:else if active === 'boxes'}
				{#if boxPager}
					<InfiniteList
						table
						columns={6}
						dense
						pager={boxPager}
						empty="No boxes for this address in the indexed range."
					>
						{#snippet head()}
							<tr>
								<th>Id</th>
								<th class="num">Value</th>
								<th class="num">Created</th>
								<th>Spent</th>
								<th class="num">Tokens</th>
								<th>Rent</th>
							</tr>
						{/snippet}
						{#snippet children(box: BoxDto)}
							<tr>
								<td><Hash value={box.id} href={`/box/${box.id}`} copy={false} /></td>
								<td class="num"><Amount nano={box.value} maxFrac={9} /></td>
								<td class="num mono"
									><a href={`/blocks/${box.creation_height}`}>{box.creation_height}</a></td
								>
								<td>
									{#if box.spent_by}
										<Hash value={box.spent_by} href={`/tx/${box.spent_by}`} copy={false} />
									{:else}
										<span class="muted">—</span>
									{/if}
								</td>
								<td class="num mono">{box.tokens.length}</td>
								<td>
									{#if box.spent_by}
										<span class="muted" title="Rent no longer applies — the box is spent">—</span>
									{:else}
										<RentBadge rent={box.rent} {tip} />
									{/if}
								</td>
							</tr>
						{/snippet}
					</InfiniteList>
				{:else}
					<Skeleton />
				{/if}
			{:else}
				<AddressRentExposure address={addr} />
			{/if}
		</div>
	</Panel>
{/if}

<style>
	.head {
		display: flex;
		flex-direction: column;
		gap: var(--space-4);
	}
	.address-head {
		display: flex;
		flex-direction: column;
		gap: 0;
		min-width: 0;
	}
	:global(
		:root[data-density][data-appearance] .content[data-page='address'] > .panel > .panel-body
	) {
		padding: var(--density-panel, 16px);
	}
	:global(:root[data-density][data-appearance] .content[data-page='address'])
		.address-section-tabs
		:global(.tabs) {
		flex-wrap: nowrap;
		overflow-x: auto;
		gap: 0 16px;
		margin-bottom: 0;
	}
	.address-section-tabs :global(.tab) {
		flex-shrink: 0;
		white-space: nowrap;
	}
	.address-masthead {
		display: grid;
		grid-template-columns: minmax(0, 1fr) auto;
		gap: 6px 16px;
		align-items: center;
		padding: var(--density-panel, 16px);
		border: var(--rule);
		border-top: 3px solid var(--accent-ink);
		border-radius: var(--radius-card);
		background: var(--surface-solid);
		min-width: 0;
	}
	.address-title {
		display: flex;
		align-items: baseline;
		justify-content: space-between;
		gap: 12px;
		flex-wrap: wrap;
	}
	.address-title h1 {
		font-family: var(--font-display, var(--font-sans));
		font-size: var(--density-title, 32px);
		line-height: 1.05;
		letter-spacing: -0.04em;
		margin: 0;
	}
	.address-balance {
		font: 600 clamp(21px, 2.5vw, 32px)/1.15 var(--font-number, var(--font-mono));
		letter-spacing: -0.035em;
		overflow-wrap: anywhere;
		min-width: 0;
	}
	.address-balance small {
		font: 500 12px var(--font-sans);
		margin-inline-start: 6px;
		letter-spacing: 0;
		color: var(--fg-muted);
	}
	.address-id {
		grid-column: 1/-1;
		margin-top: 0;
		font-size: 12px;
		color: var(--fg-muted);
		overflow-wrap: anywhere;
	}
	.address-id :global(.hash) {
		display: inline;
	}
	.address-tools {
		min-width: 0;
	}
	.address-labels {
		grid-column: 1/-1;
		display: flex;
		flex-wrap: wrap;
		gap: 4px 16px;
		margin: 0;
		font-size: 12px;
		color: var(--fg-muted);
		overflow-wrap: anywhere;
	}
	.address-labels bdi {
		color: var(--fg);
		font-weight: 650;
		margin-left: 6px;
	}
	.address-facts {
		margin: 0;
		padding: 0 var(--density-panel, 16px);
		display: flex;
		flex-wrap: wrap;
		gap: 0 20px;
		border-bottom: var(--rule);
	}
	.address-facts > div {
		display: flex;
		align-items: center;
		gap: 6px;
		min-height: 36px;
	}
	.address-facts dt {
		color: var(--fg-muted);
		font-size: 11px;
		margin: 0;
	}
	.address-facts dd {
		font: 500 13px/1.3 var(--font-mono);
		margin: 0;
		overflow-wrap: anywhere;
	}
	.holdings-table {
		padding: 0;
		margin-top: 8px;
	}
	:global([data-appearance='prism']) .address-head {
		display: grid;
		grid-template-columns: minmax(0, 1fr) auto;
		align-items: center;
	}
	:global([data-appearance='prism']) .address-masthead {
		grid-column: 1/-1;
	}
	:global([data-appearance='prism']) .address-facts {
		grid-column: 1/-1;
	}
	:global([data-appearance='atelier']) .address-masthead {
		background: transparent;
		border: 0;
		border-block: 3px double var(--hairline);
		border-radius: 0;
		padding-inline: 0;
	}
	:global([data-appearance='aurora']) .address-masthead {
		border-top-width: 1px;
		border-inline-start: 3px solid var(--accent-ink);
	}
	:global([data-appearance='aurora']) .address-title h1 {
		font-weight: 400;
	}
	@media (min-width: 900px) {
		:global([data-appearance='atelier']) .address-head {
			display: grid;
			grid-template-columns: minmax(0, 1.5fr) minmax(0, 1fr);
			gap: var(--density-gap, 16px);
			align-items: center;
		}
		:global([data-appearance='atelier']) .address-facts {
			display: grid;
			grid-template-columns: 1fr 1fr;
			border: 0;
			border-inline-start: 3px double var(--hairline);
		}
	}
	@media (max-width: 700px) {
		.address-masthead {
			padding: 10px 12px;
			grid-template-columns: minmax(0, 1fr) auto;
			gap: 4px 8px;
		}
		.address-title {
			gap: 6px 12px;
		}
		.address-title h1 {
			font-size: 24px;
		}
		.address-balance {
			font-size: 21px;
		}
		.address-facts {
			padding: 0;
			gap: 0 10px;
			justify-content: space-between;
		}
		.address-facts > div {
			gap: 4px;
			min-height: 44px;
		}
		.address-facts dd {
			font-size: 11px;
		}
		.address-facts a {
			display: inline-flex;
			align-items: center;
			min-height: 44px;
		}
		:global([data-appearance='prism']) .address-head {
			display: flex;
			align-items: stretch;
		}
		.address-tools :global(.save-trigger) {
			min-height: 44px;
			padding-inline: 10px;
		}
		.address-tools {
			grid-column: 2;
			grid-row: 1;
		}
	}
	.muted {
		color: var(--fg-muted);
	}
	.sum {
		margin-top: var(--space-3);
		color: var(--fg-muted);
		font-size: var(--fs-data);
	}
	/* A minted name is prose; only ids keep the mono face. */
	.token-name {
		font-weight: 500;
	}
</style>
