<script lang="ts">
	import { onMount, tick } from 'svelte';
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
	let inspectorOpen = $state(false);
	let inspector = $state<HTMLDivElement>();
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
	async function inspectListed(id: string) {
		selectConnection(id);
		inspectorOpen = true;
		if (!window.matchMedia('(min-width: 1000px)').matches) {
			await tick();
			inspector?.scrollIntoView({ block: 'nearest' });
			inspector?.querySelector('summary')?.focus({ preventScroll: true });
		}
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
		<aside class="observation-summary" aria-label="Mempool observation details">
			<div class="count-block">
				<strong>{snapshot.observed_count.toLocaleString('en-US')}</strong><span
					>transactions returned{snapshot.limit_reached ? ' · limit reached' : ''}</span
				>
			</div>
			<div class="observed-at">
				<span>Configured primary node</span><time
					datetime={new Date(snapshot.checked_at_ms).toISOString()}
					>{utc(snapshot.checked_at_ms)}</time
				>
			</div>
			<details class="observation-evidence">
				<summary>Observation details</summary>
				<div>
					<p>
						{snapshot.cached ? 'Reused node observation' : 'Node checked for this observation'}.
						Refreshes can reuse it until {utc(snapshot.expires_at_ms)}.
					</p>
					<p>Pending transactions can change immediately. Presence does not guarantee inclusion.</p>
				</div>
			</details>
			<p class="scope-note">One node’s view, not a network-wide total.</p>
		</aside>
		<div
			class="mempool-layout"
			class:with-inspector={snapshot.items.length > 0 || requestedFocus !== null}
		>
			{#if snapshot.items.length || requestedFocus !== null}
				<div class="connections-column" bind:this={inspector}>
					<PendingConnections
						{snapshot}
						{selected}
						{invalidFocus}
						focused={requestedFocus !== null}
						bind:open={inspectorOpen}
						onselect={selectConnection}
					/>
				</div>
			{/if}
			<section class="pending-list-panel" aria-label="Pending transactions">
				<div class="list-heading">
					<h2>
						{snapshot.observed_count === 0
							? 'No pending transactions observed.'
							: 'Pending transactions'}
					</h2>
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
						<Icon name="layers" size={28} />
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
						<div class="ledger-heading" aria-hidden="true">
							<span>Transaction ID</span><span>Miner-fee outputs</span><span>In → out</span><span
								>Inspect</span
							>
						</div>
						{#each matches as transaction (transaction.id)}
							<article
								class="pending-transaction"
								class:selected-row={selected === transaction.id}
								aria-label={'Pending transaction ' + transaction.id}
							>
								<div class="transaction-main">
									<div class="transaction-identity">
										<a
											class="transaction-id"
											href={'/tx/' + transaction.id}
											aria-label={transaction.id}
											title={transaction.id}
											><span>{transaction.id.slice(0, 10)}…{transaction.id.slice(-8)}</span><Icon
												name="arrow-right"
												size={14}
											/></a
										><span class="pending-label">Pending at observation</span>
									</div>
									<div class="transaction-fee">
										<span class="mobile-label">Miner-fee outputs</span><span
											title={transaction.fee === null ? undefined : transaction.fee + ' nanoERG'}
											>{transaction.fee === null
												? 'Unknown'
												: formatErg(transaction.fee) + ' ERG'}</span
										>
									</div>
									<div class="transaction-counts">
										<span class="mobile-label">Inputs / outputs</span><span
											>{transaction.input_count} <span aria-hidden="true">→</span><span
												class="visually-hidden"
											>
												inputs and
											</span>{transaction.output_count}<span class="visually-hidden">
												outputs</span
											></span
										>
									</div>
									<button
										type="button"
										class="secondary inspect-connections"
										onclick={() => void inspectListed(transaction.id)}
										aria-label={'Inspect connections for listed transaction ' + transaction.id}
										aria-pressed={selected === transaction.id}>Connections</button
									>
								</div>
								<details class="transaction-evidence">
									<summary
										>Details<span class="size-preview" aria-hidden="true">
											· {transaction.size === null
												? 'size not supplied'
												: transaction.size.toLocaleString('en-US') + ' bytes'} · {transaction.data_input_count}
											data inputs</span
										></summary
									>
									<dl>
										<div class="full-id">
											<dt>Transaction ID</dt>
											<dd>{transaction.id}</dd>
										</div>
										<div>
											<dt>Read-only data inputs</dt>
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
										<div>
											<dt>Exact miner-fee outputs</dt>
											<dd>
												{transaction.fee === null
													? 'Unknown'
													: formatNano(transaction.fee) + ' nanoERG'}
											</dd>
										</div>
									</dl>
								</details>
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
		align-items: center;
		justify-content: space-between;
		gap: var(--density-gap, 16px);
		margin-bottom: var(--density-gap, 16px);
	}
	.eyebrow {
		font: 11px var(--font-mono);
		letter-spacing: 0.08em;
		color: var(--fg-muted);
	}
	h1 {
		font: var(--weight-display, 750) var(--density-title, 32px)/1.1
			var(--font-display, var(--font-sans));
		letter-spacing: -0.035em;
		margin-block: 7px;
	}
	.mempool-intro > div > p:last-child {
		max-width: 78ch;
		font-size: 12px;
		line-height: 1.6;
		color: var(--fg-muted);
	}
	button {
		display: inline-flex;
		min-height: 36px;
		align-items: center;
		justify-content: center;
		gap: 8px;
		border: 1px solid var(--btn-fill);
		border-radius: var(--radius-control);
		background: var(--btn-fill);
		color: var(--btn-fill-fg);
		padding: 7px 12px;
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
	h2 {
		font: var(--weight-display, 700) 20px/1.2 var(--font-display, var(--font-sans));
		letter-spacing: -0.025em;
	}
	.mempool-notice {
		padding: var(--density-panel, 16px);
		border: 1px solid var(--hairline);
		border-radius: var(--radius-card);
		background: var(--surface-solid);
	}
	.mempool-notice h2 {
		margin-block: 12px;
	}
	.mempool-notice p {
		font-size: 13px;
		line-height: 1.7;
		max-width: 76ch;
		margin-bottom: 10px;
	}
	.mempool-notice a {
		font-size: 12px;
		display: inline-flex;
		align-items: center;
		min-height: 44px;
	}
	.observation-summary {
		display: flex;
		flex-wrap: wrap;
		align-items: center;
		gap: 8px 24px;
		padding: 10px var(--density-panel, 16px);
		border: 1px solid var(--hairline);
		border-radius: var(--radius-card);
		background: var(--surface-solid);
		margin-bottom: var(--density-gap, 16px);
	}
	.count-block {
		display: flex;
		align-items: baseline;
		gap: 8px;
	}
	.count-block strong {
		font: var(--weight-number, 700) 27px/1 var(--font-number, var(--font-sans));
	}
	.count-block > span,
	.observed-at,
	.scope-note {
		font-size: 11px;
		line-height: 1.6;
		color: var(--fg-muted);
	}
	.observed-at {
		display: grid;
	}
	time {
		font: 11px/1.6 var(--font-mono);
	}
	.observation-evidence {
		min-width: 0;
		flex: 1 1 140px;
	}
	summary {
		min-height: 32px;
		align-content: center;
		font-size: 11px;
		cursor: pointer;
		color: var(--accent-ink);
	}
	.observation-evidence > div {
		max-width: 65ch;
		padding-block: 8px;
		font-size: 11px;
		line-height: 1.7;
		color: var(--fg-muted);
	}
	.scope-note {
		flex: 1 0 100%;
	}
	.mempool-layout,
	.pending-list-panel,
	.connections-column {
		min-width: 0;
	}
	.mempool-layout {
		display: grid;
		gap: var(--density-gap, 16px);
		align-items: start;
	}
	.list-heading {
		display: flex;
		flex-wrap: wrap;
		align-items: center;
		justify-content: space-between;
		gap: 10px 16px;
	}
	.filter {
		display: grid;
		gap: 4px;
		font-size: 11px;
		color: var(--fg-muted);
		width: min(100%, 255px);
	}
	input {
		width: 100%;
		min-width: 0;
		min-height: 36px;
		padding: 7px 10px;
		border: 1px solid var(--hairline);
		border-radius: var(--radius-control);
		background: var(--surface-solid);
		color: var(--fg);
		font: 11px var(--font-mono);
	}
	.list-scope,
	.list-footnote {
		font-size: 11px;
		line-height: 1.6;
		color: var(--fg-muted);
		margin-block: 10px;
	}
	.limit-notice {
		border-left: 2px solid var(--accent-ink);
		background: var(--accent-wash);
		padding: 8px 12px;
		font-size: 12px;
		line-height: 1.6;
		margin-top: 10px;
	}
	.pending-list {
		border: 1px solid var(--hairline);
		border-radius: var(--radius-card);
		overflow: hidden;
	}
	.ledger-heading,
	.transaction-main {
		display: grid;
		grid-template-columns: minmax(0, 1.3fr) minmax(0, 1fr) 60px 90px;
		gap: 10px;
		align-items: center;
	}
	.ledger-heading {
		padding: 8px var(--density-panel, 16px);
		color: var(--fg-muted);
		font-size: 11px;
		background: var(--surface-solid);
		border-bottom: 1px solid var(--hairline);
	}
	.pending-transaction {
		padding: 5px var(--density-panel, 16px) 0;
		background: var(--surface-solid);
		min-width: 0;
	}
	.pending-transaction + .pending-transaction {
		border-top: 1px solid var(--hairline);
	}
	.pending-transaction.selected-row {
		box-shadow: inset 2px 0 var(--accent-ink);
		background: color-mix(in srgb, var(--accent-wash) 40%, var(--surface-solid));
	}
	.transaction-main {
		min-height: var(--density-row, 44px);
	}
	.transaction-identity,
	.transaction-fee,
	.transaction-counts {
		min-width: 0;
	}
	.transaction-id {
		display: flex;
		align-items: center;
		gap: 6px;
		min-height: 24px;
		font: 11px/1.5 var(--font-mono);
	}
	.transaction-id > span {
		overflow: hidden;
		text-overflow: ellipsis;
		white-space: nowrap;
	}
	.transaction-id :global(svg) {
		flex-shrink: 0;
	}
	.pending-label {
		display: block;
		font-size: 11px;
		color: var(--fg-muted);
	}
	.transaction-fee,
	.transaction-counts {
		font: 11px/1.6 var(--font-mono);
		font-variant-numeric: tabular-nums;
		overflow-wrap: anywhere;
	}
	.mobile-label {
		display: none;
	}
	.inspect-connections {
		font-size: 11px;
		min-height: var(--density-row, 44px);
		padding: 5px 8px;
	}
	.transaction-evidence > summary {
		color: var(--fg-muted);
		font-size: 11px;
	}
	.transaction-evidence dl {
		display: grid;
		grid-template-columns: repeat(3, minmax(0, 1fr));
		gap: 10px;
		border-top: 1px dashed var(--hairline);
		padding-block: 10px;
		margin: 0;
	}
	dt {
		font-size: 11px;
		color: var(--fg-muted);
	}
	dd {
		font: 11px/1.6 var(--font-mono);
		margin: 4px 0 0;
		overflow-wrap: anywhere;
	}
	.full-id {
		grid-column: 1 / -1;
	}
	.empty-observation,
	.empty-filter {
		padding: 24px var(--density-panel, 16px);
		border: 1px dashed var(--hairline);
		border-radius: var(--radius-card);
	}
	.empty-observation p {
		font-size: 13px;
		color: var(--fg-muted);
		line-height: 1.7;
		margin-top: 10px;
	}
	.empty-filter h3 {
		font-size: 15px;
		margin-bottom: 12px;
	}
	:global([data-appearance='prism']) .observation-summary {
		background: linear-gradient(140deg, var(--surface-solid), var(--accent-wash));
		border-radius: 16px;
	}
	:global([data-appearance='prism']) .pending-list {
		border: 0;
		background: transparent;
		overflow: visible;
	}
	:global([data-appearance='prism']) .ledger-heading {
		border: 0;
		background: transparent;
	}
	:global([data-appearance='prism']) .pending-transaction {
		margin-block: 6px;
		border: 1px solid var(--hairline);
		border-radius: 10px;
		box-shadow: 0 3px 10px color-mix(in srgb, var(--fg) 4%, transparent);
	}
	:global([data-appearance='prism']) .pending-transaction.selected-row {
		border-color: var(--accent-ink);
	}
	:global([data-appearance='atelier']) .mempool-intro {
		border-bottom: 4px double var(--fg);
		padding-bottom: 12px;
	}
	:global([data-appearance='atelier']) .observation-summary {
		border: 0;
		border-bottom: 1px solid var(--hairline);
		border-radius: 0;
		background: transparent;
		padding-inline: 0;
	}
	:global([data-appearance='atelier']) .pending-list {
		border: 0;
		border-top: 2px solid var(--fg);
		border-radius: 0;
	}
	:global([data-appearance='atelier']) .ledger-heading,
	:global([data-appearance='atelier']) .pending-transaction {
		background: transparent;
		padding-inline: 10px;
	}
	:global([data-appearance='atelier']) .mempool-notice {
		max-width: 760px;
		margin: 24px auto;
		border: 0;
		border-block: 4px double var(--fg);
		border-radius: 0;
		background: transparent;
	}
	:global([data-appearance='aurora']) .mempool-intro {
		justify-content: center;
		text-align: center;
	}
	:global([data-appearance='aurora']) .observation-summary {
		justify-content: center;
		background: var(--accent-wash);
		border-radius: 20px;
	}
	:global([data-appearance='aurora']) .scope-note {
		text-align: center;
	}
	:global([data-appearance='aurora']) .pending-list {
		border-radius: 20px;
		padding: 4px 8px;
		background: var(--surface-solid);
	}
	:global([data-appearance='aurora']) .transaction-main {
		grid-template-columns: 90px minmax(0, 1.3fr) minmax(0, 1fr) 60px;
	}
	:global([data-appearance='aurora']) .ledger-heading {
		grid-template-columns: 90px minmax(0, 1.3fr) minmax(0, 1fr) 60px;
	}
	:global([data-appearance='aurora']) .inspect-connections {
		grid-column: 1;
		grid-row: 1;
		border-radius: var(--radius-pill);
	}
	:global([data-appearance='aurora']) .transaction-identity {
		grid-column: 2;
		grid-row: 1;
	}
	:global([data-appearance='aurora']) .transaction-fee {
		grid-column: 3;
		grid-row: 1;
	}
	:global([data-appearance='aurora']) .transaction-counts {
		grid-column: 4;
		grid-row: 1;
	}
	:global([data-appearance='aurora']) .ledger-heading > :last-child {
		grid-column: 1;
		grid-row: 1;
	}
	:global([data-appearance='aurora']) .ledger-heading > :first-child {
		grid-column: 2;
		grid-row: 1;
	}
	:global([data-appearance='aurora']) .ledger-heading > :nth-child(2) {
		grid-column: 3;
		grid-row: 1;
	}
	:global([data-appearance='aurora']) .ledger-heading > :nth-child(3) {
		grid-column: 4;
		grid-row: 1;
	}
	:global([data-appearance='aurora']) .mempool-notice {
		max-width: 850px;
		margin-inline: auto;
		border-radius: 24px;
		text-align: center;
	}
	@media (min-width: 1000px) {
		.scope-note {
			flex: 1 1 180px;
		}
		:global([data-appearance='atelier']) .mempool-intro {
			padding-bottom: 8px;
			margin-bottom: 10px;
		}
		:global([data-appearance='atelier']) .observation-summary {
			margin-bottom: 10px;
		}
		.with-inspector {
			grid-template-columns: minmax(0, 1fr) minmax(270px, 0.44fr);
		}
		.connections-column {
			grid-column: 2;
			grid-row: 1;
			position: sticky;
			top: 16px;
		}
		.pending-list-panel {
			grid-column: 1;
			grid-row: 1;
		}
		:global([data-appearance='atelier']) .with-inspector {
			grid-template-columns: minmax(250px, 0.38fr) minmax(0, 1fr);
		}
		:global([data-appearance='atelier']) .connections-column {
			grid-column: 1;
		}
		:global([data-appearance='atelier']) .pending-list-panel {
			grid-column: 2;
		}
	}
	@media (max-width: 999px) {
		.mempool-intro {
			align-items: stretch;
			flex-direction: column;
			gap: 10px;
		}
		.mempool-intro > button {
			align-self: start;
		}
		button,
		input,
		summary,
		.transaction-id,
		.inspect-connections {
			min-height: 44px;
		}
		.observation-summary {
			gap: 8px 16px;
		}
		.observation-evidence {
			flex: 1 0 100%;
		}
		.list-heading {
			align-items: stretch;
			flex-direction: column;
		}
		.filter {
			width: 100%;
		}
		.ledger-heading {
			display: none;
		}
		.transaction-main,
		:global([data-appearance='aurora']) .transaction-main {
			grid-template-columns: minmax(0, 1fr) 92px;
			gap: 4px 10px;
		}
		.transaction-identity,
		:global([data-appearance='aurora']) .transaction-identity {
			grid-column: 1;
			grid-row: 1;
		}
		.inspect-connections,
		:global([data-appearance='aurora']) .inspect-connections {
			grid-column: 2;
			grid-row: 1;
		}
		.transaction-fee,
		:global([data-appearance='aurora']) .transaction-fee {
			grid-column: 1;
			grid-row: 2;
			padding-block: 8px;
		}
		.transaction-counts,
		:global([data-appearance='aurora']) .transaction-counts {
			grid-column: 2;
			grid-row: 2;
			padding-block: 8px;
		}
		.mobile-label {
			display: block;
			font: 11px/1.6 var(--font-sans);
			color: var(--fg-muted);
		}
		.pending-label {
			display: none;
		}
		.transaction-evidence dl {
			grid-template-columns: repeat(2, minmax(0, 1fr));
		}
		.size-preview {
			display: none;
		}
		.pending-transaction {
			padding-inline: 10px;
		}
		:global([data-appearance='aurora']) .mempool-intro {
			text-align: left;
		}
	}
</style>
