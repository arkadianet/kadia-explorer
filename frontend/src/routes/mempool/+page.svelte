<script lang="ts">
	import { onMount } from 'svelte';
	import { page } from '$app/state';
	import { replaceState } from '$app/navigation';
	import Icon from '$lib/components/Icon.svelte';
	import PendingConnections from '$lib/components/PendingConnections.svelte';
	import { formatErg, formatNano } from '$lib/format/amount';
	import {
		createMempoolLoader,
		filterPending,
		parseMempoolFocus,
		type MempoolState
	} from '$lib/mempool/observations';

	let view = $state<MempoolState>({ loading: true, snapshot: null, error: null });
	let query = $state('');
	let loader: ReturnType<typeof createMempoolLoader> | undefined;
	onMount(() => {
		const current = createMempoolLoader((next) => {
			view = next;
		});
		loader = current;
		void current.refresh();
		return () => current.stop();
	});
	const matches = $derived(filterPending(view.snapshot?.items ?? [], query));
	let requestedFocus = $state<string | null>(null);
	let repeatedFocus = $state(false);
	$effect(() => {
		// Shallow replaceState updates browser history, not page.url. Keep local
		// selection reactive and reset it when a real route navigation arrives.
		requestedFocus = page.url.searchParams.get('focus');
		repeatedFocus = page.url.searchParams.getAll('focus').length !== 1;
	});
	const invalidFocus = $derived(
		requestedFocus !== null && (repeatedFocus || !parseMempoolFocus(requestedFocus))
	);
	const selected = $derived(
		requestedFocus === null
			? (view.snapshot?.items[0]?.id ?? null)
			: invalidFocus
				? null
				: parseMempoolFocus(requestedFocus)
	);
	function selectConnection(id: string) {
		if (!parseMempoolFocus(id)) return;
		const url = new URL(window.location.href);
		url.searchParams.set('focus', id);
		replaceState(url, page.state);
		requestedFocus = id;
		repeatedFocus = false;
	}
	const problemTitle = $derived(
		view.error?.code === 'mempool_not_configured'
			? 'No node configured'
			: view.error?.code === 'mempool_unsupported'
				? 'Mempool access is unsupported'
				: view.error?.code === 'mempool_busy'
					? 'A node check is already in progress'
					: view.error?.code === 'mempool_invalid_response'
						? 'The node response could not be interpreted'
						: 'The configured node is unavailable'
	);
	function utc(milliseconds: number): string {
		const date = new Date(milliseconds);
		return Number.isNaN(date.valueOf())
			? milliseconds + ' ms since epoch'
			: date.toISOString().replace('T', ' ').replace('Z', ' UTC');
	}
</script>

<svelte:head>
	<title>Mempool — Kadia Explorer</title>
	<meta
		name="description"
		content="Inspect pending Ergo transactions observed by the configured node, with exact fees, source scope and explicit refresh."
	/>
</svelte:head>

<div class="mempool-page">
	<header class="mempool-intro">
		<div>
			<p class="eyebrow">NETWORK / BEFORE CONFIRMATION</p>
			<h1>The mempool.</h1>
			<p>
				Transactions waiting in the configured node’s local view. Inspect what it observed, then
				follow any transaction to its latest status.
			</p>
		</div>
		<button type="button" onclick={() => void loader?.refresh()} disabled={view.loading}>
			<Icon name="clock" size={16} />{view.loading ? 'Checking node…' : 'Refresh mempool'}
		</button>
	</header>

	{#if view.loading}
		<section class="mempool-notice checking" aria-label="Checking mempool" aria-busy="true">
			<Icon name="clock" size={32} />
			<h2>Checking the configured node.</h2>
			<p>Loading the current node observation.</p>
		</section>
	{:else if view.error}
		<section class="mempool-notice error" role="alert">
			<p class="eyebrow">OBSERVATION UNAVAILABLE</p>
			<h2>{problemTitle}</h2>
			<p>{view.error.message}</p>
			<p>No successful node observation is available. Refresh to check again.</p>
			<a href="/status">Open explorer status <span aria-hidden="true">↗</span></a>
		</section>
	{:else if view.snapshot}
		{@const snapshot = view.snapshot}
		{#if snapshot.items.length || requestedFocus !== null}
			<PendingConnections {snapshot} {selected} {invalidFocus} onselect={selectConnection} />
		{/if}
		<div class="mempool-layout">
			<aside class="observation-summary" aria-label="Mempool observation details">
				<div class="count-block">
					<p class="eyebrow">TRANSACTIONS RETURNED</p>
					<p class="snapshot-count">
						{snapshot.observed_count.toLocaleString('en-US')}{#if snapshot.limit_reached}<span
								>at the observation limit</span
							>{/if}
					</p>
				</div>
				<dl>
					<div>
						<dt>Source</dt>
						<dd>Configured primary node</dd>
					</div>
					<div>
						<dt>Observed at</dt>
						<dd>
							<time datetime={new Date(snapshot.checked_at_ms).toISOString()}
								>{utc(snapshot.checked_at_ms)}</time
							>
						</dd>
					</div>
					<div>
						<dt>Response</dt>
						<dd>
							{snapshot.cached ? 'Reused node observation' : 'Node checked for this observation'}
						</dd>
					</div>
				</dl>
				<p class="observation-note">
					Refreshes can reuse this observation until {utc(snapshot.expires_at_ms)}. Pending
					transactions can change immediately.
				</p>
				<p class="scope-note">
					This is one node’s view. Presence does not guarantee inclusion, and this list is not a
					network-wide total.
				</p>
			</aside>
			<section class="pending-list-panel" aria-label="Pending transactions">
				<div class="list-heading">
					<div>
						<p class="eyebrow">PENDING OBSERVATIONS</p>
						<h2>
							{snapshot.observed_count === 0
								? 'No pending transactions observed.'
								: 'Waiting for a block.'}
						</h2>
					</div>
					{#if snapshot.observed_count > 0}<label class="filter"
							>Filter loaded transaction IDs<input
								type="search"
								maxlength="64"
								bind:value={query}
								placeholder="Transaction ID or prefix"
								spellcheck="false"
								autocomplete="off"
							/></label
						>{/if}
				</div>
				{#if snapshot.limit_reached}<p class="limit-notice" role="status">
						The observation reached its 100-transaction limit. Additional pending transactions may
						exist.
					</p>{/if}
				{#if snapshot.observed_count === 0}
					<div class="empty-observation">
						<Icon name="layers" size={34} />
						<p>The node successfully returned an empty list at the observation time.</p>
					</div>
				{:else}
					<p class="list-scope">
						Showing {matches.length} of {snapshot.observed_count} loaded transactions. Order is supplied
						by the node.
					</p>
					{#if matches.length === 0}<div class="empty-filter">
							<h3>No loaded ID matches this filter.</h3>
							<button
								type="button"
								class="secondary"
								onclick={() => {
									query = '';
								}}>Clear ID filter</button
							>
						</div>{/if}
					<div class="pending-list">
						{#each matches as transaction (transaction.id)}
							<article
								class="pending-transaction"
								aria-label={'Pending transaction ' + transaction.id}
							>
								<div class="transaction-identity">
									<span class="pending-label"
										><Icon name="clock" size={13} />Pending at observation</span
									><a class="transaction-id" href={'/tx/' + transaction.id}
										>{transaction.id}<Icon name="arrow-right" size={16} /></a
									>
								</div>
								<dl class="transaction-facts">
									<div>
										<dt>Inputs / outputs</dt>
										<dd>
											{transaction.input_count} <span aria-hidden="true">→</span><span
												class="visually-hidden"
											>
												inputs and
											</span>
											{transaction.output_count}<span class="visually-hidden"> outputs</span>
										</dd>
									</div>
									<div>
										<dt>Data inputs</dt>
										<dd>{transaction.data_input_count}</dd>
									</div>
									<div>
										<dt>Size</dt>
										<dd>
											{transaction.size === null
												? 'Not supplied'
												: transaction.size.toLocaleString('en-US') + ' bytes'}
										</dd>
									</div>
									<div class="fee">
										<dt>Miner-fee outputs</dt>
										<dd>
											{#if transaction.fee !== null}<span title={transaction.fee + ' nanoERG'}
													>{formatErg(transaction.fee)} ERG</span
												><small>{formatNano(transaction.fee)} nanoERG</small>{:else}Unknown{/if}
										</dd>
									</div>
								</dl>
								<button
									type="button"
									class="secondary inspect-connections"
									onclick={() => selectConnection(transaction.id)}
									aria-label={'Inspect connections for listed transaction ' + transaction.id}
									>Inspect connections</button
								>
							</article>
						{/each}
					</div>
					<p class="list-footnote">
						Listed fees are the exact total value of supported miner-fee outputs.
					</p>
				{/if}
			</section>
		</div>
	{/if}
</div>

<style>
	.mempool-page {
		width: 100%;
		min-width: 0;
	}
	.mempool-intro {
		display: flex;
		align-items: end;
		justify-content: space-between;
		gap: 28px;
		margin-bottom: 30px;
	}
	.eyebrow {
		font: 11px var(--font-mono);
		letter-spacing: 0.11em;
		color: var(--fg-muted);
	}
	h1 {
		font: var(--weight-display, 750) clamp(38px, 5vw, 66px)/1 var(--font-display, var(--font-sans));
		letter-spacing: -0.05em;
		margin-block: 12px 18px;
	}
	.mempool-intro > div > p:last-child {
		max-width: 70ch;
		font-size: 13px;
		line-height: 1.75;
		color: var(--fg-muted);
	}
	button {
		display: inline-flex;
		align-items: center;
		justify-content: center;
		gap: 8px;
		border: 1px solid var(--btn-fill);
		border-radius: var(--radius-control);
		background: var(--btn-fill);
		color: var(--btn-fill-fg);
		padding: 12px 16px;
		font-size: 12px;
		cursor: pointer;
		flex-shrink: 0;
	}
	button:disabled {
		opacity: 0.65;
		cursor: wait;
	}
	.secondary {
		background: transparent;
		color: var(--fg);
		border-color: var(--hairline);
	}
	a {
		color: var(--accent-ink);
		overflow-wrap: anywhere;
	}
	.mempool-notice {
		padding: 38px;
		border: 1px solid var(--hairline);
		border-radius: var(--radius-card);
		background: var(--surface-solid);
	}
	h2 {
		font: var(--weight-display, 700) 27px/1.2 var(--font-display, var(--font-sans));
		letter-spacing: -0.035em;
	}
	.mempool-notice h2 {
		margin-block: 14px;
	}
	.mempool-notice p {
		font-size: 13px;
		line-height: 1.8;
		max-width: 76ch;
		margin-bottom: 12px;
	}
	.mempool-notice a {
		font-size: 12px;
		display: inline-block;
		margin-top: 10px;
	}
	.observation-summary {
		display: grid;
		grid-template-columns: minmax(160px, 0.6fr) minmax(0, 1.4fr);
		gap: 18px 30px;
		padding: 24px;
		border: 1px solid var(--hairline);
		border-radius: var(--radius-card);
		background: var(--surface-solid);
		margin-bottom: 28px;
	}
	.snapshot-count {
		font: var(--weight-number, 700) 58px/1.1 var(--font-number, var(--font-sans));
		letter-spacing: -0.05em;
		margin-top: 12px;
	}
	.snapshot-count span {
		display: block;
		font: 11px/1.5 var(--font-sans);
		letter-spacing: 0;
		margin-top: 6px;
		color: var(--fg-muted);
	}
	dl {
		margin: 0;
		min-width: 0;
	}
	.observation-summary dl {
		display: grid;
		gap: 12px;
	}
	.observation-summary dl > div {
		display: grid;
		grid-template-columns: 95px minmax(0, 1fr);
		gap: 12px;
	}
	dt {
		font-size: 11px;
		color: var(--fg-muted);
		line-height: 1.5;
	}
	dd {
		margin: 0;
		font-size: 12px;
		line-height: 1.5;
		overflow-wrap: anywhere;
	}
	time {
		font: 11px/1.6 var(--font-mono);
	}
	.observation-note,
	.scope-note {
		color: var(--fg-muted);
		font-size: 11px;
		line-height: 1.8;
	}
	.scope-note {
		border-left: 2px solid var(--accent-ink);
		padding-left: 14px;
	}
	.list-heading {
		display: flex;
		align-items: end;
		justify-content: space-between;
		gap: 24px;
		margin-block: 0 18px;
	}
	.list-heading h2 {
		margin-top: 8px;
	}
	.filter {
		display: grid;
		gap: 8px;
		font-size: 11px;
		color: var(--fg-muted);
		width: min(100%, 300px);
		flex-shrink: 0;
	}
	input {
		width: 100%;
		min-width: 0;
		padding: 11px 12px;
		border: 1px solid var(--hairline);
		border-radius: var(--radius-control);
		background: var(--surface-solid);
		color: var(--fg);
		font: 12px var(--font-mono);
	}
	.list-scope,
	.list-footnote {
		font-size: 11px;
		line-height: 1.8;
		color: var(--fg-muted);
		margin-block: 16px;
	}
	.limit-notice {
		border: 1px solid var(--hairline);
		background: var(--accent-wash);
		border-radius: var(--radius-control);
		padding: 14px 16px;
		font-size: 12px;
		line-height: 1.7;
	}
	.pending-list {
		border: 1px solid var(--hairline);
		border-radius: var(--radius-card);
		overflow: hidden;
	}
	.pending-transaction {
		padding: 22px;
		background: var(--surface-solid);
		min-width: 0;
	}
	.pending-transaction + .pending-transaction {
		border-top: 1px solid var(--hairline);
	}
	.transaction-identity {
		min-width: 0;
	}
	.pending-label {
		display: inline-flex;
		align-items: center;
		gap: 6px;
		font-size: 11px;
		color: var(--fg-muted);
		margin-bottom: 10px;
	}
	.transaction-id {
		display: flex;
		align-items: center;
		gap: 10px;
		font: 12px/1.7 var(--font-mono);
		overflow-wrap: anywhere;
		word-break: break-all;
	}
	.transaction-id :global(svg) {
		flex-shrink: 0;
	}
	.transaction-facts {
		display: grid;
		grid-template-columns: minmax(0, 1fr) minmax(0, 0.8fr) minmax(0, 1fr) minmax(0, 1.4fr);
		gap: 20px;
		margin-top: 20px;
	}
	.transaction-facts dd {
		font-variant-numeric: tabular-nums;
		margin-top: 4px;
	}
	.inspect-connections {
		margin-top: 18px;
	}
	small {
		display: block;
		font: 11px/1.6 var(--font-mono);
		color: var(--fg-muted);
		margin-top: 3px;
	}
	.empty-observation {
		padding: 36px;
		border: 1px dashed var(--hairline);
		border-radius: var(--radius-card);
	}
	.empty-observation p {
		font-size: 13px;
		color: var(--fg-muted);
		line-height: 1.8;
		margin-top: 14px;
	}
	.empty-filter {
		padding: 30px;
		border: 1px solid var(--hairline);
		border-radius: var(--radius-card);
	}
	.empty-filter h3 {
		font-size: 16px;
		margin-bottom: 18px;
	}
	:global([data-appearance='prism']) .mempool-layout {
		display: grid;
		grid-template-columns: minmax(200px, 0.35fr) minmax(0, 1fr);
		gap: 30px;
		align-items: start;
	}
	:global([data-appearance='prism']) .observation-summary {
		display: block;
		border-radius: 24px;
		background: linear-gradient(140deg, var(--surface-solid), var(--accent-wash));
		box-shadow: 0 18px 45px color-mix(in srgb, var(--accent-ink) 13%, transparent);
	}
	:global([data-appearance='prism']) .count-block {
		padding-block: 12px 24px;
	}
	:global([data-appearance='prism']) .observation-summary dl > div {
		grid-template-columns: 1fr;
		gap: 4px;
	}
	:global([data-appearance='prism']) .observation-note,
	:global([data-appearance='prism']) .scope-note {
		margin-top: 20px;
	}
	:global([data-appearance='prism']) .list-heading {
		display: block;
	}
	:global([data-appearance='prism']) .filter {
		margin-top: 20px;
		width: 100%;
	}
	:global([data-appearance='prism']) .pending-list {
		border: 0;
		overflow: visible;
	}
	:global([data-appearance='prism']) .pending-transaction {
		margin-bottom: 14px;
		border: 1px solid var(--hairline);
		border-radius: 17px;
		box-shadow: 0 8px 20px color-mix(in srgb, var(--fg) 5%, transparent);
	}
	:global([data-appearance='prism']) .transaction-facts {
		grid-template-columns: 1fr 1fr;
	}
	:global([data-appearance='atelier']) .mempool-intro {
		border-bottom: 4px double var(--fg);
		padding-block: 12px 30px;
	}
	:global([data-appearance='atelier']) .mempool-layout {
		display: grid;
		grid-template-columns: minmax(0, 1fr) minmax(180px, 0.3fr);
		gap: 30px;
		align-items: start;
	}
	:global([data-appearance='atelier']) .observation-summary {
		grid-column: 2;
		grid-row: 1;
		display: block;
		border: 0;
		border-left: 1px solid var(--hairline);
		border-radius: 0;
		padding: 0 0 0 22px;
		background: transparent;
	}
	:global([data-appearance='atelier']) .observation-summary dl {
		margin-top: 28px;
	}
	:global([data-appearance='atelier']) .observation-summary dl > div {
		grid-template-columns: 1fr;
		gap: 4px;
	}
	:global([data-appearance='atelier']) .observation-note,
	:global([data-appearance='atelier']) .scope-note {
		margin-top: 20px;
	}
	:global([data-appearance='atelier']) .pending-list-panel {
		grid-column: 1;
		grid-row: 1;
	}
	:global([data-appearance='atelier']) .list-heading {
		display: block;
	}
	:global([data-appearance='atelier']) .filter {
		margin-top: 22px;
		width: 100%;
	}
	:global([data-appearance='atelier']) .pending-list {
		border: 0;
		border-top: 1px solid var(--fg);
		border-radius: 0;
	}
	:global([data-appearance='atelier']) .pending-transaction {
		background: transparent;
		padding: 22px 0;
	}
	:global([data-appearance='atelier']) .transaction-facts {
		grid-template-columns: 1fr 1fr;
	}
	:global([data-appearance='atelier']) .mempool-notice {
		max-width: 760px;
		margin: 45px auto;
		border: 0;
		border-top: 4px double var(--fg);
		border-bottom: 1px solid var(--hairline);
		border-radius: 0;
		background: transparent;
	}
	:global([data-appearance='aurora']) .mempool-intro {
		display: grid;
		justify-content: stretch;
		justify-items: center;
		text-align: center;
		margin: 24px auto 36px;
	}
	:global([data-appearance='aurora']) .observation-summary {
		max-width: 1050px;
		margin-inline: auto;
		border-radius: 28px;
		background: var(--accent-wash);
		padding: 30px;
	}
	:global([data-appearance='aurora']) .pending-list {
		display: grid;
		grid-template-columns: repeat(2, minmax(0, 1fr));
		gap: 20px;
		border: 0;
		overflow: visible;
	}
	:global([data-appearance='aurora']) .pending-transaction {
		border: 1px solid var(--hairline);
		border-radius: 24px;
		padding: 26px;
	}
	:global([data-appearance='aurora']) .transaction-facts {
		grid-template-columns: 1fr 1fr;
	}
	:global([data-appearance='aurora']) .mempool-notice {
		max-width: 850px;
		margin-inline: auto;
		border-radius: 28px;
		text-align: center;
	}
	@media (max-width: 760px) {
		.mempool-intro,
		.list-heading {
			align-items: stretch;
			flex-direction: column;
		}
		.observation-summary {
			display: block;
			padding: 20px;
		}
		.observation-summary dl {
			margin-top: 24px;
		}
		.observation-note,
		.scope-note {
			margin-top: 18px;
		}
		.filter {
			width: 100%;
		}
		.transaction-facts {
			grid-template-columns: 1fr 1fr;
			gap: 16px;
		}
		.pending-transaction {
			padding: 18px;
		}
		.mempool-notice,
		.empty-observation,
		.empty-filter {
			padding: 24px 18px;
		}
		:global([data-appearance='prism']) .mempool-layout,
		:global([data-appearance='atelier']) .mempool-layout {
			display: block;
		}
		:global([data-appearance='atelier']) .observation-summary {
			border: 0;
			border-bottom: 1px solid var(--hairline);
			padding: 0 0 26px;
		}
		:global([data-appearance='aurora']) .pending-list {
			grid-template-columns: 1fr;
		}
		:global([data-appearance='aurora']) .observation-summary {
			padding: 22px;
		}
		:global([data-appearance='aurora']) .pending-transaction {
			padding: 20px;
		}
	}
</style>
