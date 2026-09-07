<script lang="ts">
	import { goto } from '$app/navigation';
	import { page } from '$app/state';
	import Panel from '$lib/components/Panel.svelte';
	import PageHead from '$lib/components/PageHead.svelte';
	import Facts from '$lib/components/Facts.svelte';
	import Fact from '$lib/components/Fact.svelte';
	import Tabs from '$lib/components/Tabs.svelte';
	import TokenBadge from '$lib/components/TokenBadge.svelte';
	import InfiniteList from '$lib/components/InfiniteList.svelte';
	import BoxCard from '$lib/components/BoxCard.svelte';
	import Hash from '$lib/components/Hash.svelte';
	import EmptyState from '$lib/components/EmptyState.svelte';
	import Skeleton from '$lib/components/Skeleton.svelte';
	import { api } from '$lib/api/endpoints';
	import { createPager, type Pager } from '$lib/pager/pager.svelte';
	import { status } from '$lib/status/status.svelte';
	import { formatTokenAmount } from '$lib/format/amount';
	import { truncateMiddle } from '$lib/format/hash';
	import { formatSharePct, shareBarWidth, tokenDisplayName } from '$lib/token/kind';
	import type { BoxDto, TokenHolderDto } from '$lib/api/types';
	import type { PageData } from './$types';

	const PAGE_SIZE = 50;

	const TABS = [
		{ id: 'holders', label: 'Holders' },
		{ id: 'boxes', label: 'Boxes' }
	];
	const TAB_IDS = TABS.map((t) => t.id);

	let { data }: { data: PageData } = $props();

	const id = $derived(data.id);
	const token = $derived(data.token);
	const tip = $derived(status.current?.indexed ?? null);
	const name = $derived(token ? tokenDisplayName(token.name, token.id) : truncateMiddle(id));

	/** Active tab, driven by the URL hash so a tab is linkable and survives reload. */
	const active = $derived.by(() => {
		const hash = page.url.hash.replace(/^#/, '');
		return TAB_IDS.includes(hash) ? hash : 'holders';
	});

	function selectTab(tab: string) {
		void goto(`#${tab}`, { replaceState: true, noScroll: true, keepFocus: true });
	}

	// Each tab pays for itself: the holders page is fetched on arrival, the boxes page only
	// once somebody opens that tab.
	let holderPager = $state<Pager<TokenHolderDto> | null>(null);
	let boxPager = $state<Pager<BoxDto> | null>(null);

	/** Boxes tab filter. Unspent first: "who holds it now" is the question being asked. */
	let unspentOnly = $state(true);

	// Navigating from one token to another reuses this component, so the lazily-created
	// per-tab state has to be dropped explicitly or the new token shows the old one's rows.
	// Plain `let` (not `$state`) so reading it here does not make this effect depend on it.
	let loadedFor: string | null = null;

	$effect(() => {
		if (id === loadedFor) return;
		loadedFor = id;
		holderPager = null;
		boxPager = null;
		unspentOnly = true;
	});

	$effect(() => {
		if (token === null) return;
		if (active === 'holders' && !holderPager) {
			const p = createPager<TokenHolderDto>((c) => api.tokenHolders(id, c, PAGE_SIZE));
			holderPager = p;
			void p.loadMore();
		} else if (active === 'boxes' && !boxPager) {
			startBoxes();
		}
	});

	function startBoxes() {
		const p = createPager<BoxDto>((c) => api.tokenBoxes(id, unspentOnly, c, PAGE_SIZE));
		boxPager = p;
		void p.loadMore();
	}

	function selectUnspent(next: boolean) {
		if (next === unspentOnly) return;
		unspentOnly = next;
		// A different filter is a different query: start a fresh pager rather than appending
		// the other set's rows to the ones already on screen.
		startBoxes();
	}

	const description = $derived(token?.description.trim() ?? '');
	// Read outside the markup: the `{#if token === null}` narrowing does not reach inside the
	// snippets InfiniteList renders, and every amount on the page needs the token's decimals.
	const decimals = $derived(token?.decimals ?? null);
	const boxCount = $derived(token?.box_count ?? 0);
</script>

<svelte:head>
	<title>{token ? `${name} — token` : 'Token not found'} — Ergo Explorer</title>
</svelte:head>

{#if token === null}
	<div class="head">
		<PageHead title="Token" {id} />
		<EmptyState
			message="Token not found — no minting box for this id in the index. Check the id, or come back once the indexer has reached the block that minted it."
		/>
	</div>
{:else}
	<div class="head">
		<PageHead title={name} id={token.id}>
			{#snippet aside()}
				<TokenBadge kind={token.kind} title={token.token_type ?? undefined} />
			{/snippet}
		</PageHead>

		<Facts>
			<Fact label="Supply">
				<span class="mono">{formatTokenAmount(token.supply, token.decimals)}</span>
			</Fact>
			<Fact label="Emission">
				<span class="mono">{formatTokenAmount(token.emission, token.decimals)}</span>
			</Fact>
			<Fact label="Burned">
				<span class="mono">{formatTokenAmount(token.burned, token.decimals)}</span>
			</Fact>
			<Fact label="Decimals">
				{#if token.decimals === null}
					<span class="muted" title="No decimals declared in the minting box">—</span>
				{:else}
					<span class="mono">{token.decimals}</span>
				{/if}
			</Fact>
			<Fact label="Holders">
				<span class="mono">{token.holder_count.toLocaleString('en-US')}</span>
			</Fact>
			<Fact label="Boxes">
				<span class="mono">{token.box_count.toLocaleString('en-US')}</span>
			</Fact>
			<Fact label="Minted in tx">
				<Hash value={token.mint_tx} href={`/tx/${token.mint_tx}`} copy={false} head={12} />
			</Fact>
			<Fact label="Minting box">
				<Hash value={token.mint_box} href={`/box/${token.mint_box}`} copy={false} head={12} />
			</Fact>
			<Fact label="Minted at height">
				<a class="mono" href={`/blocks/${token.mint_height}`}>{token.mint_height}</a>
			</Fact>
			{#if description}
				<Fact label="Description"><span class="desc">{description}</span></Fact>
			{/if}
		</Facts>
	</div>

	<Panel>
		<Tabs tabs={TABS} {active} onchange={selectTab} label="Token sections" />

		<div role="tabpanel" id={`panel-${active}`} tabindex="0" aria-labelledby={`tab-${active}`}>
			{#if active === 'holders'}
				{#if holderPager}
					<InfiniteList
						table
						columns={4}
						dense
						pager={holderPager}
						empty="Nobody holds this token — every unit of it has been burned or is unaccounted for in the indexed range."
					>
						{#snippet head()}
							<tr>
								<th class="num">Rank</th>
								<th>Holder</th>
								<th class="num">Amount</th>
								<th class="share-col">Share</th>
							</tr>
						{/snippet}
						{#snippet children(holder: TokenHolderDto, i: number)}
							<tr>
								<td class="num mono">{i + 1}</td>
								<td>
									{#if holder.address}
										<Hash value={holder.address} href={`/address/${holder.address}`} copy={false} />
									{:else}
										<span class="muted" title="No P2PK/P2S address — showing the ergo tree hash">
											<Hash value={holder.tree_hash} copy={false} />
										</span>
									{/if}
								</td>
								<td class="num mono">{formatTokenAmount(holder.amount, decimals)}</td>
								<td class="share-col">
									<span class="share">
										<span class="track" aria-hidden="true">
											<span class="fill" style:width={`${shareBarWidth(holder.share_pct)}%`}></span>
										</span>
										<span class="mono pct" title={`${holder.share_pct}%`}
											>{formatSharePct(holder.share_pct)}</span
										>
									</span>
								</td>
							</tr>
						{/snippet}
					</InfiniteList>
				{:else}
					<Skeleton />
				{/if}
			{:else if boxPager}
				<div class="controls">
					<div class="seg" role="group" aria-label="Filter boxes">
						<button
							type="button"
							class="seg-btn"
							aria-pressed={unspentOnly}
							onclick={() => selectUnspent(true)}>Unspent</button
						>
						<button
							type="button"
							class="seg-btn"
							aria-pressed={!unspentOnly}
							onclick={() => selectUnspent(false)}>All</button
						>
					</div>
					<span class="muted count">
						{unspentOnly
							? 'Boxes still holding the token'
							: `${boxCount.toLocaleString('en-US')} boxes ever`}
					</span>
				</div>

				<div class="boxes">
					<InfiniteList
						pager={boxPager}
						empty={unspentOnly
							? 'No unspent box carries this token.'
							: 'No box in the indexed range carries this token.'}
					>
						{#snippet children(box: BoxDto)}
							<BoxCard {box} {tip} role="output" />
						{/snippet}
					</InfiniteList>
				</div>
			{:else}
				<Skeleton />
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

	.muted {
		color: var(--fg-muted);
	}

	/* A minted description is prose, so it reads from the left and wraps, unlike the figures
	   the rest of the facts grid holds. */
	.desc {
		display: block;
		text-align: left;
		max-width: 52ch;
		color: var(--fg-muted);
	}

	/* The share column is the only cell allowed to stretch: the bar is the point of it. */
	.share-col {
		width: 200px;
	}

	.share {
		display: flex;
		align-items: center;
		gap: var(--space-2);
	}

	.track {
		flex: 1;
		min-width: 40px;
		height: 4px;
		border-radius: var(--radius-pill);
		background: var(--bg-sunk);
		overflow: hidden;
	}

	.fill {
		display: block;
		height: 100%;
		border-radius: var(--radius-pill);
		background: var(--accent);
	}

	.pct {
		width: 7ch;
		text-align: right;
		color: var(--fg-muted);
	}

	.controls {
		display: flex;
		align-items: center;
		gap: var(--space-3);
		flex-wrap: wrap;
		padding: var(--space-4) 0 var(--space-2);
		font-size: var(--fs-data);
	}

	.boxes :global(.list) {
		display: flex;
		flex-direction: column;
		gap: var(--space-3);
		padding-bottom: var(--space-2);
	}
</style>
