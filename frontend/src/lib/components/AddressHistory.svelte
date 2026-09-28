<script lang="ts">
	import { onDestroy, tick, untrack } from 'svelte';
	import { goto } from '$app/navigation';
	import { page } from '$app/state';
	import { status } from '$lib/status/status.svelte';
	import {
		createHistoryInspector,
		historySelector,
		sumHistoryBoxes,
		createHistoryComparison,
		comparisonSelectors,
		historyComparisonLink
	} from '$lib/addresses/history';
	import { formatErg, formatNano } from '$lib/format/amount';
	import Hash from './Hash.svelte';
	let { address }: { address: string } = $props();
	const uid = $props.id();
	let inspector = $state<ReturnType<typeof createHistoryInspector>['state']>({
		started: false,
		busy: false,
		balance: null,
		anchor: null,
		boxes: [],
		boxesLoaded: false,
		nextCursor: null,
		issue: null,
		balanceIssue: null,
		restartRequired: false,
		viewLimit: false
	});
	const controller = createHistoryInspector((value) => (inspector = value));
	let comparison = $state<ReturnType<typeof createHistoryComparison>['state']>({
		busy: false,
		result: null,
		changes: null,
		issue: null
	});
	const compareController = createHistoryComparison((value) => (comparison = value));
	let mode = $state<'snapshot' | 'compare'>('snapshot');
	let fromHeight = $state('0');
	let toHeight = $state('');
	const urlKey = $derived(
		JSON.stringify(
			['at_height', 'at_block', 'from_height', 'to_height', 'from_block', 'to_block'].map((key) =>
				page.url.searchParams.get(key)
			)
		)
	);
	const requested = $derived(
		historySelector(page.url.searchParams.get('at_height'), page.url.searchParams.get('at_block'))
	);
	let height = $state('');
	let formError = $state<string | null>(null);
	let tokenFilter = $state('');
	let tokenLimit = $state(50);
	let expanded = $state<Record<string, boolean>>({});
	$effect(() => {
		void address;
		void urlKey;
		untrack(() => {
			controller.reset();
			compareController.reset();
			mode =
				page.url.searchParams.has('from_height') ||
				page.url.searchParams.has('to_height') ||
				page.url.searchParams.has('from_block') ||
				page.url.searchParams.has('to_block')
					? 'compare'
					: 'snapshot';
			fromHeight = page.url.searchParams.get('from_height') ?? '0';
			toHeight = page.url.searchParams.get('to_height') ?? String(status.current?.indexed ?? '');
			height = page.url.searchParams.get('at_height') ?? String(status.current?.indexed ?? '');
			formError = null;
			tokenFilter = '';
			tokenLimit = 50;
			expanded = {};
		});
	});
	onDestroy(() => {
		controller.stop();
		compareController.stop();
	});
	const comparisonTokens = $derived(
		comparison.changes?.tokens.filter((token) =>
			token.token_id.includes(tokenFilter.trim().toLowerCase())
		) ?? []
	);
	const comparisonLink = $derived(
		comparison.result ? historyComparisonLink(address, comparison.result) : ''
	);
	function selectMode(next: 'snapshot' | 'compare') {
		controller.reset();
		compareController.reset();
		mode = next;
		formError = null;
		tokenFilter = '';
		tokenLimit = 50;
	}
	async function compare(unpin = false) {
		const selected = comparisonSelectors(
			fromHeight.trim(),
			toHeight.trim(),
			!unpin && fromHeight === page.url.searchParams.get('from_height')
				? page.url.searchParams.get('from_block')
				: null,
			!unpin && toHeight === page.url.searchParams.get('to_height')
				? page.url.searchParams.get('to_block')
				: null
		);
		formError = selected.error;
		if (!selected.value) return;
		const selectedAddress = address;
		const { from, to } = selected.value;
		// eslint-disable-next-line svelte/prefer-svelte-reactivity
		const params = new URLSearchParams(page.url.searchParams);
		params.delete('at_height');
		params.delete('at_block');
		params.set('from_height', String(from.height));
		params.set('to_height', String(to.height));
		if (from.block_id) params.set('from_block', from.block_id);
		else params.delete('from_block');
		if (to.block_id) params.set('to_block', to.block_id);
		else params.delete('to_block');
		await goto(`${page.url.pathname}?${params}#history`, { noScroll: true, keepFocus: true });
		await tick();
		if (
			address === selectedAddress &&
			mode === 'compare' &&
			page.url.searchParams.get('from_height') === String(from.height) &&
			page.url.searchParams.get('to_height') === String(to.height) &&
			page.url.searchParams.get('from_block') === (from.block_id ?? null) &&
			page.url.searchParams.get('to_block') === (to.block_id ?? null)
		)
			void compareController.start(selectedAddress, from, to);
	}
	const allBoxes = $derived(
		inspector.boxesLoaded && inspector.nextCursor === null && !inspector.viewLimit
	);
	const loaded = $derived(sumHistoryBoxes(inspector.boxes));
	const total = $derived(inspector.balance?.balance ?? loaded);
	const exact = $derived(inspector.balance !== null || allBoxes);
	const shownTokens = $derived(
		total.tokens.filter((token) => token.token_id.includes(tokenFilter.trim().toLowerCase()))
	);
	const anchorLink = $derived.by(() => {
		if (!inspector.anchor) return '';
		// A one-shot link payload, not reactive state.
		// eslint-disable-next-line svelte/prefer-svelte-reactivity
		const params = new URLSearchParams();
		params.set('at_height', String(inspector.anchor.height));
		if (inspector.anchor.block_id) params.set('at_block', inspector.anchor.block_id);
		return `/address/${encodeURIComponent(address)}?${params}#history`;
	});
	const apiLink = $derived.by(() => {
		if (!inspector.anchor) return '';
		// eslint-disable-next-line svelte/prefer-svelte-reactivity
		const params = new URLSearchParams({ height: String(inspector.anchor.height), limit: '20' });
		if (inspector.anchor.block_id) params.set('block_id', inspector.anchor.block_id);
		if (inspector.nextCursor) params.set('cursor', inspector.nextCursor);
		return `/v1/addresses/${encodeURIComponent(address)}/boxes/at?${params}`;
	});
	async function inspect(unpin = false) {
		const pin =
			!unpin && height === page.url.searchParams.get('at_height')
				? page.url.searchParams.get('at_block')
				: null;
		const parsed = historySelector(height.trim(), pin);
		formError = parsed.error;
		if (!parsed.value) return;
		const selectedAddress = address;
		// The URL is shareable, but loading it never starts this potentially expensive read.
		// eslint-disable-next-line svelte/prefer-svelte-reactivity
		const params = new URLSearchParams(page.url.searchParams);
		for (const key of ['from_height', 'to_height', 'from_block', 'to_block']) params.delete(key);
		params.set('at_height', String(parsed.value.height));
		if (parsed.value.block_id) params.set('at_block', parsed.value.block_id);
		else params.delete('at_block');
		await goto(`${page.url.pathname}?${params}#history`, { noScroll: true, keepFocus: true });
		await tick();
		if (
			address === selectedAddress &&
			page.url.searchParams.get('at_height') === String(parsed.value.height)
		)
			void controller.start(selectedAddress, parsed.value);
	}
</script>

<section class="address-history" aria-label="Historical address snapshot">
	<header class="history-heading">
		<div>
			<p class="eyebrow">Historical snapshot</p>
			<h2>A balance at a block.</h2>
			<p>
				Inspect the ERG, raw token quantities and boxes held by this locking script after a chosen
				block. No wallet ownership is implied.
			</p>
		</div>
		<span class="history-tag">End-of-block state</span>
	</header>
	<div class="history-mode" role="group" aria-label="Historical view">
		<button type="button" aria-pressed={mode === 'snapshot'} onclick={() => selectMode('snapshot')}
			>Single snapshot</button
		>
		<button type="button" aria-pressed={mode === 'compare'} onclick={() => selectMode('compare')}
			>Compare two blocks</button
		>
	</div>
	{#if mode === 'snapshot'}
		<form
			class="history-controls"
			onsubmit={(event) => {
				event.preventDefault();
				void inspect();
			}}
		>
			<label for={`${uid}-height`}
				>Block height<input
					id={`${uid}-height`}
					class="control mono"
					type="text"
					inputmode="numeric"
					maxlength="10"
					bind:value={height}
					placeholder="e.g. 1882687"
					aria-describedby={`${uid}-help`}
				/></label
			>
			<button type="submit" class="btn btn-fill" disabled={inspector.busy}
				>{inspector.busy ? 'Inspecting…' : 'Inspect snapshot'}</button
			>
			<p id={`${uid}-help`}>
				Height 0 is the pre-block genesis state.{#if status.current?.indexed != null}
					Latest reported indexed height: {status.current.indexed.toLocaleString('en-US')}.{/if}
			</p>
		</form>
		{#if formError || requested.error}<p class="history-error" role="alert">
				{formError || requested.error}
			</p>{/if}
		{#if !inspector.started}<div class="history-intro">
				<span class="intro-mark" aria-hidden="true">H</span>
				<div>
					<h3>Choose the moment to inspect.</h3>
					<p>
						Reads begin only when you select Inspect snapshot. A complete mainnet index is required;
						unavailable history is never shown as zero.
					</p>
					{#if requested.value?.block_id}<p>
							This link is pinned to block <span class="mono pinned-id"
								>{requested.value.block_id}</span
							>. A changed anchor will be reported.
						</p>{/if}
				</div>
			</div>{/if}
		{#if inspector.issue}<div class="history-error" role="alert">
				<p>{inspector.issue.message}</p>
				{#if inspector.restartRequired}<button type="button" onclick={() => void inspect(true)}
						>Restart at this height</button
					>{:else if !inspector.viewLimit}<button
						type="button"
						disabled={inspector.busy}
						onclick={() => (inspector.anchor ? void controller.more() : void inspect())}
						>Retry historical read</button
					>{/if}
			</div>{/if}
		{#if inspector.balanceIssue}<p class="history-notice">{inspector.balanceIssue.message}</p>{/if}
		{#if inspector.busy}<p role="status" class="history-progress">
				Reading the anchored historical state…
			</p>{/if}
		{#if inspector.anchor}
			<div class="history-anchor">
				<div>
					<span class="eyebrow"
						>{inspector.anchor.height === 0 ? 'Genesis snapshot' : 'Anchored snapshot'}</span
					>
					<h3>
						{inspector.anchor.height === 0
							? 'Before block 1'
							: `After block ${inspector.anchor.height.toLocaleString('en-US')}`}
					</h3>
					{#if inspector.anchor.block_id}<Hash
							value={inspector.anchor.block_id}
							href={`/blocks/${inspector.anchor.block_id}`}
							head={12}
						/>{:else}<p>Recognized mainnet genesis · no block ID</p>{/if}
				</div>
				<a class="permalink" href={anchorLink}>Pinned snapshot link ↗</a>
			</div>
			<div class="history-workspace">
				<section class="history-summary" aria-label="Historical balances">
					<p class="eyebrow">
						{exact ? 'Complete anchored balance' : 'Loaded boxes only · incomplete'}
					</p>
					<p class="history-total"><strong>{formatErg(total.nano)}</strong> <span>ERG</span></p>
					<p class="raw-nano mono">{total.nano} nanoERG</p>
					<dl class="history-counts">
						<div>
							<dt>Unspent boxes{exact ? '' : ' loaded'}</dt>
							<dd>{total.box_count.toLocaleString('en-US')}</dd>
						</div>
						<div>
							<dt>Token IDs{exact ? '' : ' loaded'}</dt>
							<dd>{total.tokens.length.toLocaleString('en-US')}</dd>
						</div>
					</dl>
					{#if exact && total.box_count === 0}<p class="history-empty">
							No boxes or balance at this anchor.
						</p>{/if}{#if !exact}<p class="history-notice">
							These are subtotals of the boxes loaded below, not the address’s full historical
							balance.
						</p>{/if}
					<p class="history-scope">
						Amounts use inclusion and spend heights. A box created and spent within the selected
						block is excluded. Mempool activity and historical prices are outside this snapshot.
					</p>
					{#if total.tokens.length}<div class="history-tokens">
							<h3>Raw token balances</h3>
							<p>
								Token names and decimal settings are not joined from today’s metadata. Quantities
								below are raw integer units.
							</p>
							{#if total.tokens.length > 10}<label for={`${uid}-token`}
									>Filter token IDs<input
										id={`${uid}-token`}
										class="control mono"
										type="search"
										bind:value={tokenFilter}
										oninput={() => (tokenLimit = 50)}
										placeholder="Token ID or prefix"
									/></label
								>{/if}
							<ul>
								{#each shownTokens.slice(0, tokenLimit) as token (token.token_id)}<li>
										<Hash
											value={token.token_id}
											href={`/token/${token.token_id}`}
											copy={false}
											head={10}
										/><strong>{formatNano(token.amount)}</strong><span>raw units</span>
									</li>{/each}
							</ul>
							{#if !shownTokens.length}<p>
									No token IDs match this filter.
								</p>{:else if shownTokens.length > tokenLimit}{#if tokenLimit < 200}<button
										type="button"
										onclick={() => (tokenLimit += 50)}>Show more token balances</button
									>{:else}<p>
										Showing 200 matches. Narrow the token ID filter to inspect another token.
									</p>{/if}{/if}
						</div>{/if}
				</section>
				<section class="history-boxes" aria-label="Historical unspent boxes">
					<header>
						<div>
							<p class="eyebrow">Box inspector</p>
							<h3>Unspent at this anchor</h3>
						</div>
						<span class="box-count">{inspector.boxes.length} loaded</span>
					</header>
					<p class="box-help">
						Inclusion heights are the blocks that created these boxes. Box and token detail links
						open their current indexed records.
					</p>
					<ol>
						{#each inspector.boxes as box, index (box.box_id)}<li class="historical-box">
								<div class="box-identity">
									<span class="box-index">{String(index + 1).padStart(2, '0')}</span><Hash
										value={box.box_id}
										href={`/box/${box.box_id}`}
										copy={false}
										head={12}
									/>
								</div>
								<div class="box-value">
									<strong>{formatErg(box.nano)} <span>ERG</span></strong>
									<p>
										Included {box.inclusion_height === 0
											? 'at genesis'
											: `in block ${box.inclusion_height.toLocaleString('en-US')}`}
									</p>
								</div>
								{#if box.tokens.length}<details
										ontoggle={(event) => (expanded[box.box_id] = event.currentTarget.open)}
									>
										<summary
											>{box.tokens.length} raw token balance{box.tokens.length === 1
												? ''
												: 's'}</summary
										>{#if expanded[box.box_id]}<ul>
												{#each box.tokens as token (token.token_id)}<li>
														<Hash
															value={token.token_id}
															href={`/token/${token.token_id}`}
															copy={false}
														/><span class="mono">{formatNano(token.amount)} raw units</span>
													</li>{/each}
											</ul>{/if}
									</details>{/if}
							</li>{/each}
					</ol>
					{#if inspector.boxesLoaded && !inspector.busy}<p class="page-state" role="status">
							{#if allBoxes}All historical box pages are loaded.{:else if inspector.viewLimit}This
								view reached its safety limit. The loaded boxes are a partial view.{:else if !inspector.boxes.length}No
								qualifying boxes in this page; more candidates remain to be scanned.{:else}More
								boxes or historical candidates remain.{/if}
						</p>{/if}
					{#if inspector.boxesLoaded && inspector.nextCursor !== null && !inspector.viewLimit && !inspector.restartRequired}<button
							class="load-boxes"
							type="button"
							disabled={inspector.busy}
							onclick={() => void controller.more()}
							>{inspector.boxes.length
								? 'Load more historical boxes'
								: 'Continue historical scan'}</button
						>{/if}
					<a class="historical-api" href={apiLink} target="_blank" rel="noreferrer"
						>{inspector.nextCursor
							? 'Continue from this cursor in the historical API'
							: 'Inspect the historical box API'} ↗</a
					>
				</section>
			</div>
			<p class="anchor-note">
				This snapshot was checked against the indexed chain. Later blocks preserve the state at this
				anchor; a rollback that changes it requires a fresh read. No automatic scans run in the
				background.
			</p>
		{/if}
	{:else}
		<form
			class="history-controls comparison-controls"
			onsubmit={(event) => {
				event.preventDefault();
				void compare();
			}}
		>
			<label for={`${uid}-from`}
				>Earlier block height<input
					id={`${uid}-from`}
					class="control mono"
					inputmode="numeric"
					maxlength="10"
					bind:value={fromHeight}
				/></label
			>
			<label for={`${uid}-to`}
				>Later block height<input
					id={`${uid}-to`}
					class="control mono"
					inputmode="numeric"
					maxlength="10"
					bind:value={toHeight}
				/></label
			>
			<button type="submit" class="btn btn-fill" disabled={comparison.busy}
				>{comparison.busy ? 'Comparing…' : 'Compare balances'}</button
			>
			<p>
				Both complete balances are read from one indexed chain snapshot. Height 0 is genesis; equal
				heights are allowed. No reads run until you compare.
			</p>
		</form>
		{#if formError}<p class="history-error" role="alert">{formError}</p>{/if}
		{#if comparison.issue}<div class="history-error" role="alert">
				<p>{comparison.issue.message}</p>
				<button type="button" onclick={() => void compare(comparison.issue?.restart)}
					>{comparison.issue.restart
						? 'Restart comparison at these heights'
						: 'Retry comparison'}</button
				>
			</div>{/if}
		{#if comparison.busy}<p role="status" class="history-progress">
				Reading both complete endpoint balances…
			</p>{/if}
		{#if comparison.result && comparison.changes}
			<section class="comparison-result" aria-label="Historical balance comparison">
				<header class="history-anchor">
					<div>
						<p class="eyebrow">One chain snapshot · two complete states</p>
						<h3>The balance difference.</h3>
						<p>Checked at indexed height {comparison.result.indexed_height ?? 'genesis'}.</p>
					</div>
					<a class="permalink" href={comparisonLink}>Pinned comparison link ↗</a>
				</header>
				<div class="comparison-points">
					{#each [{ label: 'Before', point: comparison.result.from }, { label: 'After', point: comparison.result.to }] as entry (entry.label)}
						<article class="comparison-point" aria-label={`${entry.label} historical balance`}>
							<p class="eyebrow">
								{entry.label} · {entry.point.at.height === 0
									? 'Genesis'
									: `After block ${entry.point.at.height.toLocaleString('en-US')}`}
							</p>
							<p class="history-total">{formatErg(entry.point.balance.nano)} <span>ERG</span></p>
							<p class="raw-nano mono">{entry.point.balance.nano} nanoERG</p>
							<p class="point-count">
								{entry.point.balance.box_count.toLocaleString('en-US')} unspent boxes
							</p>
							{#if entry.point.at.block_id}<Hash
									value={entry.point.at.block_id}
									href={`/blocks/${entry.point.at.block_id}`}
									head={10}
								/>{:else}<p class="history-scope">Recognized mainnet genesis · no block ID</p>{/if}
						</article>
					{/each}
					<article class="comparison-delta" aria-label="Exact balance difference">
						<p class="eyebrow">After minus before</p>
						<p class="history-total">
							{BigInt(comparison.changes.nano) > 0n ? '+' : ''}{formatErg(comparison.changes.nano)}
							<span>ERG</span>
						</p>
						<p class="raw-nano mono">{comparison.changes.nano} nanoERG difference</p>
						<p class="point-count">
							{BigInt(comparison.changes.box_count) > 0n ? '+' : ''}{formatNano(
								comparison.changes.box_count
							)} unspent box count difference
						</p>
					</article>
				</div>
				<p class="comparison-scope">
					These are differences between end-of-block balances, not transaction volume, minted or
					burned amounts, or counts of created and spent boxes. Assets can leave and return between
					the two endpoints. No wallet ownership is inferred.
				</p>
				{#if comparison.changes.tokens.length}<div class="comparison-token-list">
						<h3>Raw token balance differences</h3>
						<p>
							Token IDs and raw integer units; today’s names and decimals are not applied to
							historical balances. Zero means the endpoint amounts match.
						</p>
						<label for={`${uid}-compare-token`}
							>Filter comparison token IDs<input
								id={`${uid}-compare-token`}
								class="control mono"
								type="search"
								bind:value={tokenFilter}
								oninput={() => (tokenLimit = 50)}
							/></label
						>
						<ul>
							{#each comparisonTokens.slice(0, tokenLimit) as token (token.token_id)}<li>
									<Hash
										value={token.token_id}
										href={`/token/${token.token_id}`}
										copy={false}
										head={12}
									/>
									<dl>
										<div>
											<dt>Before</dt>
											<dd>{formatNano(token.before)}</dd>
										</div>
										<div>
											<dt>After</dt>
											<dd>{formatNano(token.after)}</dd>
										</div>
										<div>
											<dt>Difference</dt>
											<dd>{BigInt(token.delta) > 0n ? '+' : ''}{formatNano(token.delta)}</dd>
										</div>
									</dl>
								</li>{/each}
						</ul>
						{#if !comparisonTokens.length}<p>
								No token IDs match this filter.
							</p>{:else if comparisonTokens.length > tokenLimit}{#if tokenLimit < 200}<button
									type="button"
									onclick={() => (tokenLimit += 50)}>Show more token differences</button
								>{:else}<p>
									Showing 200 matches. Narrow the token ID filter to inspect another token.
								</p>{/if}{/if}
					</div>{:else}<p class="history-empty">No token balances at either endpoint.</p>{/if}
			</section>
		{:else if !comparison.busy && !comparison.issue}<div class="history-intro">
				<span class="intro-mark" aria-hidden="true">Δ</span>
				<div>
					<h3>Two moments. One exact comparison.</h3>
					<p>
						Choose earlier and later heights, then compare. Shared links preserve both block anchors
						but never start a read automatically. Partial box pages cannot establish a complete
						comparison.
					</p>
				</div>
			</div>{/if}
	{/if}
</section>

<style>
	.history-mode {
		display: flex;
		gap: 8px;
		flex-wrap: wrap;
		margin-bottom: 18px;
	}
	.history-mode button,
	.comparison-token-list button {
		border: var(--rule);
		border-radius: var(--radius-control);
		background: var(--surface-solid);
		color: var(--fg);
		padding: 10px 13px;
		font: inherit;
		font-size: 12px;
		cursor: pointer;
	}
	.history-mode button[aria-pressed='true'] {
		color: var(--accent-ink);
		background: var(--accent-wash);
		border-color: var(--accent-ink);
	}
	.comparison-points {
		display: grid;
		grid-template-columns: repeat(3, minmax(0, 1fr));
		gap: 22px;
		padding: 24px 0;
	}
	.comparison-point,
	.comparison-delta {
		min-width: 0;
		padding: 16px 0;
	}
	.comparison-points .history-total {
		font-size: clamp(23px, 2.3vw, 32px);
	}
	.point-count {
		font-size: 12px;
		margin: 16px 0;
		color: var(--fg-muted);
		overflow-wrap: anywhere;
	}
	.comparison-point :global(.hash) {
		font-size: 11px;
	}
	.comparison-delta {
		border-top: 3px solid var(--accent-ink);
	}
	.comparison-scope,
	.comparison-token-list > p {
		font-size: 12px;
		color: var(--fg-muted);
		line-height: 1.8;
		margin: 16px 0;
	}
	.comparison-token-list {
		border-top: var(--rule);
		padding-top: 24px;
		margin-top: 24px;
	}
	.comparison-token-list h3 {
		font: 600 22px var(--font-display, var(--font-sans));
	}
	.comparison-token-list label {
		display: grid;
		gap: 8px;
		font-size: 12px;
		max-width: 480px;
		margin: 18px 0;
	}
	.comparison-token-list input {
		min-width: 0;
		width: 100%;
	}
	.comparison-token-list li {
		display: grid;
		grid-template-columns: minmax(0, 1fr) minmax(0, 2fr);
		gap: 20px;
		align-items: center;
		padding: 18px 0;
		border-bottom: var(--rule);
		font-size: 12px;
	}
	.comparison-token-list dl {
		display: grid;
		grid-template-columns: repeat(3, minmax(0, 1fr));
		gap: 16px;
	}
	.comparison-token-list dt {
		font-size: 11px;
		color: var(--fg-muted);
		margin-bottom: 8px;
	}
	.comparison-token-list dd {
		font: 500 14px var(--font-number, var(--font-mono));
		overflow-wrap: anywhere;
	}
	:global(:root[data-appearance='prism']) .comparison-point {
		padding: 20px;
		background: var(--surface-solid);
		border: var(--rule);
		border-radius: 14px;
		box-shadow: var(--shadow-rest);
	}
	:global(:root[data-appearance='prism']) .comparison-delta {
		padding: 20px;
		background: var(--hero-bg);
		color: var(--hero-fg);
		border-radius: 14px;
		box-shadow: var(--shadow-lift);
	}
	:global(:root[data-appearance='prism'])
		.comparison-delta
		:is(.eyebrow, .raw-nano, .point-count, .history-total > span) {
		color: var(--hero-muted);
	}
	:global(:root[data-appearance='atelier']) .comparison-points {
		grid-template-columns: minmax(0, 1fr) minmax(0, 1fr);
		border-bottom: 3px double var(--fg);
	}
	:global(:root[data-appearance='atelier']) .comparison-delta {
		grid-column: 1/-1;
		display: grid;
		grid-template-columns: minmax(0, 1fr) minmax(0, 1fr);
		gap: 8px 30px;
		border-top: 3px double var(--fg);
	}
	:global(:root[data-appearance='atelier']) .comparison-delta .eyebrow {
		grid-column: 1/-1;
	}
	:global(:root[data-appearance='atelier']) .comparison-token-list li {
		grid-template-columns: minmax(0, 0.7fr) minmax(0, 2fr);
	}
	:global(:root[data-appearance='aurora']) .comparison-points {
		grid-template-columns: repeat(2, minmax(0, 1fr));
	}
	:global(:root[data-appearance='aurora']) .comparison-point {
		border: var(--rule);
		border-radius: 24px;
		padding: 26px;
		text-align: center;
		background: var(--surface-solid);
	}
	:global(:root[data-appearance='aurora']) .comparison-delta {
		grid-column: 1/-1;
		text-align: center;
		padding: 26px;
		border-radius: 24px;
		background: var(--surface-hover);
	}
	@media (max-width: 800px) {
		.comparison-points,
		:global(:root[data-appearance='atelier']) .comparison-points,
		:global(:root[data-appearance='aurora']) .comparison-points {
			grid-template-columns: minmax(0, 1fr);
		}
		.comparison-token-list li,
		:global(:root[data-appearance='atelier']) .comparison-token-list li {
			grid-template-columns: minmax(0, 1fr);
		}
		:global(:root[data-appearance='atelier']) .comparison-delta {
			display: block;
		}
	}
	@media (max-width: 400px) {
		.comparison-token-list dl {
			grid-template-columns: minmax(0, 1fr);
			gap: 12px;
		}
		.comparison-token-list dl > div {
			display: grid;
			grid-template-columns: 70px minmax(0, 1fr);
			gap: 10px;
		}
		.comparison-points .history-total {
			font-size: 24px;
		}
	}
	.address-history {
		min-width: 0;
		padding-top: 22px;
	}
	.history-heading {
		display: flex;
		align-items: start;
		justify-content: space-between;
		gap: 24px;
		margin-bottom: 24px;
	}
	.eyebrow {
		font: 11px var(--font-mono);
		text-transform: uppercase;
		letter-spacing: 0.09em;
		color: var(--accent-ink);
		margin-bottom: 9px;
	}
	.history-heading h2 {
		font: var(--weight-display, 700) clamp(28px, 3vw, 40px)/1.08
			var(--font-display, var(--font-sans));
		letter-spacing: -0.04em;
		margin-bottom: 14px;
	}
	.history-heading p:not(.eyebrow) {
		max-width: 65ch;
		font-size: 13px;
		color: var(--fg-muted);
		line-height: 1.7;
	}
	.history-tag {
		font: 11px var(--font-mono);
		border: var(--rule);
		border-radius: 20px;
		padding: 8px 12px;
		white-space: nowrap;
	}
	.history-controls {
		display: flex;
		flex-wrap: wrap;
		gap: 12px;
		align-items: end;
		background: var(--surface-hover);
		border: var(--rule);
		border-radius: 12px;
		padding: 16px;
	}
	.history-controls label {
		display: grid;
		gap: 8px;
		flex: 1 1 160px;
		font-size: 12px;
	}
	.history-controls input {
		width: 100%;
		min-width: 0;
		height: 42px;
	}
	.history-controls button {
		min-height: 42px;
		font-size: 12px;
	}
	.history-controls > p {
		flex-basis: 100%;
		font-size: 11px;
		line-height: 1.6;
		color: var(--fg-muted);
	}
	.history-intro {
		display: flex;
		gap: 24px;
		align-items: center;
		padding: 34px 20px;
	}
	.intro-mark {
		display: grid;
		place-items: center;
		width: 70px;
		height: 70px;
		flex: none;
		font: 40px var(--font-display, var(--font-sans));
		color: var(--accent-ink);
		background: var(--accent-wash);
		border: var(--rule);
		border-radius: 16px;
	}
	.history-intro h3 {
		font-size: 18px;
		margin-bottom: 10px;
	}
	.history-intro p {
		font-size: 13px;
		line-height: 1.7;
		color: var(--fg-muted);
		max-width: 70ch;
	}
	.pinned-id {
		overflow-wrap: anywhere;
	}
	.history-error {
		padding: 16px;
		margin: 16px 0;
		border-left: 3px solid var(--danger-ink);
		background: var(--surface-hover);
		color: var(--danger-ink);
		font-size: 13px;
		line-height: 1.7;
	}
	.history-notice {
		color: var(--warn-ink);
		font-size: 12px;
		line-height: 1.7;
		margin: 16px 0;
	}
	.history-error button,
	.history-tokens button,
	.load-boxes {
		border: var(--rule);
		border-radius: 8px;
		background: var(--surface-solid);
		color: var(--fg);
		padding: 9px 13px;
		font-size: 12px;
		cursor: pointer;
		margin-top: 12px;
	}
	button:disabled {
		opacity: 0.6;
		cursor: default;
	}
	.history-progress {
		margin: 18px 0;
		font-size: 12px;
		color: var(--fg-muted);
	}
	.history-anchor {
		display: flex;
		align-items: center;
		justify-content: space-between;
		gap: 20px;
		flex-wrap: wrap;
		padding: 25px 0;
		border-bottom: var(--rule);
	}
	.history-anchor > div {
		min-width: 0;
	}
	.history-anchor h3 {
		font: 600 24px var(--font-display, var(--font-sans));
		margin-bottom: 10px;
	}
	.history-anchor :global(.hash) {
		font-size: 12px;
	}
	.history-anchor p,
	.permalink {
		font-size: 12px;
	}
	.permalink,
	.historical-api {
		color: var(--accent-ink);
		text-decoration: underline;
	}
	.history-workspace {
		display: grid;
		grid-template-columns: minmax(0, 1fr);
		gap: 28px;
		padding-top: 28px;
	}
	.history-summary {
		min-width: 0;
	}
	.history-total {
		font: 500 clamp(28px, 3vw, 40px) var(--font-number, var(--font-sans));
		overflow-wrap: anywhere;
	}
	.history-total > span {
		font-size: 14px;
		color: var(--fg-muted);
	}
	.raw-nano {
		color: var(--fg-muted);
		font-size: 11px;
		margin-top: 8px;
		overflow-wrap: anywhere;
	}
	.history-counts {
		display: flex;
		gap: 28px;
		margin: 24px 0;
	}
	.history-counts dt {
		font-size: 11px;
		color: var(--fg-muted);
		margin-bottom: 8px;
	}
	.history-counts dd {
		font: 500 23px var(--font-number, var(--font-sans));
	}
	.history-empty {
		font-weight: 600;
		font-size: 14px;
		margin-bottom: 14px;
	}
	.history-scope,
	.anchor-note,
	.box-help {
		font-size: 11px;
		color: var(--fg-muted);
		line-height: 1.8;
	}
	.history-tokens {
		margin-top: 24px;
		padding-top: 22px;
		border-top: var(--rule);
	}
	.history-tokens h3 {
		font-size: 17px;
		margin-bottom: 10px;
	}
	.history-tokens > p {
		font-size: 12px;
		color: var(--fg-muted);
		line-height: 1.7;
	}
	.history-tokens label {
		display: grid;
		gap: 8px;
		font-size: 12px;
		margin-top: 14px;
	}
	.history-tokens input {
		width: 100%;
		min-width: 0;
	}
	.history-tokens li {
		display: grid;
		grid-template-columns: minmax(0, 1fr) auto;
		gap: 8px;
		padding: 14px 0;
		border-bottom: var(--rule);
		font-size: 12px;
	}
	.history-tokens li strong {
		font: 600 15px var(--font-number, var(--font-sans));
		overflow-wrap: anywhere;
		text-align: right;
		max-width: 24ch;
	}
	.history-tokens li > span {
		grid-column: 2;
		color: var(--fg-muted);
		font-size: 11px;
		text-align: right;
	}
	.history-boxes {
		min-width: 0;
	}
	.history-boxes > header {
		display: flex;
		justify-content: space-between;
		align-items: center;
		gap: 14px;
		flex-wrap: wrap;
		margin-bottom: 12px;
	}
	.history-boxes h3 {
		font: 600 22px var(--font-display, var(--font-sans));
	}
	.box-count {
		font: 11px var(--font-mono);
		color: var(--fg-muted);
	}
	.historical-box {
		padding: 20px 0;
		border-bottom: var(--rule);
		min-width: 0;
	}
	.box-identity {
		display: flex;
		gap: 12px;
		align-items: center;
		font-size: 12px;
		min-width: 0;
	}
	.box-index {
		font: 11px var(--font-mono);
		color: var(--fg-muted);
	}
	.box-value {
		display: flex;
		gap: 12px;
		flex-wrap: wrap;
		justify-content: space-between;
		align-items: center;
		margin-top: 14px;
	}
	.box-value strong {
		font: 600 20px var(--font-number, var(--font-sans));
		overflow-wrap: anywhere;
	}
	.box-value strong span {
		font-size: 12px;
		font-weight: 400;
		color: var(--fg-muted);
	}
	.box-value p {
		font-size: 11px;
		color: var(--fg-muted);
	}
	details {
		font-size: 12px;
		margin-top: 12px;
	}
	summary {
		cursor: pointer;
		color: var(--accent-ink);
	}
	details ul {
		padding-top: 10px;
	}
	details li {
		display: flex;
		gap: 12px;
		flex-wrap: wrap;
		justify-content: space-between;
		padding: 8px 0;
		overflow-wrap: anywhere;
	}
	.page-state {
		font-size: 12px;
		color: var(--fg-muted);
		line-height: 1.7;
		margin: 18px 0;
	}
	.historical-api {
		display: block;
		font-size: 11px;
		margin-top: 18px;
		overflow-wrap: anywhere;
	}
	.anchor-note {
		border-top: var(--rule);
		padding-top: 20px;
		margin-top: 28px;
	}
	:global(:root[data-appearance='prism']) .history-workspace {
		grid-template-columns: minmax(0, 0.8fr) minmax(0, 1.2fr);
		gap: 20px;
	}
	:global(:root[data-appearance='prism']) .history-summary {
		padding: 22px;
		border: var(--rule);
		border-radius: 14px;
		background: var(--surface-solid);
		align-self: start;
		box-shadow: var(--shadow-rest);
	}
	:global(:root[data-appearance='prism']) .historical-box {
		padding: 20px;
		border: var(--rule);
		border-radius: 12px;
		margin-top: 14px;
		background: var(--surface-solid);
	}
	:global(:root[data-appearance='atelier']) .history-heading {
		border-bottom: 3px double var(--fg);
		padding-bottom: 20px;
	}
	:global(:root[data-appearance='atelier']) .history-controls {
		background: none;
		border-width: 0 0 1px;
		border-radius: 0;
		padding-inline: 0;
	}
	:global(:root[data-appearance='atelier']) .historical-box {
		display: grid;
		grid-template-columns: minmax(0, 1fr) minmax(0, 1fr);
		gap: 14px 24px;
	}
	:global(:root[data-appearance='atelier']) .box-value {
		justify-content: end;
		margin: 0;
		text-align: right;
	}
	:global(:root[data-appearance='atelier']) .historical-box details {
		grid-column: 1/-1;
	}
	:global(:root[data-appearance='atelier']) .history-summary {
		display: grid;
		grid-template-columns: minmax(0, 1fr) minmax(0, 1fr);
		gap: 10px 30px;
	}
	:global(:root[data-appearance='atelier']) .history-summary > .eyebrow,
	:global(:root[data-appearance='atelier']) .history-tokens,
	:global(:root[data-appearance='atelier']) .history-scope,
	:global(:root[data-appearance='atelier']) .history-notice,
	:global(:root[data-appearance='atelier']) .history-empty {
		grid-column: 1/-1;
	}
	:global(:root[data-appearance='atelier']) .history-counts {
		grid-column: 2;
		grid-row: 2/4;
		margin: 0;
		justify-content: end;
	}
	:global(:root[data-appearance='aurora']) .history-heading {
		display: block;
		text-align: center;
		padding: 20px 0;
	}
	:global(:root[data-appearance='aurora']) .history-heading p {
		margin-inline: auto;
	}
	:global(:root[data-appearance='aurora']) .history-tag {
		display: inline-block;
		margin-top: 16px;
	}
	:global(:root[data-appearance='aurora']) .history-summary {
		padding: 30px;
		border: var(--rule);
		border-radius: 24px;
		background: var(--surface-solid);
		box-shadow: var(--shadow-lift);
	}
	:global(:root[data-appearance='aurora']) .history-boxes > ol {
		display: grid;
		grid-template-columns: repeat(2, minmax(0, 1fr));
		gap: 20px;
	}
	:global(:root[data-appearance='aurora']) .historical-box {
		padding: 24px;
		border: var(--rule);
		border-radius: 22px;
		margin-top: 16px;
	}
	@media (max-width: 1100px) {
		:global(:root[data-appearance='prism']) .history-workspace {
			grid-template-columns: minmax(0, 1fr);
		}
	}
	@media (max-width: 700px) {
		.history-heading {
			display: block;
		}
		.history-tag {
			display: inline-block;
			margin-top: 15px;
		}
		.history-intro {
			padding: 24px 0;
			align-items: start;
			gap: 16px;
		}
		.intro-mark {
			width: 48px;
			height: 48px;
			font-size: 28px;
		}
		:global(:root[data-appearance='aurora']) .history-boxes > ol {
			grid-template-columns: minmax(0, 1fr);
		}
		:global(:root[data-appearance='atelier']) .historical-box,
		:global(:root[data-appearance='atelier']) .history-summary {
			display: block;
		}
		:global(:root[data-appearance='atelier']) .history-counts {
			justify-content: start;
			margin: 22px 0;
		}
		:global(:root[data-appearance='atelier']) .box-value {
			justify-content: start;
			text-align: left;
			margin-top: 14px;
		}
	}
	@media (max-width: 400px) {
		.history-controls {
			padding: 12px;
		}
		.history-controls label,
		.history-controls button {
			flex-basis: 100%;
			width: 100%;
			justify-content: center;
		}
		.history-total {
			font-size: 28px;
		}
		.history-tokens li {
			grid-template-columns: minmax(0, 1fr);
		}
		.history-tokens li strong,
		.history-tokens li > span {
			grid-column: 1;
			text-align: left;
			max-width: none;
		}
		:global(:root[data-appearance='prism']) .history-summary,
		:global(:root[data-appearance='aurora']) .history-summary,
		:global(:root[data-appearance='prism']) .historical-box,
		:global(:root[data-appearance='aurora']) .historical-box {
			padding: 16px;
		}
		.history-counts {
			gap: 20px;
		}
	}
</style>
