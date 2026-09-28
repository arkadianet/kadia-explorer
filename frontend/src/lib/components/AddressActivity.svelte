<script lang="ts">
	import { untrack } from 'svelte';
	import { goto } from '$app/navigation';
	import { page } from '$app/state';
	import { api } from '$lib/api/endpoints';
	import { ApiError } from '$lib/api/client';
	import type { AddressActivityDto, AddressActivityPageDto } from '$lib/api/types';
	import {
		activityCsv,
		DIRECTIONS,
		MAX_ACTIVITY_ITEMS,
		parseActivityFilters
	} from '$lib/addresses/activity';
	import { savedAddresses } from '$lib/addresses/saved.svelte';
	import { formatErg, formatTokenAmount } from '$lib/format/amount';
	import { truncateMiddle } from '$lib/format/hash';
	import Hash from './Hash.svelte';
	let { address }: { address: string } = $props();
	const queryKey = $derived(
		JSON.stringify(
			['asset', 'direction', 'from', 'to'].map((key) => page.url.searchParams.get(key))
		)
	);
	const parsed = $derived(parseActivityFilters(page.url.searchParams));
	let asset = $state('all');
	let tokenId = $state('');
	let direction = $state('all');
	let from = $state('');
	let to = $state('');
	let formError = $state<string | null>(null);
	let items = $state<AddressActivityDto[]>([]);
	let loading = $state(false);
	let failure = $state<string | null>(null);
	let restartRequired = $state(false);
	let loaded = $state(false);
	let cursor = $state<string | null>(null);
	let snapshot: string | undefined;
	let metadata = $state<AddressActivityPageDto | null>(null);
	let scanned = $state(0);
	let generation = 0;
	let exportMessage = $state('');
	const atLimit = $derived(items.length >= MAX_ACTIVITY_ITEMS);
	$effect(() => {
		void address;
		void queryKey;
		untrack(() => {
			const current = page.url.searchParams.get('asset') || 'all';
			asset = current === 'all' || current === 'erg' ? current : 'token';
			tokenId = asset === 'token' ? current : '';
			direction = page.url.searchParams.get('direction') || 'all';
			from = page.url.searchParams.get('from') || '';
			to = page.url.searchParams.get('to') || '';
			formError = null;
			reset();
			if (!parsed.error) void loadMore();
		});
		return () => {
			generation++;
		};
	});
	function reset() {
		generation++;
		items = [];
		cursor = null;
		snapshot = undefined;
		metadata = null;
		scanned = 0;
		failure = null;
		restartRequired = false;
		loaded = false;
		loading = false;
		exportMessage = '';
	}
	async function loadMore() {
		if (loading || restartRequired || atLimit || (loaded && cursor === null) || parsed.error)
			return;
		const current = generation;
		const requestedAddress = address;
		const filters = parsed.filters;
		loading = true;
		failure = null;
		try {
			const result = await api.addressActivity(
				requestedAddress,
				filters,
				cursor ?? undefined,
				snapshot,
				Math.min(20, MAX_ACTIVITY_ITEMS - items.length)
			);
			if (current !== generation) return;
			items = [...items, ...result.items];
			cursor = result.next_cursor;
			snapshot = result.next_snapshot ?? undefined;
			metadata = result;
			scanned += result.scanned;
			loaded = true;
		} catch (error) {
			if (current !== generation) return;
			if (error instanceof ApiError && error.status === 409) {
				items = [];
				metadata = null;
				cursor = null;
				snapshot = undefined;
				restartRequired = true;
				failure = 'The indexed snapshot changed. Refresh activity to start a consistent view.';
			} else
				failure =
					error instanceof ApiError && error.status === 404
						? 'Address activity is not available on this server yet. The Transactions and box tabs remain available.'
						: error instanceof Error
							? error.message
							: 'Activity could not be loaded.';
		} finally {
			if (current === generation) loading = false;
		}
	}
	function submit(event: SubmitEvent) {
		event.preventDefault();
		// A one-shot navigation payload, never reactive component state.
		// eslint-disable-next-line svelte/prefer-svelte-reactivity
		const params = new URLSearchParams(page.url.searchParams);
		for (const [key, value] of Object.entries({
			asset: asset === 'token' ? tokenId.trim() : asset,
			direction,
			from,
			to
		})) {
			if (value && value !== 'all') params.set(key, value);
			else params.delete(key);
		}
		const checked = parseActivityFilters(params);
		if (asset === 'token' && !tokenId.trim()) formError = 'Enter the full token ID.';
		else formError = checked.error;
		if (!formError)
			void goto(`${page.url.pathname}${params.size ? `?${params}` : ''}#activity`, {
				noScroll: true,
				keepFocus: true
			});
	}
	function refresh() {
		reset();
		void loadMore();
	}
	function signed(value: string, decimals = 9) {
		const amount = decimals === 9 ? formatErg(value) : formatTokenAmount(value, decimals);
		return `${BigInt(value) > 0n ? '+' : ''}${amount}`;
	}
	function exportLoaded() {
		if (!metadata?.anchor || loading || restartRequired) return;
		try {
			const label = savedAddresses.items.find((item) => item.address === address)?.label ?? '';
			const csv = activityCsv(items, address, label, metadata.anchor, parsed.filters, {
				partialFrom: metadata.partial_from,
				matchingScanComplete: cursor === null
			});
			const url = URL.createObjectURL(new Blob([csv], { type: 'text/csv;charset=utf-8' }));
			const link = document.createElement('a');
			link.href = url;
			link.download = `kadia-address-activity-${metadata.anchor.height}.csv`;
			link.click();
			setTimeout(() => URL.revokeObjectURL(url), 1000);
			exportMessage = `Exported ${items.length} loaded transactions at indexed block ${metadata.anchor.height.toLocaleString('en-US')}.`;
		} catch (error) {
			exportMessage = error instanceof Error ? error.message : 'Export failed.';
		}
	}
</script>

<section class="address-activity" aria-label="Address activity">
	<header class="activity-heading">
		<div>
			<p class="eyebrow">Address activity</p>
			<h2>Every change, in context.</h2>
			<p>
				Net changes for this locking script, including returned change. Ownership and fee payer are
				not inferred.
			</p>
		</div>
		<button type="button" class="refresh" onclick={refresh} disabled={loading || !!parsed.error}
			>Refresh activity</button
		>
	</header>
	<form class="activity-filters" onsubmit={submit}>
		<label
			>Asset<select class="control" bind:value={asset}
				><option value="all">All assets</option><option value="erg">ERG</option><option
					value="token">Token ID</option
				></select
			></label
		>
		{#if asset === 'token'}<label class="token-filter"
				>Token ID<input
					class="control mono"
					bind:value={tokenId}
					maxlength="64"
					placeholder="64 hexadecimal characters"
					spellcheck="false"
				/></label
			>{/if}
		<label
			>Direction<select class="control" bind:value={direction}
				>{#each DIRECTIONS as value (value)}<option {value}
						>{value === 'all' ? 'All directions' : value[0].toUpperCase() + value.slice(1)}</option
					>{/each}</select
			></label
		>
		<label>From (UTC)<input class="control" type="date" bind:value={from} /></label><label
			>Through (UTC)<input class="control" type="date" bind:value={to} /></label
		>
		<button type="submit" class="btn btn-fill">Apply filters</button>
	</form>
	{#if formError || parsed.error}<p class="notice error" role="alert">
			{formError || parsed.error}
		</p>{/if}
	<p class="filter-help">
		Dates include the selected UTC days. Direction refers to the selected asset; mixed means some
		assets rose while others fell.
	</p>
	{#if metadata}<div class="activity-snapshot">
			<p>
				{#if metadata.anchor}Snapshot <a href={`/blocks/${metadata.anchor.height}`}
						>block {metadata.anchor.height.toLocaleString('en-US')}</a
					>{:else}No indexed block snapshot{/if} · {items.length} transactions loaded · {scanned} candidates
				scanned
			</p>
			{#if metadata.partial_from !== null}<p class="notice">
					Indexed history starts at block {metadata.partial_from.toLocaleString('en-US')}; earlier
					activity is outside this view. Unresolved older inputs can also hide later activity for
					this address.
				</p>{/if}
		</div>{/if}
	{#if failure}<div class="notice error" role="alert">
			<p>{failure}</p>
			<button
				type="button"
				onclick={restartRequired ? refresh : () => void loadMore()}
				disabled={loading}
				>{restartRequired ? 'Restart from latest snapshot' : 'Retry activity'}</button
			>
		</div>{/if}
	<ol class="activity-list">
		{#each items as item (item.id)}<li class="activity-entry">
				<div class="activity-identity">
					<div class="entry-top">
						<span class="direction" class:uncertain={item.direction === 'unknown'}
							>{item.direction}</span
						><a class="receipt-link" href={`/tx/${item.id}?address=${encodeURIComponent(address)}`}
							>View receipt ↗</a
						>
					</div>
					<Hash
						value={item.id}
						href={`/tx/${item.id}?address=${encodeURIComponent(address)}`}
						copy={false}
						head={12}
					/>
					<p class="time">
						<time datetime={new Date(item.timestamp).toISOString()}
							>{new Date(item.timestamp)
								.toISOString()
								.replace('T', ' ')
								.replace('.000Z', ' UTC')}</time
						>
						· <a href={`/blocks/${item.height}`}>{item.height.toLocaleString('en-US')}</a>
					</p>
				</div>
				<div class="activity-changes">
					{#if !item.coverage.complete}<p class="unknown">Exact net changes unavailable</p>
						<p>
							{item.coverage.total_inputs - item.coverage.resolved_inputs} unresolved input{item
								.coverage.total_inputs -
								item.coverage.resolved_inputs ===
							1
								? ''
								: 's'}; an unresolved box may belong to this address.
						</p>{:else}<p
							class="erg-change"
							class:negative={item.erg_delta !== null && BigInt(item.erg_delta) < 0n}
						>
							{item.erg_delta === null ? 'Unknown' : signed(item.erg_delta)} <span>ERG</span>
						</p>
						{#if item.tokens.length}<ul class="token-changes">
								{#each item.tokens as token (token.id)}<li>
										<strong class:negative={token.delta !== null && BigInt(token.delta) < 0n}
											>{token.delta === null
												? 'Unknown'
												: `${BigInt(token.delta) > 0n ? '+' : ''}${formatTokenAmount(token.delta, token.decimals)}`}</strong
										>
										<a href={`/token/${token.id}`}
											><bdi>{token.name?.trim() || 'Unnamed token'}</bdi></a
										><small class="mono"
											>{truncateMiddle(token.id)}{token.decimals === null
												? ' · raw units; decimals unknown'
												: ''}</small
										>
									</li>{/each}
							</ul>{/if}{/if}{#if item.asset_match === 'uncertain'}<p class="notice">
							Token involvement is uncertain because some inputs are unresolved.
						</p>{/if}
				</div>
			</li>{/each}
	</ol>
	{#if loading}<p class="scan-state" role="status">
			Checking indexed address activity…
		</p>{:else if loaded && !failure}<p class="scan-state" role="status">
			{#if cursor === null}{items.length
					? 'End of the matching indexed activity.'
					: 'No matching activity in the indexed range.'}{:else if atLimit}Loaded the
				500-transaction view limit. Narrow the filters to inspect another range.{:else if metadata?.scan_limit_reached}This
				scan reached its candidate limit. More history remains to be checked.{:else}More indexed
				history is available.{/if}
		</p>{/if}
	{#if loaded && cursor !== null && !atLimit && !restartRequired && !failure}<button
			class="load-activity"
			type="button"
			onclick={() => void loadMore()}
			disabled={loading}
			>{metadata?.scan_limit_reached ? 'Continue scanning' : 'Load more activity'}</button
		>{/if}
	<div class="export">
		<div>
			<h3>Export this view</h3>
			<p>
				Only the {items.length} loaded transactions at this snapshot, with their asset changes and applied
				filters. Maximum 500 transactions, 5,000 asset rows and 2 MB. Unresolved deltas stay blank.
			</p>
			<p>
				Raw values are exact integers. Import raw-value columns as text in spreadsheets to retain
				every digit.
			</p>
		</div>
		<button
			type="button"
			class="btn btn-ghost"
			onclick={exportLoaded}
			disabled={!items.length || !metadata?.anchor || loading || !!failure}
			>Export loaded CSV</button
		>
	</div>
	<p role="status" class="export-message">{exportMessage}</p>
</section>

<style>
	.address-activity {
		min-width: 0;
	}
	.activity-heading {
		display: flex;
		align-items: start;
		justify-content: space-between;
		gap: 20px;
		margin-bottom: 20px;
	}
	.eyebrow {
		font: 11px var(--font-mono);
		text-transform: uppercase;
		letter-spacing: 0.08em;
		color: var(--accent-ink);
		margin-bottom: 8px;
	}
	h2 {
		font: var(--weight-display, 750) clamp(26px, 3vw, 38px)/1.1
			var(--font-display, var(--font-sans));
		letter-spacing: -0.04em;
		margin-bottom: 12px;
	}
	.activity-heading p:not(.eyebrow) {
		font-size: 12px;
		color: var(--fg-muted);
		max-width: 65ch;
	}
	.refresh,
	.load-activity,
	.notice button {
		border: var(--rule);
		border-radius: 8px;
		padding: 9px 13px;
		background: var(--surface-solid);
		color: var(--fg);
		font-size: 12px;
		cursor: pointer;
		white-space: nowrap;
	}
	button:disabled {
		opacity: 0.6;
		cursor: default;
	}
	.activity-filters {
		display: flex;
		flex-wrap: wrap;
		align-items: end;
		gap: 12px;
		padding: 16px;
		background: var(--surface-hover);
		border: var(--rule);
		border-radius: 12px;
	}
	.activity-filters label {
		display: grid;
		gap: 6px;
		min-width: 0;
		flex: 1 1 125px;
		font-size: 11px;
		color: var(--fg-muted);
	}
	.activity-filters .control {
		width: 100%;
		min-width: 0;
		height: 38px;
	}
	.activity-filters .token-filter {
		flex-basis: 260px;
	}
	.activity-filters .btn {
		height: 38px;
		font-size: 12px;
		padding: 0 14px;
	}
	.filter-help,
	.activity-snapshot,
	.scan-state,
	.export-message {
		font-size: 12px;
		color: var(--fg-muted);
		margin: 14px 0;
		line-height: 1.6;
	}
	.activity-snapshot {
		padding-bottom: 14px;
		border-bottom: var(--rule);
	}
	.notice {
		font-size: 12px;
		color: var(--warn-ink);
		margin: 10px 0;
		line-height: 1.65;
	}
	.error {
		color: var(--danger-ink);
		padding: 14px;
		border-left: 3px solid var(--danger-ink);
		background: var(--surface-hover);
	}
	.notice button {
		margin-top: 12px;
	}
	.activity-entry {
		display: grid;
		grid-template-columns: minmax(0, 1.1fr) minmax(0, 1fr);
		gap: 20px;
		padding: 22px 0;
		border-bottom: var(--rule);
		min-width: 0;
	}
	.entry-top {
		display: flex;
		gap: 12px;
		align-items: center;
		justify-content: space-between;
		margin-bottom: 12px;
		flex-wrap: wrap;
	}
	.direction {
		font: 11px var(--font-mono);
		text-transform: uppercase;
		background: var(--accent-wash);
		color: var(--accent-ink);
		padding: 5px 8px;
		border-radius: 5px;
	}
	.direction.uncertain {
		color: var(--warn-ink);
		background: color-mix(in srgb, var(--warn-ink), transparent 90%);
	}
	.receipt-link {
		font-size: 12px;
		color: var(--accent-ink);
	}
	.activity-identity {
		min-width: 0;
		font-size: 12px;
	}
	.activity-identity :global(.hash) {
		overflow-wrap: anywhere;
		max-width: 100%;
	}
	.time {
		font-size: 11px;
		color: var(--fg-muted);
		margin-top: 10px;
	}
	.activity-changes {
		min-width: 0;
	}
	.erg-change {
		font: 500 25px var(--font-number, var(--font-sans));
		color: var(--accent-ink);
		overflow-wrap: anywhere;
	}
	.erg-change span {
		font: 12px var(--font-sans);
		color: var(--fg-muted);
	}
	.negative {
		color: var(--danger-ink) !important;
	}
	.token-changes {
		display: grid;
		gap: 10px;
		margin-top: 12px;
		font-size: 13px;
		overflow-wrap: anywhere;
	}
	.token-changes small {
		display: block;
		font-size: 11px;
		color: var(--fg-muted);
		margin-top: 4px;
	}
	.token-changes strong {
		margin-inline-end: 0.4em;
	}
	.unknown {
		font-weight: 650;
		color: var(--warn-ink);
		margin-bottom: 8px;
	}
	.unknown + p {
		font-size: 12px;
		color: var(--fg-muted);
	}
	.export {
		display: flex;
		justify-content: space-between;
		align-items: center;
		flex-wrap: wrap;
		gap: 20px;
		margin-top: 24px;
		padding: 20px;
		border: var(--rule);
		border-radius: 12px;
		background: var(--surface-hover);
	}
	.export > div {
		flex: 1 1 300px;
	}
	.export h3 {
		font-size: 16px;
		margin-bottom: 10px;
	}
	.export p {
		font-size: 11px;
		color: var(--fg-muted);
		line-height: 1.7;
	}
	.export p + p {
		margin-top: 6px;
	}
	.export .btn {
		font-size: 12px;
	}
	.load-activity {
		margin-top: 4px;
	}
	:global(:root[data-appearance='prism']) .activity-entry {
		margin-top: 14px;
		padding: 20px;
		border: var(--rule);
		border-radius: 12px;
		box-shadow: var(--shadow-rest);
		background: var(--surface-solid);
	}
	:global(:root[data-appearance='prism']) .activity-changes {
		border-left: var(--rule);
		padding-left: 20px;
	}
	:global(:root[data-appearance='atelier']) .activity-heading {
		border-bottom: 3px double var(--fg);
		padding-bottom: 18px;
	}
	:global(:root[data-appearance='atelier']) .activity-filters {
		background: none;
		border-radius: 0;
		border-width: 0 0 1px;
		padding-inline: 0;
	}
	:global(:root[data-appearance='atelier']) .activity-entry {
		grid-template-columns: minmax(0, 1fr) minmax(0, 0.65fr);
		padding-block: 24px;
	}
	:global(:root[data-appearance='atelier']) .activity-changes {
		text-align: right;
	}
	:global(:root[data-appearance='atelier']) .export {
		border-radius: 0;
		background: none;
	}
	:global(:root[data-appearance='aurora']) .activity-heading {
		display: block;
		text-align: center;
		padding: 18px 10px;
	}
	:global(:root[data-appearance='aurora']) .activity-heading p {
		margin-inline: auto;
	}
	:global(:root[data-appearance='aurora']) .refresh {
		margin-top: 16px;
	}
	:global(:root[data-appearance='aurora']) .activity-list {
		display: grid;
		grid-template-columns: repeat(2, minmax(0, 1fr));
		gap: 20px;
		margin-top: 20px;
	}
	:global(:root[data-appearance='aurora']) .activity-entry {
		display: block;
		padding: 26px;
		border: var(--rule);
		border-radius: 24px;
		box-shadow: var(--shadow-lift);
	}
	:global(:root[data-appearance='aurora']) .activity-changes {
		margin-top: 22px;
	}
	:global(:root[data-appearance='aurora']) .activity-filters {
		border-radius: 20px;
		padding: 22px;
	}
	@media (max-width: 850px) {
		.activity-heading {
			display: block;
		}
		.refresh {
			margin-top: 14px;
		}
		.activity-entry,
		:global(:root[data-appearance='atelier']) .activity-entry {
			grid-template-columns: minmax(0, 1fr);
		}
		:global(:root[data-appearance='prism']) .activity-changes {
			border-left: 0;
			border-top: var(--rule);
			padding: 16px 0 0;
		}
		:global(:root[data-appearance='atelier']) .activity-changes {
			text-align: left;
		}
		:global(:root[data-appearance='aurora']) .activity-list {
			grid-template-columns: minmax(0, 1fr);
		}
	}
	@media (max-width: 400px) {
		.activity-filters {
			padding: 12px;
		}
		.activity-filters label {
			flex-basis: 100%;
		}
		.activity-filters .btn {
			width: 100%;
			justify-content: center;
		}
		.export {
			padding: 14px;
		}
		.export .btn {
			width: 100%;
			justify-content: center;
		}
		:global(:root[data-appearance='prism']) .activity-entry,
		:global(:root[data-appearance='aurora']) .activity-entry {
			padding: 16px;
		}
	}
</style>
