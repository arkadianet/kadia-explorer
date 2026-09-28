<script lang="ts">
	import { onDestroy, tick, untrack } from 'svelte';
	import { goto } from '$app/navigation';
	import { page } from '$app/state';
	import { status } from '$lib/status/status.svelte';
	import { createHistoryInspector, historySelector, sumHistoryBoxes } from '$lib/addresses/history';
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
	const urlKey = $derived(
		JSON.stringify([page.url.searchParams.get('at_height'), page.url.searchParams.get('at_block')])
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
			height = page.url.searchParams.get('at_height') ?? String(status.current?.indexed ?? '');
			formError = null;
			tokenFilter = '';
			tokenLimit = 50;
			expanded = {};
		});
	});
	onDestroy(() => controller.stop());
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
					Amounts use inclusion and spend heights. A box created and spent within the selected block
					is excluded. Mempool activity and historical prices are outside this snapshot.
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
					Inclusion heights are the blocks that created these boxes. Box and token detail links open
					their current indexed records.
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
							qualifying boxes in this page; more candidates remain to be scanned.{:else}More boxes
							or historical candidates remain.{/if}
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
</section>

<style>
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
